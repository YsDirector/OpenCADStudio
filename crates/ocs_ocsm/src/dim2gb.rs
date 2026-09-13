//! `OCSMDIM2GB`（短命令 `D2G`）：把宿主原生标注（`DIMENSION`）**重建**为 OCSM 的
//! GB 标准版本——OCSM_GB_x{图框比例} 标注样式 + `7标注层`（尺寸）/ 匿名块（直径、
//! 半径、角度、弧长），文字样式 OCSM_GB、箭头实心闭合。
//!
//! 设计要点：
//! - **纯规划**：`plan()` 只读 `&CadDocument`，产出一份"行动计划"（要建的标注样式、
//!   匿名块、要加/删的实体 + 报告），便于单元测试；命令侧再用 `HostApi` 落盘。
//! - **复用既有构件**：转换本身调用引导标注那套已验证的构造函数（`build_dimension`、
//!   `build_guide_angular_block`、`apply_arclen`）。它们通过 `PluginRequestSender` 向
//!   宿主请求建样式/建块/加实体——这里给一个**本地收集器** `CollectSender` 顶替宿主，
//!   把请求截获成行动计划，因此不需要改动既有代码路径。
//! - **不改宿主主线程**：宿主在主线程 drain 插件请求，命令执行期间同步用真 sender 会
//!   死锁；所以命令只能用 `HostApi` 直连方法落地。
//! - **保留用户所见**：文字覆盖（`base.text`/`user_text`）、小数位（DIMDEC）、样式前后缀
//!   （DIMPOST）、公差（DIMTOL/DIMTP/DIMTM）、手动文字位置（`text_user_positioned`）都
//!   尽量原样带过去；直径/半径/角度/弧长这类"块内字面文字"的形态，优先从原标注自己的
//!   匿名块里读回用户实际看到的文字。
//! - **跳过并报告**：不支持的类型（坐标/折弯半径等）或几何无效的对象不中断整批，记入
//!   报告由命令打印。

use std::sync::{Arc, Mutex};

use ocs_plugin_api::host::acadrust;
use ocs_plugin_api::host::acadrust::entities::{Dimension, DimensionBase, DimensionType};
use ocs_plugin_api::host::acadrust::types::Vector3;
use ocs_plugin_api::host::acadrust::xdata::XDataValue;
use ocs_plugin_api::host::{DimStyleDef, PluginRequestError, PluginRequestSender};
use ocs_plugin_api::ipc::protocol::{PluginRequest, PluginResponse};

use crate::guide_server as gs;
use crate::guide_url::{
    AngleMode, DatumVersion, FlipDir, GuideParams, GuideType, LinearSub, SectionSide, WeldParams,
};

type Doc = acadrust::CadDocument;
type Entity = acadrust::EntityType;
type Handle = acadrust::Handle;

/// 转换行动计划（纯数据，便于测试）。
#[derive(Debug, Default)]
pub(crate) struct Dim2GbPlan {
    /// 需要确保的标注样式（OCSM_GB_x{比例}）。
    pub styles: Vec<DimStyleDef>,
    /// 需要新建的匿名块（名称 + 成员）。
    pub blocks: Vec<(String, Vec<Entity>)>,
    /// 需要新增的实体（重建后的标注 / INSERT）。
    pub adds: Vec<Entity>,
    /// 需要删除的原标注 handle。
    pub removes: Vec<Handle>,
    /// 成功转换数。
    pub converted: usize,
    /// 扫描到的原生标注总数。
    pub seen: usize,
    /// 跳过清单（每条 = 一个对象 + 原因）。
    pub skipped: Vec<String>,
}

impl Dim2GbPlan {
    /// 命令结束时打印的报告文本。
    pub fn report(&self) -> String {
        let mut s = format!(
            "OCSMDIM2GB：扫描原生标注 {} 个，转换成功 {} 个。",
            self.seen, self.converted
        );
        if !self.skipped.is_empty() {
            s.push_str(&format!("跳过 {} 个：", self.skipped.len()));
            s.push_str(&self.skipped.join("；"));
        } else {
            s.push_str("无跳过。");
        }
        if self.converted > 0 {
            s.push_str("（可用一次 Ctrl+Z 全部撤销）");
        }
        s
    }
}

/// 本地收集器：顶替宿主接收构件发出的宿主请求，转成行动计划。
#[derive(Default)]
pub(crate) struct CollectSender {
    styles: Mutex<Vec<DimStyleDef>>,
    blocks: Mutex<Vec<(String, Vec<Entity>)>>,
    adds: Mutex<Vec<Entity>>,
}

impl CollectSender {
    fn drain(&self) -> (Vec<DimStyleDef>, Vec<(String, Vec<Entity>)>, Vec<Entity>) {
        let styles = std::mem::take(&mut *self.styles.lock().unwrap());
        let blocks = std::mem::take(&mut *self.blocks.lock().unwrap());
        let adds = std::mem::take(&mut *self.adds.lock().unwrap());
        (styles, blocks, adds)
    }
}

impl PluginRequestSender for CollectSender {
    fn request(&self, req: PluginRequest) -> Result<PluginResponse, PluginRequestError> {
        use PluginRequest as R;
        use PluginResponse as P;
        match req {
            R::EnsureDimStyles(defs) => {
                if defs.is_empty() {
                    return Ok(P::Count(0));
                }
                self.styles.lock().unwrap().extend(defs);
                Ok(P::Count(0))
            }
            R::AddBlockRecord { name, entities } => {
                self.blocks.lock().unwrap().push((name, entities));
                Ok(P::Ok)
            }
            R::AddEntities(es) => {
                self.adds.lock().unwrap().extend(es);
                Ok(P::Handles(Vec::new()))
            }
            R::AddEntity(e) => {
                self.adds.lock().unwrap().push(e);
                Ok(P::Handle(Handle::new(0)))
            }
            // 收集阶段无需真正生效的请求。
            R::PushUndo { .. }
            | R::SetDirty
            | R::BumpGeometry
            | R::RemoveEntity { .. }
            | R::WriteRecord { .. } => Ok(P::Ok),
            R::ReadRecord { .. } => Ok(P::Record(None)),
            other => Err(PluginRequestError(format!(
                "dim2gb 收集器不支持该请求: {other:?}"
            ))),
        }
    }
}

// ── 小工具 ────────────────────────────────────────────────────────────────

fn to_arr(v: Vector3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

fn axis_perp(axis: Vector3) -> Vector3 {
    Vector3::new(-axis.y, axis.x, 0.0)
}

fn type_label(dim: &Dimension) -> &'static str {
    match dim {
        Dimension::Linear(l) => match l.base.dimension_type {
            DimensionType::Aligned => "对齐标注",
            _ => "线性标注",
        },
        Dimension::Aligned(_) => "对齐标注",
        Dimension::Radius(_) => "半径标注",
        Dimension::Diameter(_) => "直径标注",
        Dimension::Angular2Ln(_) | Dimension::Angular3Pt(_) => "角度标注",
        Dimension::Arc(_) => "弧长标注",
        Dimension::Ordinate(_) => "坐标标注",
        Dimension::LargeRadial(_) => "折弯半径标注",
    }
}

/// 原标注的有效小数位：实体 XDATA DSTYLE(271) > 样式表 DIMDEC > 2。
fn effective_dec(doc: &Doc, base: &DimensionBase) -> u32 {
    let recs = base.common.extended_data.records();
    for rec in recs {
        if rec.application_name != "ACAD" {
            continue;
        }
        for w in rec.values.windows(2) {
            if matches!(w[0], XDataValue::Integer16(271)) {
                if let XDataValue::Integer16(v) = w[1] {
                    if v >= 0 {
                        return v as u32;
                    }
                }
            }
        }
    }
    if let Some(st) = doc
        .dim_styles
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(&base.style_name))
    {
        if st.dimdec >= 0 {
            return st.dimdec as u32;
        }
    }
    2
}

/// 原标注所属样式（若在表内）。
fn source_style<'a>(doc: &'a Doc, base: &DimensionBase) -> Option<&'a acadrust::tables::DimStyle> {
    doc.dim_styles
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(&base.style_name))
}

/// 用户在该标注上真正看到的文字：优先取它自己匿名块里的文字（TEXT/MTEXT），
/// 其次是实体文字覆盖（`user_text` / `text`）。
fn displayed_text(doc: &Doc, base: &DimensionBase) -> Option<String> {
    let block_name = base.block_name.trim();
    if !block_name.is_empty() {
        if let Some(br) = doc
            .block_records
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(block_name))
        {
            for h in &br.entity_handles {
                match doc.get_entity(*h) {
                    Some(Entity::Text(t)) if !t.value.trim().is_empty() => {
                        return Some(t.value.clone());
                    }
                    Some(Entity::MText(m)) if !m.value.trim().is_empty() => {
                        return Some(m.value.clone());
                    }
                    _ => {}
                }
            }
        }
    }
    if let Some(t) = &base.user_text {
        if !t.trim().is_empty() {
            return Some(t.clone());
        }
    }
    if !base.text.trim().is_empty() {
        return Some(base.text.clone());
    }
    None
}

/// 文字参数：实体覆盖 > 样式前后缀（DIMPOST，如 `M<>` / `<>)mm`）。
fn text_param(doc: &Doc, base: &DimensionBase) -> Option<String> {
    if let Some(t) = displayed_text(doc, base) {
        return Some(t);
    }
    if let Some(st) = source_style(doc, base) {
        if !st.dimpost.trim().is_empty() {
            return Some(st.dimpost.clone());
        }
    }
    None
}

/// 读实体 XDATA（ACAD/DSTYLE）里的某个覆盖值。
fn xdata_int(base: &DimensionBase, code: i16) -> Option<i16> {
    for rec in base.common.extended_data.records() {
        if rec.application_name != "ACAD" {
            continue;
        }
        for w in rec.values.windows(2) {
            if matches!(w[0], XDataValue::Integer16(c) if c == code) {
                if let XDataValue::Integer16(v) = w[1] {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn xdata_real(base: &DimensionBase, code: i16) -> Option<f64> {
    for rec in base.common.extended_data.records() {
        if rec.application_name != "ACAD" {
            continue;
        }
        for w in rec.values.windows(2) {
            if matches!(w[0], XDataValue::Integer16(c) if c == code) {
                if let XDataValue::Real(v) = w[1] {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// 偏差文本：按 `dec` 位小数格式化后**抹尾零**；四舍五入结果为 0 时输出纯 `"0"`
/// （不带正负号与小数点，例：0.0000 → `0`、-0.0 → `0`）；非零时带 `sign`（+ / −）。
fn deviation_text(value: f64, dec: usize, sign: char) -> String {
    let dec = dec.min(8);
    // 先按 dec 位量化：丢弃 0.0000xxx 这类数值噪声与 -0.0 的符号。
    let factor = 10f64.powi(dec as i32);
    let q = (value * factor).round() / factor;
    if q.abs() < 1e-12 {
        return "0".to_string();
    }
    let mut s = format!("{:.*}", dec, q.abs());
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    format!("{sign}{s}")
}

/// 公差（极限偏差）→ 公差堆叠参数。取值优先级与宿主一致：
/// **实体 XDATA DSTYLE 覆盖**（DIMTOL 71 / DIMTP 47 / DIMTM 48 / DIMTDEC 272）
/// 优先于**样式表**（dimtol/dimtp/dimtm/dimtdec）——OCS 自带公差正是写在实体覆盖里，
/// 只读样式会全部丢掉（用户实测反馈）。
/// 返回 (上偏差, 下偏差) 文本；无公差时 (None, None)。
fn tol_params(doc: &Doc, base: &DimensionBase) -> (Option<String>, Option<String>) {
    let st = source_style(doc, base);
    let dimtol = xdata_int(base, 71)
        .map(|v| v != 0)
        .or_else(|| st.map(|s| s.dimtol))
        .unwrap_or(false);
    let dimlim = xdata_int(base, 72)
        .map(|v| v != 0)
        .or_else(|| st.map(|s| s.dimlim))
        .unwrap_or(false);
    if !dimtol && !dimlim {
        return (None, None);
    }
    let tp = xdata_real(base, 47)
        .or_else(|| st.map(|s| s.dimtp))
        .unwrap_or(0.0);
    let tm = xdata_real(base, 48)
        .or_else(|| st.map(|s| s.dimtm))
        .unwrap_or(0.0);
    let dec = xdata_int(base, 272)
        .or_else(|| st.map(|s| s.dimtdec))
        .map(|v| v.max(0) as usize)
        .unwrap_or(2)
        .min(8);
    if tp.abs() <= 1e-12 && tm.abs() <= 1e-12 {
        return (None, None);
    }
    // 偏差文本：抹尾零；值按 `dec` 位四舍五入为 0 时输出纯 "0"（不带正负号、不带小数，
    // 例如下差 0.0000 → "0" 而不是 "-0.00"）。非零时显式带号（上 +、下 −）。
    let up = deviation_text(tp, dec, '+');
    let dn = deviation_text(tm, dec, '-');
    (Some(up), Some(dn))
}

/// 该点附近是否有同心的圆/弧（用于直径标注的中心点消歧）。
fn circle_near(doc: &Doc, center: Vector3, radius: f64) -> bool {
    let tol = (radius.abs() * 1e-6).max(1e-6);
    doc.entities().any(|e| match e {
        Entity::Circle(c) => {
            c.center.distance(&center) < tol && (c.radius - radius).abs() < tol
        }
        Entity::Arc(a) => a.center.distance(&center) < tol && (a.radius - radius).abs() < tol,
        _ => false,
    })
}

/// 线性子类型：优先按测量值匹配，其次按尺寸线旋转角。
fn pick_linear_sub(dim: &Dimension, first: Vector3, second: Vector3, measured: f64) -> LinearSub {
    if matches!(dim, Dimension::Aligned(_)) {
        return LinearSub::Aligned;
    }
    let rotation = match dim {
        Dimension::Linear(l) => l.rotation,
        _ => 0.0,
    };
    // 旋转角决定候选次序：0 → 水平，±90° → 竖直，其余按测量值挑。
    let mut cands = vec![
        (LinearSub::Horizontal, (second.x - first.x).abs()),
        (LinearSub::Vertical, (second.y - first.y).abs()),
    ];
    let r = rotation.rem_euclid(std::f64::consts::PI);
    let prefer_vertical = (r - std::f64::consts::FRAC_PI_2).abs() < 1e-6;
    if prefer_vertical {
        cands.swap(0, 1);
    }
    if measured > 0.0 {
        for (s, v) in &cands {
            if (v - measured).abs() <= measured.abs() * 1e-6 + 1e-9 {
                return *s;
            }
        }
        cands.sort_by(|a, b| {
            (a.1 - measured)
                .abs()
                .partial_cmp(&(b.1 - measured).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    cands[0].0
}

/// 构造 `GuideParams`（只填本次转换用得到的字段，其余用保守默认值）。
fn base_params(guide_type: GuideType, dist: f64, dec: Option<u32>) -> GuideParams {
    GuideParams {
        guide_type,
        sub: None,
        dist,
        text: None,
        tol: None,
        up: None,
        dn: None,
        fit: None,
        sym: None,
        dec,
        ver: DatumVersion::GB2008,
        letter: None,
        scale: None,
        flip: FlipDir::None,
        marker: None,
        angle_mode: AngleMode::Minor,
        section_side: SectionSide::Right,
        show_arrow: true,
        gdt_sym: None,
        gdt_dia: false,
        gdt_tol: None,
        gdt_d1: None,
        gdt_d2: None,
        gdt_d3: None,
        gdt_rows: Vec::new(),
        gdt_top: None,
        gdt_bot: None,
        detail_scale: 1.0,
        detail_no: None,
        detail_pos: None,
        detail_frame: 1.0,
        weld: WeldParams::default(),
    }
}

// ── 逐类型转换 ────────────────────────────────────────────────────────────

/// 线性 / 对齐标注 → OCSM 原生 DIMENSION（OCSM_GB_x{比例} + 7标注层）。
fn convert_linear(
    c: &Arc<dyn PluginRequestSender>,
    doc: &Doc,
    base: &DimensionBase,
    first: Vector3,
    second: Vector3,
    def: Vector3,
    sub: LinearSub,
    style: &str,
) -> Result<Entity, String> {
    // 尺寸线偏移：构件内部一律 pt = second + perp*dist，其中 perp ⊥ (first→second)。
    // 水平/竖直子类型只要求尺寸线平行于坐标轴，故按"让尺寸线过 def 点"反解 dist。
    let axis = (second - first).normalize();
    let perp = axis_perp(axis);
    let delta = def - second;
    let dist = match sub {
        LinearSub::Aligned => delta.dot(&perp),
        LinearSub::Horizontal => {
            if perp.y.abs() > 1e-9 {
                delta.y / perp.y
            } else {
                0.0
            }
        }
        LinearSub::Vertical => {
            if perp.x.abs() > 1e-9 {
                delta.x / perp.x
            } else {
                0.0
            }
        }
    };

    let mut params = base_params(GuideType::Linear, dist, Some(effective_dec(doc, base)));
    params.sub = Some(sub);
    params.text = text_param(doc, base);
    let (up, dn) = tol_params(doc, base);
    params.up = up;
    params.dn = dn;

    let mut dim = gs::build_dimension(c, doc, to_arr(first), to_arr(second), &params, style)?;
    // 手动拖过的文字位置原样保留。
    if base.text_user_positioned {
        let b = dim.base_mut();
        b.text_middle_point = base.text_middle_point;
        b.insertion_point = base.insertion_point;
        b.text_user_positioned = true;
        if base.text_rotation.abs() > 1e-9 {
            b.text_rotation = base.text_rotation;
        }
    }
    Ok(Entity::Dimension(dim))
}

/// 半径 / 直径 → OCSM 匿名块形态（块内 MTEXT + 7标注层）。
fn convert_radial(
    c: &Arc<dyn PluginRequestSender>,
    doc: &Doc,
    base: &DimensionBase,
    center: Vector3,
    rim: Vector3,
    is_diameter: bool,
    style: &str,
) -> Result<Entity, String> {
    let radius = center.distance(&rim);
    if radius < 1e-9 {
        return Err("半径为零".into());
    }
    // 文字锚点：原标注的尺寸线点（用户拖放处）。
    let text_pt = base.text_middle_point;
    let d_text = text_pt.distance(&center);
    let inside = d_text < radius;
    let dist = if inside {
        -text_pt.distance(&rim)
    } else {
        text_pt.distance(&rim)
    };

    let gt = if is_diameter {
        GuideType::Diameter
    } else {
        GuideType::Radius
    };
    let mut params = base_params(gt, dist, Some(effective_dec(doc, base)));
    params.text = displayed_text(doc, base);
    let (up, dn) = tol_params(doc, base);
    params.up = up;
    params.dn = dn;

    let dim = gs::build_dimension(c, doc, to_arr(center), to_arr(rim), &params, style)?;
    Ok(Entity::Dimension(dim))
}

/// 角度 → OCSM 匿名块形态（弧 + 箭头 + MTEXT）。
fn convert_angular(
    c: &Arc<dyn PluginRequestSender>,
    doc: &Doc,
    base: &DimensionBase,
    vertex: Vector3,
    ray1: Vector3,
    ray2: Vector3,
    arc_pt: Vector3,
    style: &str,
) -> Result<Entity, String> {
    let scale = crate::frame_scale_at(doc, to_arr(vertex));
    let gap = 1.0 * scale;
    // 文字锚点距顶点距离；过小则抬到 arc 半径上（构件要求 > DIMGAP）。
    let d_text = base.text_middle_point.distance(&vertex);
    let d_arc = arc_pt.distance(&vertex);
    let mut dist = if d_text > gap + 1e-6 { d_text } else { d_arc };
    if dist <= gap {
        dist = gap + 1.25 * scale;
    }
    let mut params = base_params(GuideType::Angle, dist, Some(effective_dec(doc, base)));
    params.text = displayed_text(doc, base);

    let dim = gs::build_guide_angular_block(
        c,
        doc,
        to_arr(ray1),
        to_arr(vertex),
        to_arr(ray2),
        &params,
        style,
    )?;
    Ok(Entity::Dimension(dim))
}

/// 弧长 → OCSM 匿名块 + INSERT（块成员含尺寸弧/界线/箭头/文字）。
fn convert_arclen(
    c: &Arc<dyn PluginRequestSender>,
    doc: &Doc,
    base: &DimensionBase,
    center: Vector3,
    first_ext: Vector3,
    second_ext: Vector3,
    start_angle: f64,
    end_angle: f64,
) -> Result<Option<Entity>, String> {
    let radius = center.distance(&first_ext);
    if radius < 1e-9 {
        return Err("弧长标注半径为零".into());
    }
    let def = base.definition_point;
    let d_def = def.distance(&center);
    let dist = if d_def >= radius {
        d_def - radius
    } else {
        -(radius - d_def)
    };
    let mut params = base_params(GuideType::ArcLen, dist, Some(effective_dec(doc, base)));
    params.text = displayed_text(doc, base);
    let (up, dn) = tol_params(doc, base);
    params.up = up;
    params.dn = dn;

    // 在临时文档里造出引导 ARC（apply_arclen 的输入是引导弧 handle）。
    let mut scratch = doc.clone();
    let mut arc = acadrust::entities::Arc::from_center_radius_angles(
        center,
        radius,
        start_angle,
        end_angle,
    );
    arc.common.layer = "10引导线层".into();
    let handle = scratch
        .add_entity(Entity::Arc(arc))
        .map_err(|e| format!("临时引导弧创建失败: {e:?}"))?;
    let json = gs::apply_arclen(c, &scratch, handle, &params)?;
    let _ = json;
    let _ = second_ext;
    // apply_arclen 内部经收集器 AddEntities 加实体；这里不再返回实体。
    Ok(None)
}

/// 单条标注 → 新实体。`Ok(None)` = 新实体已通过收集器的 AddEntities 通道产出
/// （弧长走 `apply_arclen`，它自己经收集器加 INSERT）。
fn convert_one(
    c: &Arc<dyn PluginRequestSender>,
    doc: &Doc,
    dim: &Dimension,
) -> Result<Option<Entity>, String> {
    let base = dim.base();
    // 图框比例样式按标注自己的尺寸线位置判定。
    let anchor = base.definition_point;
    let style = gs::ensure_style_for_point(c, doc, to_arr(anchor))?;

    match dim {
        Dimension::Linear(l) => {
            let sub = pick_linear_sub(
                dim,
                l.first_point,
                l.second_point,
                base.actual_measurement,
            );
            convert_linear(
                c, doc, base, l.first_point, l.second_point, l.definition_point, sub, &style,
            )
            .map(Some)
        }
        Dimension::Aligned(a) => convert_linear(
            c,
            doc,
            base,
            a.first_point,
            a.second_point,
            a.definition_point,
            LinearSub::Aligned,
            &style,
        )
        .map(Some),
        Dimension::Radius(r) => {
            let mut rim = r.definition_point;
            let mut radius = r.angle_vertex.distance(&rim);
            let m = base.actual_measurement;
            if m > 1e-9 && (radius - m).abs() > m * 1e-6 {
                // 实体只存了圆心与半径时，按测量值把圆周点摆正。
                let dir = (rim - r.angle_vertex).normalize();
                rim = r.angle_vertex + dir * m;
                radius = m;
            }
            if radius < 1e-9 {
                return Err("半径为零".into());
            }
            convert_radial(c, doc, base, r.angle_vertex, rim, false, &style).map(Some)
        }
        Dimension::Diameter(d) => {
            let a = d.angle_vertex;
            let b = d.definition_point;
            let mid = (a + b) * 0.5;
            let dist_ab = a.distance(&b);
            if dist_ab < 1e-9 {
                return Err("直径两点重合".into());
            }
            // 两种常见写法：
            //  ① 宿主画法：两点 = 圆周上的一对对径点（圆心 = 中点）
            //  ② acadrust 写法：angle_vertex = 圆心、definition_point = 圆周点
            let (center, rim) = if circle_near(doc, mid, dist_ab * 0.5) {
                (mid, a)
            } else if circle_near(doc, a, dist_ab) {
                (a, b)
            } else {
                // 无参照圆：按宿主画法（对径点），测量值校验。
                let m = base.actual_measurement;
                if m > 1e-9 && (dist_ab - m).abs() <= m * 1e-6 {
                    (a, b) // 两点距离本身 = 直径 → ② 圆心写法
                } else {
                    (mid, a)
                }
            };
            convert_radial(c, doc, base, center, rim, true, &style).map(Some)
        }
        Dimension::Angular2Ln(g) => {
            let vertex = g.angle_vertex;
            let ray1 = g.second_point;
            let ray2 = pick_second_ray(base, vertex, ray1, g.definition_point, g.dimension_arc);
            convert_angular(c, doc, base, vertex, ray1, ray2, g.dimension_arc, &style)
                .map(Some)
        }
        Dimension::Angular3Pt(g) => {
            let vertex = g.angle_vertex;
            convert_angular(
                c,
                doc,
                base,
                vertex,
                g.first_point,
                g.second_point,
                g.definition_point,
                &style,
            )
            .map(Some)
        }
        Dimension::Arc(a) => {
            if a.center_point.distance(&a.first_extension_point) < 1e-9 {
                return Err("弧长标注尺寸无效（可能来自旧文件解析）".into());
            }
            let start = if a.arc_start_parameter.is_finite() {
                a.arc_start_parameter
            } else {
                (a.first_extension_point.y - a.center_point.y)
                    .atan2(a.first_extension_point.x - a.center_point.x)
            };
            let end = if a.arc_end_parameter.is_finite() {
                a.arc_end_parameter
            } else {
                (a.second_extension_point.y - a.center_point.y)
                    .atan2(a.second_extension_point.x - a.center_point.x)
            };
            convert_arclen(
                c,
                doc,
                base,
                a.center_point,
                a.first_extension_point,
                a.second_extension_point,
                start,
                end,
            )
        }
        Dimension::Ordinate(_) => Err("坐标标注本期不转换（OCSM 无对应构件）".into()),
        Dimension::LargeRadial(_) => Err("折弯半径本期不转换（OCSM 无对应构件）".into()),
    }
}

/// 角度（两线式）第二边取点：`definition_point` 与 `dimension_arc` 里挑一个
/// 能让实测角最接近原值的（DXF 组 16/10 语义在不同写入方下不一致）。
fn pick_second_ray(
    base: &DimensionBase,
    vertex: Vector3,
    ray1: Vector3,
    cand_a: Vector3,
    cand_b: Vector3,
) -> Vector3 {
    let dir1 = ray1 - vertex;
    let angle_of = |p: Vector3| {
        let d = p - vertex;
        if d.length() < 1e-9 {
            return None;
        }
        let cross = dir1.x * d.y - dir1.y * d.x;
        let dot = dir1.x * d.x + dir1.y * d.y;
        Some(cross.atan2(dot).abs().to_degrees())
    };
    let m = base.actual_measurement;
    let a = angle_of(cand_a);
    let b = angle_of(cand_b);
    match (a, b) {
        (Some(a), Some(b)) => {
            if m > 1e-9 {
                if (a - m).abs() <= (b - m).abs() {
                    cand_a
                } else {
                    cand_b
                }
            } else if a <= 180.0 {
                cand_a
            } else {
                cand_b
            }
        }
        (Some(_), None) => cand_a,
        (None, Some(_)) => cand_b,
        (None, None) => cand_a,
    }
}

/// 把已收集的匿名块名补进 scratch 文档：构件按文档里 `*D{n}` 的最大序号取名，
/// 若不同步，多次转换会算出同一个块名（互相覆盖）。
fn sync_collected_blocks(scratch: &mut Doc, collector: &CollectSender) {
    let names: Vec<String> = collector
        .blocks
        .lock()
        .unwrap()
        .iter()
        .map(|(n, _)| n.clone())
        .collect();
    for name in names {
        if scratch.block_records.get(&name).is_none() {
            let mut br = acadrust::tables::BlockRecord::new(&name);
            br.handle = scratch.allocate_handle();
            let _ = scratch.block_records.add(br);
        }
    }
}

/// 规划整批转换：`selected` 为空 = 全图（模型空间）扫描。
pub(crate) fn plan(doc: &Doc, selected: &[Handle]) -> Dim2GbPlan {
    let collector = Arc::new(CollectSender::default());
    let c_dyn: Arc<dyn PluginRequestSender> = collector.clone();
    let mut plan = Dim2GbPlan::default();

    // 选中集里的非标注对象也计入跳过报告。
    if !selected.is_empty() {
        for h in selected {
            match doc.get_entity(*h) {
                Some(Entity::Dimension(_)) => {}
                Some(other) => plan.skipped.push(format!(
                    "handle {} 非标注对象（{}）",
                    h.value(),
                    entity_kind(other)
                )),
                None => plan.skipped.push(format!("handle {} 不存在", h.value())),
            }
        }
    }

    let targets: Vec<(Handle, Dimension)> = doc
        .entities()
        .filter(|e| matches!(e, Entity::Dimension(_)))
        .filter(|e| selected.is_empty() || selected.contains(&e.common().handle))
        .map(|e| {
            let h = e.common().handle;
            let Entity::Dimension(d) = e else {
                unreachable!()
            };
            (h, d.clone())
        })
        .collect();

    // 构件读文档：用一份 scratch 副本，并把已收集的块名回灌，保证块名不重复。
    let mut scratch = doc.clone();
    for (handle, dim) in targets {
        plan.seen += 1;
        let label = type_label(&dim);
        sync_collected_blocks(&mut scratch, &collector);
        let check = convert_one(&c_dyn, &scratch, &dim).and_then(|out| {
            // 几何自检：线性/对齐测量值必须一致；角度允许补角（原标注画的是优角时）。
            if let Some(entity) = &out {
                if let Entity::Dimension(new) = entity {
                    let before = dim.base().actual_measurement;
                    let after = new.base().actual_measurement;
                    let tol = before.abs() * 1e-6 + 1e-9;
                    let ok = match dim {
                        Dimension::Linear(_) | Dimension::Aligned(_) => (after - before).abs() <= tol,
                        Dimension::Angular2Ln(_) | Dimension::Angular3Pt(_) => {
                            (after - before).abs() <= tol
                                || (after - (360.0 - before)).abs() <= tol.max(1e-6)
                        }
                        _ => true,
                    };
                    if !ok {
                        return Err(format!("几何自检未通过（原 {before:.4} → 新 {after:.4}）"));
                    }
                }
            }
            Ok(out)
        });
        match check {
            Ok(Some(entity)) => {
                plan.adds.push(entity);
                plan.removes.push(handle);
                plan.converted += 1;
            }
            // 弧长：实体已由收集器的 AddEntities 通道产出，这里只记删原实体。
            Ok(None) => {
                plan.removes.push(handle);
                plan.converted += 1;
            }
            Err(e) => plan.skipped.push(format!("{label}（{e}）")),
        }
    }

    let (styles, blocks, adds) = collector.drain();
    plan.styles = styles;
    if plan.converted > 0 {
        // 基线样式 OCSM_GB（无图框比例时的落点）一并确保存在。
        if let Some(base) = crate::dim_style_defs().into_iter().next() {
            plan.styles.push(base);
        }
    }
    plan.blocks = blocks;
    plan.adds.extend(adds);
    plan
}

fn entity_kind(e: &Entity) -> &'static str {
    match e {
        Entity::Point(_) => "点",
        Entity::Line(_) => "直线",
        Entity::Circle(_) => "圆",
        Entity::Arc(_) => "圆弧",
        Entity::Polyline(_) | Entity::LwPolyline(_) => "多段线",
        Entity::Text(_) => "文字",
        Entity::MText(_) => "多行文字",
        Entity::Insert(_) => "块参照",
        Entity::Dimension(_) => "标注",
        Entity::Solid(_) => "实体填充",
        _ => "其它",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acadrust::entities::{
        Circle, DimensionAligned, DimensionAngular2Ln, DimensionArc, DimensionDiameter,
        DimensionLinear, DimensionOrdinate, DimensionRadius, MText,
    };

    fn doc_with(entities: Vec<Entity>) -> Doc {
        let mut doc = Doc::new();
        for e in entities {
            doc.add_entity(e).unwrap();
        }
        doc
    }

    /// 原标注文字覆盖（DXF 组 1）。
    fn set_override(dim: &mut Dimension, text: &str) {
        dim.base_mut().text = text.to_string();
        dim.base_mut().user_text = Some(text.to_string());
    }

    fn read_dec(dim: &Dimension) -> Option<i16> {
        let rec = dim.base().common.extended_data.get_record("ACAD")?;
        for w in rec.values.windows(2) {
            if matches!(w[0], XDataValue::Integer16(271)) {
                if let XDataValue::Integer16(v) = w[1] {
                    return Some(v);
                }
            }
        }
        None
    }

    fn block_text(block: &(String, Vec<Entity>)) -> String {
        block
            .1
            .iter()
            .find_map(|e| match e {
                Entity::MText(m) => Some(m.value.clone()),
                Entity::Text(t) => Some(t.value.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// 线性标注：重建为 OCSM_GB 样式 + 7标注层，并保留小数位。
    #[test]
    fn plan_converts_linear_dim_to_gb_dimstyle() {
        let mut d = DimensionLinear::horizontal(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(40.0, 0.0, 0.0),
        );
        d.definition_point = Vector3::new(20.0, 12.0, 0.0);
        d.base.definition_point = d.definition_point;
        d.base.text_middle_point = Vector3::new(20.0, 12.0, 0.0);
        d.base.actual_measurement = 40.0;
        d.base.style_name = "Standard".into();
        let mut doc = Doc::new();
        let handle = doc
            .add_entity(Entity::Dimension(Dimension::Linear(d)))
            .unwrap();

        let plan = plan(&doc, &[]);
        assert_eq!(plan.seen, 1, "扫描到 1 个标注");
        assert_eq!(plan.converted, 1, "转换成功 1 个");
        assert_eq!(plan.removes, vec![handle], "删除原标注");
        assert!(plan.skipped.is_empty(), "无跳过: {:?}", plan.skipped);

        let Entity::Dimension(new) = &plan.adds[0] else {
            panic!("产物应为 DIMENSION");
        };
        assert_eq!(new.base().style_name, "OCSM_GB", "样式 → OCSM_GB");
        assert_eq!(new.base().common.layer, "7标注层", "图层 → 7标注层");
        assert_eq!(read_dec(new), Some(2), "小数位保留（无样式表 → 兜底 2）");
        // 尺寸线位置保持：def 点法向偏移 = 12。
        assert!(
            (new.base().definition_point.y - 12.0).abs() < 1e-9,
            "尺寸线位置保持: {:?}",
            new.base().definition_point
        );
    }

    /// 文字覆盖 + 样式前后缀 + 小数位保留。
    #[test]
    fn plan_preserves_text_dec_and_dimpost() {
        let mut style = acadrust::tables::DimStyle::new("STD2");
        style.dimdec = 3;
        style.dimpost = "M<>".into();
        let mut doc = Doc::new();
        doc.dim_styles.add_or_replace(style);

        let mut d = DimensionLinear::horizontal(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(30.0, 0.0, 0.0),
        );
        d.definition_point = Vector3::new(15.0, 8.0, 0.0);
        d.base.actual_measurement = 30.0;
        d.base.style_name = "STD2".into();
        doc.add_entity(Entity::Dimension(Dimension::Linear(d))).unwrap();

        let plan_a = plan(&doc, &[]);
        let Entity::Dimension(new) = &plan_a.adds[0] else {
            panic!()
        };
        assert_eq!(new.base().text, "M<>", "样式 DIMPOST 带过来了");
        assert_eq!(read_dec(new), Some(3), "DIMDEC=3 保留");

        // 有实体文字覆盖时，覆盖优先于 DIMPOST。
        let mut doc2 = Doc::new();
        doc2.dim_styles.add_or_replace({
            let mut s = acadrust::tables::DimStyle::new("STD2");
            s.dimdec = 1;
            s.dimpost = "M<>".into();
            s
        });
        let mut d2 = DimensionLinear::horizontal(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(30.0, 0.0, 0.0),
        );
        d2.definition_point = Vector3::new(15.0, 8.0, 0.0);
        d2.base.actual_measurement = 30.0;
        d2.base.style_name = "STD2".into();
        let mut d2 = d2;
        d2.base.text = "自定义".into();
        d2.base.user_text = Some("自定义".into());
        doc2.add_entity(Entity::Dimension(Dimension::Linear(d2))).unwrap();
        let plan_b = plan(&doc2, &[]);
        let Entity::Dimension(new2) = &plan_b.adds[0] else {
            panic!()
        };
        assert_eq!(new2.base().text, "自定义", "实体覆盖优先");
    }

    /// 直径标注（宿主画法：两点为对径圆周点）→ 匿名块形态，测量值取对径距离/2×2。
    #[test]
    fn plan_converts_diameter_two_rim_points() {
        let mut doc = Doc::new();
        let mut circle = Circle::new();
        circle.center = Vector3::new(0.0, 0.0, 0.0);
        circle.radius = 10.0;
        doc.add_entity(Entity::Circle(circle)).unwrap();
        // 宿主 DimensionDiameter::new(chord, far_chord)：angle_vertex=chord、
        // definition_point=far_chord（测量值在宿主里被算成 2× 直径）。
        let mut d = DimensionDiameter::new(
            Vector3::new(10.0, 0.0, 0.0),
            Vector3::new(-10.0, 0.0, 0.0),
        );
        d.base.text_middle_point = Vector3::new(14.0, 0.0, 0.0);
        d.base.definition_point = d.base.text_middle_point;
        d.base.actual_measurement = 40.0; // 宿主的 2× 值
        d.base.style_name = "Standard".into();
        doc.add_entity(Entity::Dimension(Dimension::Diameter(d))).unwrap();

        let plan = plan(&doc, &[]);
        assert_eq!(plan.converted, 1, "直径转换成功: {:?}", plan.skipped);
        assert_eq!(plan.blocks.len(), 1, "建了 1 个匿名块");
        let text = block_text(&plan.blocks[0]);
        assert!(
            text.contains("Ø20"),
            "直径值按真实几何算出 20（修正宿主 2× 值），实际 {text}"
        );
    }

    /// 半径标注（圆心 + 圆周点）→ 匿名块，文字 R+半径。
    #[test]
    fn plan_converts_radius_dim() {
        let mut d = DimensionRadius::new(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 25.0, 0.0),
        );
        d.base.text_middle_point = Vector3::new(0.0, 30.0, 0.0);
        d.base.definition_point = d.base.text_middle_point;
        d.base.actual_measurement = 25.0;
        d.base.style_name = "Standard".into();
        let doc = doc_with(vec![Entity::Dimension(Dimension::Radius(d))]);

        let plan = plan(&doc, &[]);
        assert_eq!(plan.converted, 1, "半径转换成功: {:?}", plan.skipped);
        let text = block_text(&plan.blocks[0]);
        assert!(text.contains('R'), "半径带 R 前缀，实际 {text}");
        assert!(text.contains("25"), "半径值 25，实际 {text}");
    }

    /// 角度标注：原文字（用户所见）原样进块内 MTEXT。
    #[test]
    fn plan_angular_keeps_displayed_text() {
        let mut g = DimensionAngular2Ln::default();
        g.angle_vertex = Vector3::new(0.0, 0.0, 0.0);
        g.second_point = Vector3::new(10.0, 0.0, 0.0);
        g.first_point = Vector3::new(0.0, 0.0, 0.0);
        g.definition_point = Vector3::new(0.0, 10.0, 0.0);
        g.dimension_arc = Vector3::new(7.0, 7.0, 0.0);
        g.base.definition_point = g.dimension_arc;
        g.base.text_middle_point = Vector3::new(8.0, 8.0, 0.0);
        g.base.actual_measurement = 90.0;
        g.base.style_name = "Standard".into();
        g.base.text = "90°".into();
        let doc = doc_with(vec![Entity::Dimension(Dimension::Angular2Ln(g))]);

        let plan = plan(&doc, &[]);
        assert_eq!(plan.converted, 1, "角度转换成功: {:?}", plan.skipped);
        let text = block_text(&plan.blocks[0]);
        assert_eq!(text, "90°", "用户所见文字原样保留");
    }

    /// 弧长标注：走 apply_arclen（收集器 AddEntities 通道）+ 建块。
    #[test]
    fn plan_converts_arclen_dim() {
        let mut a = DimensionArc::default();
        a.center_point = Vector3::new(0.0, 0.0, 0.0);
        a.first_extension_point = Vector3::new(20.0, 0.0, 0.0);
        a.second_extension_point = Vector3::new(0.0, 20.0, 0.0);
        a.arc_start_parameter = 0.0;
        a.arc_end_parameter = std::f64::consts::FRAC_PI_2;
        a.definition_point = Vector3::new(24.0, 0.0, 0.0);
        a.base.definition_point = a.definition_point;
        a.base.text_middle_point = Vector3::new(24.0, 8.0, 0.0);
        a.base.actual_measurement = 20.0 * std::f64::consts::FRAC_PI_2;
        a.base.style_name = "Standard".into();
        let doc = doc_with(vec![Entity::Dimension(Dimension::Arc(a))]);

        let plan = plan(&doc, &[]);
        assert_eq!(plan.converted, 1, "弧长转换成功: {:?}", plan.skipped);
        assert_eq!(plan.blocks.len(), 1, "建了 1 个匿名块");
        assert!(
            plan.adds
                .iter()
                .any(|e| matches!(e, Entity::Insert(_))),
            "产出 INSERT"
        );
        assert_eq!(plan.removes.len(), 1, "删除原弧长标注");
    }

    /// 不支持的类型：跳过并给出原因，不阻断整批。
    #[test]
    fn plan_skips_unsupported_and_reports() {
        let mut o = DimensionOrdinate::new(
            Vector3::new(5.0, 0.0, 0.0),
            Vector3::new(5.0, 10.0, 0.0),
            true,
        );
        o.base.style_name = "Standard".into();
        let mut l = DimensionLinear::horizontal(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(10.0, 0.0, 0.0),
        );
        l.definition_point = Vector3::new(5.0, 5.0, 0.0);
        l.base.actual_measurement = 10.0;
        l.base.style_name = "Standard".into();
        let doc = doc_with(vec![
            Entity::Dimension(Dimension::Ordinate(o)),
            Entity::Dimension(Dimension::Linear(l)),
        ]);

        let plan = plan(&doc, &[]);
        assert_eq!(plan.seen, 2);
        assert_eq!(plan.converted, 1, "线性成功");
        assert_eq!(plan.skipped.len(), 1, "坐标跳过");
        assert!(plan.skipped[0].contains("坐标标注"), "{:?}", plan.skipped);
        let r = plan.report();
        assert!(r.contains("转换成功 1"), "{r}");
        assert!(r.contains("跳过 1"), "{r}");
    }

    /// 有选中 → 只转选中集。
    #[test]
    fn plan_selected_only_converts_selection() {
        let mut doc = Doc::new();
        let mut mk = |x: f64| {
            let mut l = DimensionLinear::horizontal(
                Vector3::new(x, 0.0, 0.0),
                Vector3::new(x + 10.0, 0.0, 0.0),
            );
            l.definition_point = Vector3::new(x + 5.0, 5.0, 0.0);
            l.base.actual_measurement = 10.0;
            l.base.style_name = "Standard".into();
            doc.add_entity(Entity::Dimension(Dimension::Linear(l)))
                .unwrap()
        };
        let h1 = mk(0.0);
        let _h2 = mk(50.0);

        let plan = plan(&doc, &[h1]);
        assert_eq!(plan.seen, 1, "只扫描选中的 1 个");
        assert_eq!(plan.converted, 1);
        assert_eq!(plan.removes, vec![h1]);
    }

    /// 空图纸 / 无标注：0 扫描 0 转换。
    #[test]
    fn plan_empty_drawing_is_noop() {
        let doc = Doc::new();
        let plan = plan(&doc, &[]);
        assert_eq!(plan.seen, 0);
        assert_eq!(plan.converted, 0);
        assert!(plan.adds.is_empty() && plan.removes.is_empty());
    }

    /// 选中非标注对象 → 记入跳过报告。
    #[test]
    fn plan_reports_non_dimension_selection() {
        let doc = doc_with(vec![Entity::MText(MText::new())]);
        let h = doc.entities().next().unwrap().common().handle;
        let plan = plan(&doc, &[h]);
        assert_eq!(plan.seen, 0);
        assert_eq!(plan.skipped.len(), 1);
        assert!(plan.skipped[0].contains("非标注对象"), "{:?}", plan.skipped);
    }

    /// 对齐标注：产物为 Aligned 子类型（测量值 = 斜距）。
    #[test]
    fn plan_converts_aligned_dim() {
        let mut a = DimensionAligned::new(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(30.0, 40.0, 0.0),
        );
        a.definition_point = Vector3::new(21.0, 28.0, 0.0);
        a.base.actual_measurement = 50.0;
        a.base.style_name = "Standard".into();
        let doc = doc_with(vec![Entity::Dimension(Dimension::Aligned(a))]);
        let plan = plan(&doc, &[]);
        assert_eq!(plan.converted, 1, "{:?}", plan.skipped);
        let Entity::Dimension(new) = &plan.adds[0] else {
            panic!()
        };
        assert_eq!(new.base().dimension_type, DimensionType::Aligned);
    }
    /// 水平标注但两端点斜置（AutoCAD 常见）：尺寸线必须仍过原尺寸线点。
    #[test]
    fn plan_keeps_dimline_through_original_point_for_skewed_horizontal() {
        let mut d = DimensionLinear::horizontal(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(40.0, 10.0, 0.0),
        );
        d.definition_point = Vector3::new(20.0, 20.0, 0.0);
        d.base.definition_point = d.definition_point;
        d.base.actual_measurement = 40.0; // 水平：量的是 |dx|
        d.base.style_name = "Standard".into();
        let doc = doc_with(vec![Entity::Dimension(Dimension::Linear(d))]);

        let plan = plan(&doc, &[]);
        assert_eq!(plan.converted, 1, "转换成功: {:?}", plan.skipped);
        let Entity::Dimension(new) = &plan.adds[0] else {
            panic!()
        };
        assert!(
            (new.base().definition_point.y - 20.0).abs() < 1e-6,
            "尺寸线仍过原 y=20，实际 {:?}",
            new.base().definition_point
        );
        assert!(
            (new.base().actual_measurement - 40.0).abs() < 1e-9,
            "水平测量值 40 保持，实际 {}",
            new.base().actual_measurement
        );
    }

    /// 竖直标注：尺寸线仍在原 x 处。
    #[test]
    fn plan_keeps_dimline_x_for_vertical() {
        let mut d = DimensionLinear::vertical(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(5.0, 30.0, 0.0),
        );
        d.definition_point = Vector3::new(-12.0, 15.0, 0.0);
        d.base.definition_point = d.definition_point;
        d.base.actual_measurement = 30.0;
        d.base.style_name = "Standard".into();
        let doc = doc_with(vec![Entity::Dimension(Dimension::Linear(d))]);
        let plan = plan(&doc, &[]);
        assert_eq!(plan.converted, 1, "{:?}", plan.skipped);
        let Entity::Dimension(new) = &plan.adds[0] else {
            panic!()
        };
        assert!(
            (new.base().definition_point.x + 12.0).abs() < 1e-6,
            "尺寸线仍在 x=-12，实际 {:?}",
            new.base().definition_point
        );
    }
    // ── 端到端冒烟：plan → 落盘（镜像宿主 add_block_record 配方）→ DXF ──

    /// 把行动计划真实落到文档（镜像宿主 `ensure_dim_styles`/`add_block_record` 语义），
    /// 供 DxfWriter 落盘检查。
    fn apply_plan(doc: &mut Doc, plan: &Dim2GbPlan) {
        use acadrust::entities::{Block, BlockEnd};
        // 标注样式（镜像宿主 ensure_dim_styles 的字段映射）。
        for def in &plan.styles {
            let mut st = acadrust::tables::DimStyle::new(&def.name);
            st.handle = doc.allocate_handle();
            st.dimtxt = def.dimtxt;
            st.dimasz = def.dimasz;
            st.dimcen = def.dimcen;
            st.dimexe = def.dimexe;
            st.dimexo = def.dimexo;
            st.dimgap = def.dimgap;
            st.dimdli = def.dimdli;
            st.dimdle = def.dimdle;
            st.dimclrd = def.dimclrd;
            st.dimclre = def.dimclre;
            st.dimclrt = def.dimclrt;
            st.dimlwd = def.dimlwd;
            st.dimlwe = def.dimlwe;
            st.dimtad = def.dimtad;
            st.dimjust = def.dimjust;
            st.dimtih = def.dimtih;
            st.dimtoh = def.dimtoh;
            st.dimtix = def.dimtix;
            st.dimsoxd = def.dimsoxd;
            st.dimtofl = def.dimtofl;
            st.dimscale = def.dimscale;
            st.annotative = def.annotative;
            st.dimdec = def.dimdec;
            st.dimlunit = def.dimlunit;
            st.dimzin = def.dimzin;
            st.dimaunit = def.dimaunit;
            st.dimadec = def.dimadec;
            st.dimtol = def.dimtol;
            st.dimtp = def.dimtp;
            st.dimtm = def.dimtm;
            st.dimtdec = def.dimtdec;
            st.dimpost = def.dimpost.clone();
            if !def.dimtxsty.trim().is_empty() {
                st.dimtxsty = def.dimtxsty.clone();
            }
            doc.dim_styles.add_or_replace(st);
        }
        for (name, members) in &plan.blocks {
            if doc.block_records.get(name).is_some() {
                continue;
            }
            let mut br = acadrust::tables::BlockRecord::new(name);
            br.handle = doc.allocate_handle();
            let origin = Vector3::new(0.0, 0.0, 0.0);
            let mut block = Block::new(name, origin);
            block.common.handle = doc.allocate_handle();
            block.common.owner_handle = br.handle;
            br.block_entity_handle = block.common.handle;
            doc.add_entity(Entity::Block(block)).unwrap();
            let br_handle = br.handle;
            doc.block_records.add(br).unwrap();
            let mut hs = Vec::new();
            for mut e in members.clone() {
                e.common_mut().owner_handle = br_handle;
                hs.push(doc.add_entity(e).unwrap());
            }
            let mut end = BlockEnd::new();
            end.common.handle = doc.allocate_handle();
            end.common.owner_handle = br_handle;
            let end_handle = end.common.handle;
            doc.add_entity(Entity::BlockEnd(end)).unwrap();
            if let Some(br) = doc.block_records.get_mut(name) {
                br.entity_handles = hs;
                br.block_end_handle = end_handle;
            }
        }
        for e in &plan.adds {
            doc.add_entity(e.clone()).unwrap();
        }
        for h in &plan.removes {
            doc.remove_entity(*h);
        }
    }

    /// 六类原生标注 → 转换 → 落盘 DXF（设 OCSM_DIM2GB_SMOKE_OUT 才写文件）。
    #[test]
    fn dim2gb_smoke_write_dxf() {
        let out = std::env::var("OCSM_DIM2GB_SMOKE_OUT").unwrap_or_default();
        let mut doc = Doc::new();
        doc.dim_styles.add_or_replace({
            let mut st = acadrust::tables::DimStyle::new("Standard");
            st.dimdec = 2;
            st
        });

        // ① 水平线性 ② 竖直线性 ③ 对齐 ④ 半径 ⑤ 直径（+圆） ⑥ 角度 ⑦ 弧长
        let mut l1 = DimensionLinear::horizontal(
            Vector3::new(10.0, 10.0, 0.0),
            Vector3::new(60.0, 10.0, 0.0),
        );
        l1.definition_point = Vector3::new(35.0, 20.0, 0.0);
        l1.base.definition_point = l1.definition_point;
        l1.base.actual_measurement = 50.0;
        l1.base.style_name = "Standard".into();
        // OCS 自带公差（实体级 DSTYLE 覆盖）：+0.1 / −0.14（3 位小数）。
        l1.base.common.extended_data.add_record(dstyle_record(&[
            (71, XDataValue::Integer16(1)),
            (47, XDataValue::Real(0.1)),
            (48, XDataValue::Real(0.14)),
            (272, XDataValue::Integer16(3)),
        ]));
        doc.add_entity(Entity::Dimension(Dimension::Linear(l1))).unwrap();

        let mut l2 = DimensionLinear::vertical(
            Vector3::new(10.0, 10.0, 0.0),
            Vector3::new(10.0, 60.0, 0.0),
        );
        l2.definition_point = Vector3::new(-5.0, 35.0, 0.0);
        l2.base.definition_point = l2.definition_point;
        l2.base.actual_measurement = 50.0;
        l2.base.style_name = "Standard".into();
        doc.add_entity(Entity::Dimension(Dimension::Linear(l2))).unwrap();

        let mut a1 = DimensionAligned::new(
            Vector3::new(80.0, 10.0, 0.0),
            Vector3::new(110.0, 50.0, 0.0),
        );
        a1.definition_point = Vector3::new(105.0, 20.0, 0.0);
        a1.base.actual_measurement = 50.0;
        a1.base.style_name = "Standard".into();
        doc.add_entity(Entity::Dimension(Dimension::Aligned(a1))).unwrap();

        let mut r1 = DimensionRadius::new(
            Vector3::new(160.0, 30.0, 0.0),
            Vector3::new(160.0, 55.0, 0.0),
        );
        r1.base.text_middle_point = Vector3::new(160.0, 62.0, 0.0);
        r1.base.definition_point = r1.base.text_middle_point;
        r1.base.actual_measurement = 25.0;
        r1.base.style_name = "Standard".into();
        r1.base.common.extended_data.add_record(dstyle_record(&[
            (71, XDataValue::Integer16(1)),
            (47, XDataValue::Real(0.02)),
            (48, XDataValue::Real(0.02)),
            (272, XDataValue::Integer16(2)),
        ]));
        doc.add_entity(Entity::Dimension(Dimension::Radius(r1))).unwrap();

        let mut circle = Circle::new();
        circle.center = Vector3::new(230.0, 30.0, 0.0);
        circle.radius = 20.0;
        doc.add_entity(Entity::Circle(circle)).unwrap();
        let mut d1 = DimensionDiameter::new(
            Vector3::new(250.0, 30.0, 0.0),
            Vector3::new(210.0, 30.0, 0.0),
        );
        d1.base.text_middle_point = Vector3::new(258.0, 30.0, 0.0);
        d1.base.definition_point = d1.base.text_middle_point;
        d1.base.actual_measurement = 80.0; // 宿主口径的 2× 值
        d1.base.style_name = "Standard".into();
        doc.add_entity(Entity::Dimension(Dimension::Diameter(d1))).unwrap();

        let mut g = DimensionAngular2Ln::default();
        g.angle_vertex = Vector3::new(320.0, 30.0, 0.0);
        g.first_point = Vector3::new(320.0, 30.0, 0.0);
        g.second_point = Vector3::new(370.0, 30.0, 0.0);
        g.definition_point = Vector3::new(320.0, 80.0, 0.0);
        g.dimension_arc = Vector3::new(348.0, 48.0, 0.0);
        g.base.definition_point = g.dimension_arc;
        g.base.text_middle_point = Vector3::new(350.0, 50.0, 0.0);
        g.base.actual_measurement = 90.0;
        g.base.style_name = "Standard".into();
        doc.add_entity(Entity::Dimension(Dimension::Angular2Ln(g))).unwrap();

        let mut ar = DimensionArc::default();
        ar.center_point = Vector3::new(420.0, 30.0, 0.0);
        ar.first_extension_point = Vector3::new(450.0, 30.0, 0.0);
        ar.second_extension_point = Vector3::new(420.0, 60.0, 0.0);
        ar.arc_start_parameter = 0.0;
        ar.arc_end_parameter = std::f64::consts::FRAC_PI_2;
        ar.definition_point = Vector3::new(452.0, 30.0, 0.0);
        ar.base.definition_point = ar.definition_point;
        ar.base.text_middle_point = Vector3::new(455.0, 42.0, 0.0);
        ar.base.actual_measurement = 30.0 * std::f64::consts::FRAC_PI_2;
        ar.base.style_name = "Standard".into();
        doc.add_entity(Entity::Dimension(Dimension::Arc(ar))).unwrap();

        let plan = plan(&doc, &[]);
        assert_eq!(plan.seen, 7, "扫到 7 个原生标注");
        assert_eq!(
            plan.converted, 7,
            "七类全部转换成功，跳过：{:?}",
            plan.skipped
        );
        assert!(plan.blocks.len() >= 4, "直径/半径/角度/弧长各建块");

        if out.trim().is_empty() {
            return; // 未设环境变量：只跑断言，不落盘
        }
        // 输入版（未转换的原生标注）：给用户在 OCS 里实测 D2G 用。
        if let Some(input_path) = out.strip_suffix(".dxf") {
            acadrust::io::DxfWriter::new(&doc)
                .write_to_file(&format!("{input_path}-输入.dxf"))
                .unwrap();
        }
        apply_plan(&mut doc, &plan);
        acadrust::io::DxfWriter::new(&doc).write_to_file(&out).unwrap();
        println!("dim2gb smoke dxf written: {out}");
    }
    /// 回归：多个块式转换必须各自拿到**唯一**块名，且载体引用的块确实会被创建
    /// （历史 bug：收集器不同步块名 → 全部算出 *D1 互相覆盖）。
    #[test]
    fn plan_block_names_are_unique_and_referenced() {
        let mut doc = Doc::new();
        // 半径
        let mut r = DimensionRadius::new(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 10.0, 0.0),
        );
        r.base.text_middle_point = Vector3::new(0.0, 14.0, 0.0);
        r.base.actual_measurement = 10.0;
        r.base.style_name = "Standard".into();
        doc.add_entity(Entity::Dimension(Dimension::Radius(r))).unwrap();
        // 角度
        let mut g = DimensionAngular2Ln::default();
        g.angle_vertex = Vector3::new(50.0, 0.0, 0.0);
        g.first_point = Vector3::new(50.0, 0.0, 0.0);
        g.second_point = Vector3::new(60.0, 0.0, 0.0);
        g.definition_point = Vector3::new(50.0, 10.0, 0.0);
        g.dimension_arc = Vector3::new(57.0, 7.0, 0.0);
        g.base.definition_point = g.dimension_arc;
        g.base.text_middle_point = Vector3::new(58.0, 8.0, 0.0);
        g.base.actual_measurement = 90.0;
        g.base.style_name = "Standard".into();
        doc.add_entity(Entity::Dimension(Dimension::Angular2Ln(g))).unwrap();
        // 弧长
        let mut a = DimensionArc::default();
        a.center_point = Vector3::new(100.0, 0.0, 0.0);
        a.first_extension_point = Vector3::new(120.0, 0.0, 0.0);
        a.second_extension_point = Vector3::new(100.0, 20.0, 0.0);
        a.arc_start_parameter = 0.0;
        a.arc_end_parameter = std::f64::consts::FRAC_PI_2;
        a.definition_point = Vector3::new(122.0, 0.0, 0.0);
        a.base.definition_point = a.definition_point;
        a.base.text_middle_point = Vector3::new(124.0, 8.0, 0.0);
        a.base.actual_measurement = 20.0 * std::f64::consts::FRAC_PI_2;
        a.base.style_name = "Standard".into();
        doc.add_entity(Entity::Dimension(Dimension::Arc(a))).unwrap();

        let plan = plan(&doc, &[]);
        assert_eq!(plan.converted, 3, "{:?}", plan.skipped);
        assert_eq!(plan.blocks.len(), 3, "三个块各建一次");
        let names: Vec<&str> = plan.blocks.iter().map(|(n, _)| n.as_str()).collect();
        let mut uniq = names.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(uniq.len(), 3, "块名唯一，实际 {names:?}");

        // 尺寸载体（DIMENSION 的 block_name / INSERT 的块名）必须都在 plan.blocks 里。
        for e in &plan.adds {
            let referenced = match e {
                Entity::Dimension(d) => d.base().block_name.clone(),
                Entity::Insert(i) => i.block_name.clone(),
                _ => continue,
            };
            if referenced.trim().is_empty() {
                continue;
            }
            assert!(
                names.iter().any(|n| n.eq_ignore_ascii_case(&referenced)),
                "引用的块 {referenced} 未列入 plan.blocks（names={names:?}）"
            );
        }
    }
    /// 造一个 OCS 风格的实体级 DSTYLE 覆盖记录（与宿主 dim_override 写入同形）。
    fn dstyle_record(pairs: &[(i16, XDataValue)]) -> acadrust::xdata::ExtendedDataRecord {
        let mut rec = acadrust::xdata::ExtendedDataRecord::new("ACAD");
        rec.add_value(XDataValue::String("DSTYLE".into()));
        rec.add_value(XDataValue::ControlString("{".into()));
        for (code, val) in pairs {
            rec.add_value(XDataValue::Integer16(*code));
            rec.add_value(val.clone());
        }
        rec.add_value(XDataValue::ControlString("}".into()));
        rec
    }

    /// 用户实测反馈：OCS 自带公差写在**实体 XDATA 覆盖**里，D2G 必须带过来
    /// （只读样式表会全丢）。线性：堆叠进 dimtext。
    #[test]
    fn plan_carries_entity_level_tolerance_linear() {
        let mut l = DimensionLinear::horizontal(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(50.0, 0.0, 0.0),
        );
        l.definition_point = Vector3::new(25.0, 10.0, 0.0);
        l.base.actual_measurement = 50.0;
        l.base.style_name = "Standard".into();
        l.base.common.extended_data.add_record(dstyle_record(&[
            (71, XDataValue::Integer16(1)),   // DIMTOL
            (47, XDataValue::Real(0.1)),      // DIMTP
            (48, XDataValue::Real(0.14)),     // DIMTM
            (272, XDataValue::Integer16(3)),  // DIMTDEC
        ]));
        let doc = doc_with(vec![Entity::Dimension(Dimension::Linear(l))]);

        let plan = plan(&doc, &[]);
        assert_eq!(plan.converted, 1, "{:?}", plan.skipped);
        let Entity::Dimension(new) = &plan.adds[0] else {
            panic!()
        };
        let text = new.base().text.clone();
        assert!(
            text.contains("\\S+0.1^-0.14;"),
            "公差堆叠应来自实体覆盖且抹尾零，实际 {text:?}"
        );
    }

    /// 直径/半径（块内 MTEXT 形态）同样带公差。
    #[test]
    fn plan_carries_entity_level_tolerance_radial() {
        let mut r = DimensionRadius::new(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 20.0, 0.0),
        );
        r.base.text_middle_point = Vector3::new(0.0, 26.0, 0.0);
        r.base.definition_point = r.base.text_middle_point;
        r.base.actual_measurement = 20.0;
        r.base.style_name = "Standard".into();
        r.base.common.extended_data.add_record(dstyle_record(&[
            (71, XDataValue::Integer16(1)),
            (47, XDataValue::Real(0.05)),
            (48, XDataValue::Real(0.05)),
            (272, XDataValue::Integer16(2)),
        ]));
        let doc = doc_with(vec![Entity::Dimension(Dimension::Radius(r))]);

        let plan = plan(&doc, &[]);
        assert_eq!(plan.converted, 1, "{:?}", plan.skipped);
        let text = block_text(&plan.blocks[0]);
        assert!(
            text.contains("\\S+0.05^-0.05;"),
            "半径块内文字应含公差堆叠，实际 {text:?}"
        );
    }

    /// 样式表里的公差（无实体覆盖）照样带过来。
    #[test]
    fn plan_carries_style_level_tolerance() {
        let mut doc = Doc::new();
        let mut st = acadrust::tables::DimStyle::new("STDTOL");
        st.dimtol = true;
        st.dimtp = 0.2;
        st.dimtm = 0.2;
        st.dimtdec = 1;
        doc.dim_styles.add_or_replace(st);

        let mut l = DimensionLinear::horizontal(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(30.0, 0.0, 0.0),
        );
        l.definition_point = Vector3::new(15.0, 6.0, 0.0);
        l.base.actual_measurement = 30.0;
        l.base.style_name = "STDTOL".into();
        doc.add_entity(Entity::Dimension(Dimension::Linear(l))).unwrap();

        let plan = plan(&doc, &[]);
        let Entity::Dimension(new) = &plan.adds[0] else {
            panic!()
        };
        assert!(
            new.base().text.contains("\\S+0.2^-0.2;"),
            "样式公差应带过来，实际 {:?}",
            new.base().text
        );
    }
    /// 偏差文本规则：抹尾零；值为 0（含 −0.0 与数值噪声）→ 输出纯 "0"（无符号无小数）。
    #[test]
    fn deviation_text_strips_zeros_and_drops_sign_at_zero() {
        assert_eq!(deviation_text(0.1, 3, '+'), "+0.1");
        assert_eq!(deviation_text(0.14, 3, '-'), "-0.14");
        assert_eq!(deviation_text(0.2, 1, '-'), "-0.2");
        // 0 / 负零 / 小于量化步长的噪声 → 纯 "0"
        assert_eq!(deviation_text(0.0, 3, '-'), "0");
        assert_eq!(deviation_text(-0.0, 3, '-'), "0");
        assert_eq!(deviation_text(0.0, 3, '+'), "0");
        assert_eq!(deviation_text(0.0004, 3, '-'), "0");
        assert_eq!(deviation_text(-0.0004, 3, '-'), "0");
        // 恰好半格仍会进位（round-half-away-from-zero）
        assert_eq!(deviation_text(0.0005, 3, '-'), "-0.001");
        // 整数偏差不带小数点
        assert_eq!(deviation_text(1.0, 2, '+'), "+1");
    }

    /// 实体级公差里下差为 -0.0（OCS 界面常见）→ 堆叠应是 `^0` 而非 `^-0.00`。
    #[test]
    fn plan_zero_lower_deviation_converts_to_plain_zero() {
        let mut l = DimensionLinear::horizontal(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(20.0, 0.0, 0.0),
        );
        l.definition_point = Vector3::new(10.0, 5.0, 0.0);
        l.base.actual_measurement = 20.0;
        l.base.style_name = "Standard".into();
        l.base.common.extended_data.add_record(dstyle_record(&[
            (71, XDataValue::Integer16(1)),
            (47, XDataValue::Real(0.025)),
            (48, XDataValue::Real(-0.0)),
            (272, XDataValue::Integer16(4)),
        ]));
        let doc = doc_with(vec![Entity::Dimension(Dimension::Linear(l))]);
        let plan = plan(&doc, &[]);
        let Entity::Dimension(new) = &plan.adds[0] else {
            panic!()
        };
        let text = new.base().text.clone();
        assert!(
            text.contains("\\S+0.025^0;"),
            "下差 -0.0 应转成 0（无符号无小数），实际 {text:?}"
        );
        assert!(!text.contains("-0"), "不应出现 -0，实际 {text:?}");
    }
}
