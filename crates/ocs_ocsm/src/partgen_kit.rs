//! 参数化标准件**共用工具箱**（第四批起的 partgen_b1…b4 用）。
//!
//! 为什么另起一个文件：第四批是 11 个族、由**多个并行分支**各自实现，若都把
//! 图元构造/表加载写进 `partgen.rs`/`partgen_more.rs`，同一个文件会被反复改、
//! 冲突不断。这里把「与族无关」的东西集中一处，各族只在自己的模块里写画法与数据。
//!
//! 图层/颜色约定与 `partgen.rs` **完全一致**（都 ByLayer，颜色随层）：
//! - `1轮廓实线层` 粗实线；`2细线层` 牙底/细线；`3中心线层` 点划线；
//! - `4虚线层` 不可见轮廓；`5剖面线层` 剖面线。
//!
//! 圆弧的角度参数一律用**度**（与 DXF/GB 画法一致），内部自己转弧度
//! （acadrust 的 `Arc` 角度字段是弧度——这是本库最容易踩的坑）。

use ocs_plugin_api::host::acadrust::entities::{Arc, Circle, EntityType, Hatch, Line, LwPolyline};
use ocs_plugin_api::host::acadrust::types::{Color, LineWeight, Vector3};

pub use crate::partgen::{
    across_corners, minor_dia, spec_text_bolt, spec_text_dia, to_svg, BoltView, GenPart, PartMeta,
};

/// 轮廓（粗实线）层。
pub const LAYER_MAIN: &str = "1轮廓实线层";
/// 细实线层（牙底、细线、螺纹大径细线）。
pub const LAYER_THIN: &str = "2细线层";
/// 中心线层（点划线）。
pub const LAYER_CENTER: &str = "3中心线层";
/// 虚线层（不可见轮廓）。
pub const LAYER_HIDDEN: &str = "4虚线层";
/// 剖面线层（剖视图的金属剖面线）。
pub const LAYER_HATCH: &str = "5剖面线层";

// ── 基础图元 ───────────────────────────────────────────────────────────────

/// 一条线（落在指定图层）。
pub fn line(a: [f64; 2], b: [f64; 2], layer: &str) -> EntityType {
    let mut l = Line::from_points(
        Vector3::new(a[0], a[1], 0.0),
        Vector3::new(b[0], b[1], 0.0),
    );
    set_layer(&mut l, layer);
    EntityType::Line(l)
}

/// 圆弧：入参用**度**（与 DXF/GB 画法一致），内部转弧度。
pub fn arc(c: [f64; 2], r: f64, start_deg: f64, end_deg: f64, layer: &str) -> EntityType {
    let mut a = Arc::from_center_radius_angles(
        Vector3::new(c[0], c[1], 0.0),
        r,
        start_deg.to_radians(),
        end_deg.to_radians(),
    );
    set_layer(&mut a, layer);
    EntityType::Arc(a)
}

/// 整圆。
pub fn circle(c: [f64; 2], r: f64, layer: &str) -> EntityType {
    let mut e = Circle::from_center_radius(Vector3::new(c[0], c[1], 0.0), r);
    set_layer(&mut e, layer);
    EntityType::Circle(e)
}

/// 点列 → 单条 LwPolyline（`closed=true` 闭合）。虚线/中心线/异形轮廓都用它。
pub fn polyline(pts: &[[f64; 2]], closed: bool, layer: &str) -> EntityType {
    use ocs_plugin_api::host::acadrust::entities::LwVertex;
    use ocs_plugin_api::host::acadrust::types::Vector2;
    let mut pl = LwPolyline::new();
    for p in pts {
        let mut v = LwVertex::new(Vector2::new(p[0], p[1]));
        v.bulge = 0.0;
        pl.vertices.push(v);
    }
    pl.is_closed = closed;
    set_layer(&mut pl, layer);
    EntityType::LwPolyline(pl)
}

/// ANSI31 图案填充（落在 `5剖面线层`）：边界为闭合折线，`angle_deg` 传度数。
///
/// 三个单位/语义坑（都已按宿主实现校准，**别再改回去**）：
/// 1. `Hatch.pattern_angle` 存**弧度**（DXF 写出 `to_degrees()`、读入 `to_radians()`）；
///    写成度数 → 落图后被转 15470°。
/// 2. `HatchPatternLine.angle` 也存**弧度**（DXF 53 是度）。
/// 3. `HatchPatternLine.offset` 是**世界单位**偏移（DXF 45/46 原样存）；别写 PAT 文件里的
///    `0,.125`——宿主在有 `pattern.lines` 时直接用这一条定义画线并把 offset 当世界单位，
///    `0.125` 就变成 0.125 mm 间距（视觉上等于实心）。ANSI31 = 0.125″ = 3.175 mm 线间距。
pub fn hatch_ansi31(verts: &[[f64; 2]], angle_deg: f64) -> EntityType {
    use ocs_plugin_api::host::acadrust::entities::hatch::{
        BoundaryEdge, BoundaryPath, HatchPattern, HatchPatternLine, LineEdge,
    };
    use ocs_plugin_api::host::acadrust::types::Vector2;
    let mut h = Hatch::new();
    let mut pat = HatchPattern::new("ANSI31");
    pat.description = "ANSI Iron, Brick, Stone masonry".into();
    pat.add_line(HatchPatternLine {
        angle: 45f64.to_radians(),
        base_point: Vector2::new(0.0, 0.0),
        offset: Vector2::new(-2.245064030267288, 2.245064030267288),
        dash_lengths: Vec::new(),
    });
    h.pattern = pat;
    h.is_solid = false;
    h.pattern_angle = angle_deg.to_radians();
    h.pattern_scale = 1.0;
    let mut bp = BoundaryPath::new();
    bp.flags.set_external(true);
    for i in 0..verts.len() {
        bp.add_edge(BoundaryEdge::Line(LineEdge {
            start: Vector2::new(verts[i][0], verts[i][1]),
            end: Vector2::new(verts[(i + 1) % verts.len()][0], verts[(i + 1) % verts.len()][1]),
        }));
    }
    h.paths.push(bp);
    set_layer(&mut h, LAYER_HATCH);
    EntityType::Hatch(h)
}

/// 把实体归到某图层并统一 ByLayer（颜色/线型/线宽随层）。
pub fn set_layer<T: CommonLayer>(e: &mut T, layer: &str) {
    let c = e.common_mut();
    c.layer = layer.to_string();
    c.color = Color::ByLayer;
    c.linetype = "ByLayer".to_string();
    c.line_weight = LineWeight::ByLayer;
}

/// 让 `set_layer` 同时适配 Line/Arc/Circle/LwPolyline/Hatch。
pub trait CommonLayer {
    fn common_mut(&mut self) -> &mut ocs_plugin_api::host::acadrust::entities::EntityCommon;
}

macro_rules! impl_common_layer {
    ($($t:ty),* $(,)?) => {$(
        impl CommonLayer for $t {
            fn common_mut(&mut self) -> &mut ocs_plugin_api::host::acadrust::entities::EntityCommon {
                &mut self.common
            }
        }
    )*};
}
impl_common_layer!(Line, Arc, Circle, Hatch, LwPolyline);

/// 数字→规格文本用的紧凑格式（去掉多余的 0）。
pub fn trim(v: f64) -> String {
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

// ── 表加载 ─────────────────────────────────────────────────────────────────

/// 数据表 JSON 的通用外壳（`tables/parts*.json` 统一用这四个键）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Table<T> {
    #[allow(dead_code)]
    pub family: String,
    #[allow(dead_code)]
    pub code: String,
    #[allow(dead_code)]
    pub iso: String,
    pub rows: Vec<T>,
}

impl<T: serde::de::DeserializeOwned> Table<T> {
    /// 解析一张表（配合 `include_str!` + `OnceLock` 用，见各族的 `xxx_table()`）。
    pub fn parse(json: &str) -> Self {
        serde_json::from_str(json).expect("零件数据表 JSON 解析失败")
    }
    pub fn row(&self, d: f64) -> Option<&T>
    where
        T: HasD,
    {
        self.rows.iter().find(|r| (r.d() - d).abs() < 1e-9)
    }
}

/// 表行的公称直径（各族行结构都带 `d`）。
pub trait HasD {
    fn d(&self) -> f64;
}

// ── 视图与校验 ─────────────────────────────────────────────────────────────

/// 某族的视图注册表 → GUI 用的 JSON（`[{id,name}]`）。
///
/// 视图名的**唯一数据源**是 `partgen_more::family_views`（它会把新批次的
/// `partgen_bN::family_views` 也算进去），这里只做名字本地化，别另建一份。
pub fn views_json(family: &str) -> serde_json::Value {
    serde_json::Value::Array(
        crate::partgen_more::family_views(family)
            .into_iter()
            .map(|id| {
                let name = match id {
                    "main" => "主视图",
                    "section" => "剖视图",
                    "top" => "俯视图",
                    "end" => "左视图",
                    other => other,
                };
                serde_json::json!({ "id": id, "name": name })
            })
            .collect(),
    )
}

/// 视图名（`main`/`top`/`end`/`section`）→ `BoltView`（只有 main/top/end 的老三视图族用）。
pub fn bolt_view(view: &str) -> Result<BoltView, String> {
    match view {
        "main" => Ok(BoltView::Main),
        "top" => Ok(BoltView::Top),
        "end" => Ok(BoltView::End),
        other => Err(format!("没有视图 {other}（可用 main/top/end）")),
    }
}

/// 长度范围校验（错误文本与既有族一致，GUI/CLI 直接显示）。
pub fn check_length(l_min: f64, l_max: f64, d: f64, l: f64) -> Result<(), String> {
    if l < l_min - 1e-9 || l > l_max + 1e-9 {
        return Err(format!(
            "M{} 的长度范围是 {}…{}（收到 {l}）",
            trim(d),
            trim(l_min),
            trim(l_max)
        ));
    }
    Ok(())
}

/// 重量估算（钢 7.85 g/cm³）：体积 mm³ → kg。各族自己算体积。
pub fn weight_kg(volume_mm3: f64, density_g_cm3: f64) -> f64 {
    volume_mm3 * density_g_cm3 * 1e-3 / 1000.0
}

/// 重量文本（kg，4 位有效小数；与既有族一致）。
pub fn weight_text(kg: f64) -> String {
    let s = format!("{kg:.4}");
    s
}

// ── 自检/导出（人工核对用）──────────────────────────────────────────────────

/// 单视图 SVG（与 `partgen_more::view_svg` 同一渲染器）——给人工核对与叠合比对用。
pub fn part_svg(part: &GenPart, px_w: f64, px_h: f64) -> String {
    to_svg(part, &format!("{} {}", part.meta.code, part.meta.spec), px_w, px_h)
}

/// 测试里把生成结果落盘成 SVG（`dump_*` 测试用；路径放 `/tmp`）。
pub fn dump_svg(part: &GenPart, path: &str) -> std::io::Result<()> {
    std::fs::write(path, part_svg(part, 1200.0, 800.0))
}
