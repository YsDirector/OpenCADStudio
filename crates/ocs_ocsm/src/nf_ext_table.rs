//! 智能卡片「NF 外花键参数表」（`OCSMCARD`；`CardRenderer::NfExtTable`）：
//! **照模板 `~/桌面/GB/参数表/外花键参数表NF.dxf` 原版落卡**（67 线几何与 NF 内卡同构，
//! 见 `nf_table.rs`；本模块 = 模板原版，`nf_table.rs` 是它的内花键镜像）。
//!
//! # 与 NF 内卡的关系
//!
//! 用户那份 `外花键参数表NF.dxf` 是自定画法（模板实测：97 图元 = 66 LINE + 25 MTEXT +
//! 6 TEXT；13 行 × 2 列；末行标签 `公法线 W` 原 x=−444.95 出框）。NF 内卡照它同构镜像，
//! 本卡则是**模板原版**：标题/行名/符号照模板（外花键侧就是 `K / W` 那套），
//! 末行标签同样归位到 `x=−313.8426`；文字样式一律 `OCSM_GB`。
//!
//! # 字段与取值（NF 原文符号为准）
//!
//! | 行 | 标签 | 取值 | 出处 |
//! |---|---|---|---|
//! | 3 | 定心方式 | 齿面定心（缺省，模板）/ 外径定心 | p04 / p07 / 模板 |
//! | 4–6 | 模数 m / 齿数 z / 压力角 a | 输入 + p20–p22 表行核对 | p07 / p20–p22 |
//! | 7 | 齿根样式 | 平齿根（缺省）/ 圆齿根 | p05 / p07 |
//! | 8 | 加工方法 | 滚齿（模板口径） | 模板 / p19 |
//! | 9 | 大径 `Dee` | 齿面定心 `=A−0.2m`；外径定心 `=A` | p04 / p07 |
//! | 10 | 小径 `Die` | 平齿根 `=A−2.4m`；圆齿根 `=A−2.694m` | p05 / p07 |
//! | 11 | 基准尺寸 Do | `=A` | p07 |
//! | 12 | 跨测齿数 `K` | p23–p25 检查表 K 列（表外 → `—`） | p25 |
//! | 13 | 公法线 `W` | p23–p25 检查表 E 列（表外 → `—`） | p25 |
//!
//! **公差（模板实测 + ISO 286 + p29）**：
//! * 大径上/下差 = ISO 286 **h12**（模板示例 `Dee=298.5` 实测 `0/−0.460` 与 h12 一致；
//!   齿顶圆为非功能直径）；
//! * 小径上/下差 = ISO 286 **H7**（模板示例 `Die=282` 实测 `+0.052/0` 与 H7 一致；
//!   与 NF 内卡小径 H7 同一 ISO 286 查表口径）；
//! * 公法线上/下差 = p29 检查尺寸的公差值里 **外花键 E** 的偏差（µm→mm；四配合档，
//!   缺省固定）。模板示例 `+0.055/−0.042` 只作对账（与 p29 固定列 `+42/−42` 同量级），
//!   卡按标准表取值。
//!
//! **表外不外推**：`K/W` 只从 p23–p25 表值取；`(m,A)` 不在 p29 或 ISO 档缺时对应格
//! 显示 `—`，元素（ATTDEF）始终在，可在 CAD 里改写。`Dee/Die/Do` 是公式量照给。
//!
//! 排版口径：值属性用**实体级** [`VALUE_WIDTH_FACTOR`]（0.6，比内卡窄一档，保证最长值
//! `279.795` 不进公差格）压缩，公差照模板 [`crate::nf_table::TOL_WIDTH_FACTOR`]（几何回归见
//! `nf_ext_values_clear_tolerance_column`）。

use ocs_plugin_api::host::acadrust::entities::{
    AttributeDefinition, AttributeEntity, Entity as _, EntityType, Insert,
};
use ocs_plugin_api::host::acadrust::types::{Color, LineWeight, Vector3};

/// 表格块名（外花键一版版面）。
pub const BLOCK: &str = "OCSM_NFTABLE_NF_EXT";

/// 沿用 NF 内卡同一几何口径（模板 = 同一份 DXF）。
pub use crate::nf_table::{Centering, FitClass, RootStyle, LABEL_X, MISSING, TITLE_AT, VALUE_X};

/// 值属性实体级字宽：外卡最长值 `279.795`（小径圆齿根）要保证值框不进公差列（见几何回归）。
pub const VALUE_WIDTH_FACTOR: f64 = 0.6;

/// 12 个内容行：`(标签, 值属性 tag, 行中心 y)` —— 标签照模板原文（含空格）。
const NF_EXT_ROWS: &[(&str, &'static str, f64)] = &[
    ("执行标准", "执行标准", -62.27581186329274),
    ("定心方式", "定心方式", -102.8587392375943),
    ("模数 m", "模数", -143.4416666118961),
    ("齿数 z", "齿数", -184.0787951035162),
    ("压力角 a", "压力角", -224.7701247124548),
    ("齿根样式", "齿根样式", -265.4614543213933),
    ("加工方法", "加工方法", -305.9901805783765),
    ("大径 Dee", "大径Dee", -346.5189068353597),
    ("小径 Die", "小径Die", -386.9934319750245),
    ("基准尺寸 Do", "基准尺寸", -427.4679571146893),
    ("跨测齿数 K", "跨测齿数K", -468.4844934275385),
    ("公法线 W", "公法线W", -509.5552308577061),
];

/// 6 个公差洞位：`(tag, 插入 x, 基线 y)`，照模板 TEXT 逐点（字高 15、字宽 0.667）。
const NF_EXT_TOL_ATTS: &[(&str, f64, f64)] = &[
    ("大径上差", -79.67949597123788, -342.7153021054078),
    ("大径下差", -79.90685200460302, -362.9124102114762),
    ("小径上差", -87.69659398855129, -384.3053261490311),
    ("小径下差", -87.92395002191643, -404.5024342550995),
    ("公法线上差", -68.34721762208392, -505.6563921767285),
    ("公法线下差", -68.57457365544906, -525.8535002827967),
];

/// 12 个值属性 tag（自上而下的行序；`values()` 按本序返回）。
pub const VALUE_TAGS: &[&str] = &[
    "执行标准",
    "定心方式",
    "模数",
    "齿数",
    "压力角",
    "齿根样式",
    "加工方法",
    "大径Dee",
    "小径Die",
    "基准尺寸",
    "跨测齿数K",
    "公法线W",
];

/// 6 个公差属性 tag（大径/小径/公法线 的 上/下）。
pub const TOL_TAGS: &[&str] = &[
    "大径上差",
    "大径下差",
    "小径上差",
    "小径下差",
    "公法线上差",
    "公法线下差",
];

// ══════════════════════════════════════════════════════════════════════════
// 块几何（66 线照模板 + 标题 + 12 标签 + 18 ATTDEF）
// ══════════════════════════════════════════════════════════════════════════

/// 表格块成员：**66 线 + 13 MTEXT（标题 + 12 标签）+ 18 ATTDEF**（12 值 + 6 公差）。
pub fn block_entities() -> Vec<EntityType> {
    let mut out = Vec::with_capacity(
        crate::nf_table::NF_LINES.len() + 1 + NF_EXT_ROWS.len() + VALUE_TAGS.len() + TOL_TAGS.len(),
    );
    for (a, b, layer) in crate::nf_table::NF_LINES {
        out.push(crate::partgen_kit::line(*a, *b, layer));
    }
    out.push(crate::nf_table::mtext_ent(
        "外花键参数表",
        TITLE_AT.0,
        TITLE_AT.1,
        5,
    ));
    for (label, _, y) in NF_EXT_ROWS {
        out.push(crate::nf_table::mtext_ent(label, LABEL_X, *y, 4));
    }
    for ad in attdefs() {
        out.push(EntityType::AttributeDefinition(ad));
    }
    out
}

/// 18 个 ATTDEF：先 12 个值（行序），再 6 个公差（大径/小径/公法线 上/下）。
pub fn attdefs() -> Vec<AttributeDefinition> {
    let mut out = Vec::with_capacity(VALUE_TAGS.len() + TOL_TAGS.len());
    for (_, tag, y) in NF_EXT_ROWS {
        out.push(crate::nf_table::value_attdef_wf(tag, VALUE_X, *y, VALUE_WIDTH_FACTOR));
    }
    for (tag, x, y) in NF_EXT_TOL_ATTS {
        out.push(crate::nf_table::tol_attdef(tag, *x, *y));
    }
    out
}

// ══════════════════════════════════════════════════════════════════════════
// 取值（查 nf_e22141 已入库表；表外不外推）
// ══════════════════════════════════════════════════════════════════════════

/// 「NF 外花键参数表」卡参数（CLI 与 GUI 同一份字段）。
#[derive(Debug, Clone, PartialEq)]
pub struct NfExtTableSpec {
    /// 公称直径 `A`（NF 主参数）。
    pub a: f64,
    /// 模数 `m`。
    pub m: f64,
    /// 齿数 `z`（选填；给了要和 p20–p22 表行的 N 核对）。
    pub z: Option<u32>,
    /// 定心方式（缺省 = 齿面定心，模板口径）。
    pub centering: Centering,
    /// 齿根样式（缺省 = 平齿根）。
    pub root: RootStyle,
    /// 配合类别（p31/p34 四档；决定 p29 外花键 E 偏差）。
    pub fit: FitClass,
    pub at: Option<[f64; 2]>,
    pub rot: f64,
}

impl NfExtTableSpec {
    /// 缺省口径：齿面定心 + 平齿根 + 固定配合（照模板示例）。
    pub fn default_centering() -> Centering {
        Centering::Flank
    }
}

/// 查表 + 公式的中间结果（测试与预览共用；不直接落图）。
#[derive(Debug, Clone, PartialEq)]
pub struct NfExtDerived {
    /// 齿数（输入优先，否则取 p20–p22 表值；都无 = `None`）。
    pub z: Option<u32>,
    /// 大径 `Dee`（公式）。
    pub dee: f64,
    /// 小径 `Die`（公式）。
    pub die: f64,
    /// 跨测齿数 `K`（p23–p25；表外 `None`）。
    pub k: Option<f64>,
    /// 公法线 `W = E`（p23–p25；表外 `None`）。
    pub w: Option<f64>,
    /// p20–p22 表里的平齿根圆（交叉核对；表外 `None`）。
    pub table_flat_root: Option<f64>,
    /// p20–p22 表里的圆齿根圆（交叉核对；表外 `None`）。
    pub table_round_root: Option<f64>,
    /// 变位系数 `x`（p20–p22 尺寸表）。
    pub x: Option<f64>,
    /// 分度圆直径 `d = m·z`。
    pub d: Option<f64>,
    /// 基圆直径 `dB = d·cos20°`。
    pub db: Option<f64>,
    /// 分度圆弧齿厚 `s`。
    pub s: Option<f64>,
    /// 基圆弧齿厚 `sB`。
    pub sb: Option<f64>,
    /// 齿根圆角半径 `Rf`（平齿根）。
    pub rf: Option<f64>,
    /// 齿根圆角半径 `Rr`（圆齿根）。
    pub rr: Option<f64>,
    /// 齿顶倒角高度 `h`。
    pub h: Option<f64>,
    /// p20–p22 行来源说明（`—` + 原因 = 表外）。
    pub dims_source: String,
    /// p23–p25 行来源说明（K/W 的出处）。
    pub check_source: String,
    /// p29 偏差行（E/xm；表外 `None` → 公法线公差标缺）。
    pub tol_row: Option<&'static crate::nf_table::NfDevRow>,
    /// 外花键大径公差（ISO 286 h12；模板实测口径）。
    pub major_tol: Option<crate::tolerance::Limits>,
    /// 外花键小径公差（ISO 286 H7；模板实测口径）。
    pub minor_tol: Option<crate::tolerance::Limits>,
    /// 大径公差来源说明（含失败原因）。
    pub major_tol_note: String,
    /// 小径公差来源说明。
    pub minor_tol_note: String,
}

/// 主参数合法性（`z` 给了就限 3..=1000，与 NF 表量级一致）。
fn validate_basic(a: f64, m: f64, z: Option<u32>) -> Result<(), String> {
    if !(a.is_finite() && a > 0.0) {
        return Err(format!("NF 外花键参数表：公称直径 A={a} 必须是正数"));
    }
    if !(m.is_finite() && m > 0.0) {
        return Err(format!("NF 外花键参数表：模数 m={m} 必须是正数"));
    }
    if let Some(z) = z {
        if !(3..=1000).contains(&z) {
            return Err(format!("NF 外花键参数表：齿数 z={z} 超出范围（3–1000）"));
        }
    }
    Ok(())
}

/// 表行来源串（`p22 尺寸表(m=5.00~10.00)（source=p22）`）。
fn row_source_note(r: &crate::invol_spline::NfE22141Row) -> String {
    format!("p{} {}（source={}）", r.page, r.table_no, r.source)
}

/// 查表 + 公式求值；校验 `z` 与 p20–p22 表值一致（标准件口径）。
pub fn derive(spec: &NfExtTableSpec) -> Result<NfExtDerived, String> {
    validate_basic(spec.a, spec.m, spec.z)?;
    let m_tol = 1e-9;
    let a_tol = 1e-3;
    // ① p20–p22：外花键 14 列尺寸表行（m+A 联合定位）。
    let dims = crate::invol_spline::nf_e22141_rows()
        .iter()
        .find(|r| {
            (20..=22).contains(&r.page)
                && (r.m - spec.m).abs() < m_tol
                && (r.a - spec.a).abs() < a_tol
        })
        .cloned();
    // ② p23–p25：检查尺寸行（K/E = 跨测齿数/公法线）。
    let check = crate::invol_spline::nf_check_by_a_m(spec.a, spec.m)
        .into_iter()
        .find(|r| (23..=25).contains(&r.page) && r.has_check_dims());
    // ③ z 交叉核对：输入与表值不一致直接拦下（标准件表口径，避免 K/W 取错行）。
    let table_z = dims
        .as_ref()
        .map(|r| r.z)
        .or_else(|| check.as_ref().and_then(|r| r.n));
    if let (Some(z_in), Some(z_tab)) = (spec.z, table_z) {
        if z_in != z_tab {
            return Err(format!(
                "NF 外花键参数表：齿数 z={z_in} 与 NF E22-141 表行（A={} m={}）的 N={z_tab} 不一致；\
                 表值行请去掉 z 或改为 N={z_tab}",
                crate::partgen_kit::trim(spec.a),
                crate::partgen_kit::trim(spec.m)
            ));
        }
    }
    let z = spec.z.or(table_z);
    // ④ 公式量（p04/p05/p07）：齿面定心 Dee=A−0.2m，外径定心 Dee=A；Die 按平/圆齿根。
    let dee = match spec.centering {
        Centering::Flank => spec.a - 0.2 * spec.m,
        Centering::Outer => spec.a,
    };
    let die = match spec.root {
        RootStyle::Flat => spec.a - 2.4 * spec.m,
        RootStyle::Fillet => spec.a - 2.694 * spec.m,
    };
    // ⑤ 直径公差（模板实测对账：h12@298.5 = 0/−0.460；H7@282 = +0.052/0）。
    let major_tol = crate::tolerance::shaft(dee, "h12");
    let minor_tol = crate::tolerance::hole(die, "H7");
    let major_tol_note = match major_tol {
        Some(_) => "ISO 286 h12（外花键齿顶圆非功能直径；Ø298.5 → 0/−0.520。模板该格实测 0/−0.460 为模板另填，卡按 ISO 286 取值）"
            .to_string(),
        None => format!("—（ISO 286 无 Dee={} 的 h12 档）", crate::nf_table::fmt_mm(dee)),
    };
    let minor_tol_note = match minor_tol {
        Some(_) => "ISO 286 H7（模板示例 Die=282 实测 +0.052/0 与 H7 一致；与 NF 内卡小径同口径）".to_string(),
        None => format!("—（ISO 286 无 Die={} 的 H7 档）", crate::nf_table::fmt_mm(die)),
    };
    let tol_row = crate::nf_table::e_xm_tol_row(spec.m, spec.a);
    Ok(NfExtDerived {
        z,
        dee,
        die,
        k: check.as_ref().and_then(|r| r.k),
        w: check.as_ref().and_then(|r| r.e),
        table_flat_root: dims.as_ref().and_then(|r| r.flat_root),
        table_round_root: dims.as_ref().and_then(|r| r.round_root),
        x: dims.as_ref().and_then(|r| r.x),
        d: dims.as_ref().and_then(|r| r.d),
        db: dims.as_ref().and_then(|r| r.base_dia),
        s: dims.as_ref().and_then(|r| r.s),
        sb: dims.as_ref().and_then(|r| r.s_b),
        rf: dims.as_ref().and_then(|r| r.r),
        rr: dims.as_ref().and_then(|r| r.r_i),
        h: dims.as_ref().and_then(|r| r.h),
        dims_source: dims
            .as_ref()
            .map(row_source_note)
            .unwrap_or_else(|| "—（该 (m, A) 不在 p20–p22 外花键尺寸表内）".to_string()),
        check_source: check
            .as_ref()
            .map(|r| r.source_note())
            .unwrap_or_else(|| "—（该 (m, A) 不在 p23–p25 检查尺寸表内）".to_string()),
        tol_row,
        major_tol,
        minor_tol,
        major_tol_note,
        minor_tol_note,
    })
}

/// 18 项取值（顺序 = `attdefs()`：12 值 + 6 公差）。
///
/// 公差口径（模板 + ISO 286 + p29）：
/// * 大径上/下差 = ISO 286 **h12**（模板实测 Dee=298.5 → 0/−0.460）；
/// * 小径上/下差 = ISO 286 **H7**（模板实测 Die=282 → +0.052/0）；
/// * 公法线上/下差 = p29 **外花键 E 的偏差**（µm→mm；按所选配合类别）。
/// 查不到（(m,A) 不在 p29、ISO 档缺）只对应格「—」，其余照给。
pub fn values(spec: &NfExtTableSpec) -> Result<Vec<(String, String)>, String> {
    let d = derive(spec)?;
    let out: Vec<(&str, String)> = vec![
        ("执行标准", "NF E22-141".to_string()),
        ("定心方式", spec.centering.label().to_string()),
        ("模数", crate::nf_table::fmt_mm(spec.m)),
        (
            "齿数",
            d.z.map(|z| z.to_string())
                .unwrap_or_else(|| MISSING.to_string()),
        ),
        ("压力角", "20°".to_string()),
        ("齿根样式", spec.root.label().to_string()),
        ("加工方法", "滚齿".to_string()),
        ("大径Dee", crate::nf_table::fmt_mm(d.dee)),
        ("小径Die", crate::nf_table::fmt_mm(d.die)),
        ("基准尺寸", crate::nf_table::fmt_mm(spec.a)),
        (
            "跨测齿数K",
            d.k.map(crate::nf_table::fmt_mm)
                .unwrap_or_else(|| MISSING.to_string()),
        ),
        (
            "公法线W",
            d.w.map(crate::nf_table::fmt_mm)
                .unwrap_or_else(|| MISSING.to_string()),
        ),
        ("大径上差", crate::nf_table::tol_display(&d.major_tol, true)),
        ("大径下差", crate::nf_table::tol_display(&d.major_tol, false)),
        ("小径上差", crate::nf_table::tol_display(&d.minor_tol, true)),
        ("小径下差", crate::nf_table::tol_display(&d.minor_tol, false)),
        (
            "公法线上差",
            d.tol_row
                .map(|r| crate::nf_table::fmt_um_mm(r.e_ext_for(spec.fit).upper))
                .unwrap_or_else(|| MISSING.to_string()),
        ),
        (
            "公法线下差",
            d.tol_row
                .map(|r| crate::nf_table::fmt_um_mm(r.e_ext_for(spec.fit).lower))
                .unwrap_or_else(|| MISSING.to_string()),
        ),
    ];
    // 顺序护栏：取值顺序必须与 ATTDEF 表一致（加/改行时先在这里暴露）。
    let got: Vec<&str> = out.iter().map(|(t, _)| *t).collect();
    let want: Vec<String> = attdefs().iter().map(|ad| ad.tag.clone()).collect();
    let want: Vec<&str> = want.iter().map(|s| s.as_str()).collect();
    if got != want {
        return Err(format!(
            "NF 外花键参数表：取值映射顺序与模板属性不一致：{got:?} != {want:?}"
        ));
    }
    Ok(out.into_iter().map(|(t, v)| (t.to_string(), v)).collect())
}

/// 建 INSERT（基点在 `at`，旋转 `rot_deg` 度；18 个 ATTRIB 取自 `values()`）。
pub fn build_insert(spec: &NfExtTableSpec, at: [f64; 2], rot_deg: f64) -> Result<Insert, String> {
    let vals = values(spec)?;
    let mut ins = Insert::new(BLOCK, Vector3::new(at[0], at[1], 0.0));
    ins.rotation = rot_deg.to_radians();
    {
        let c = &mut ins.common;
        c.layer = crate::partgen::LAYER_MAIN.to_string();
        c.color = Color::ByLayer;
        c.linetype = "ByLayer".to_string();
        c.line_weight = LineWeight::ByLayer;
    }
    for ad in attdefs() {
        let val = vals
            .iter()
            .find(|(tag, _)| tag == &ad.tag)
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        let mut tmpl = ad.clone();
        tmpl.rotation = 0.0; // 旋转由 INSERT 变换施加
        let mut attr = AttributeEntity::from_definition(&tmpl, Some(val));
        attr.apply_transform(&ins.get_transform());
        ins.attributes.push(attr);
    }
    Ok(ins)
}

/// 18 项 Markdown（回执/计算书用；与表同一份取值）。
pub fn markdown_table(spec: &NfExtTableSpec) -> Result<String, String> {
    let vals = values(spec)?;
    let mut md = String::new();
    md.push_str("| 属性 | 值 |\n|---|---|\n");
    for (tag, v) in vals {
        md.push_str(&format!("| {tag} | {v} |\n"));
    }
    Ok(md)
}

// ══════════════════════════════════════════════════════════════════════════
// GUI 选项表（18 项口径）与表单模型（CLI/GUI/HTTP 同一份字段）
// ══════════════════════════════════════════════════════════════════════════

/// 一项属性的取值口径（进原生 `title=`，不做常显）。
#[derive(Debug, Clone, Copy)]
pub struct NfExtColumnSpec {
    pub tag: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub formula: &'static str,
    pub source: &'static str,
}

const SOURCE_DIMS: &str = "NF E22-141 中文译本 p20–p22（外花键尺寸表；assets/nf_e22141_dims.csv）";
const SOURCE_CHECK: &str = "NF E22-141 中文译本 p23–p25（检查尺寸表；assets/nf_e22141_check.csv）";
const SOURCE_MAJOR_TOL: &str =
    "ISO 286 h12（外花键齿顶圆非功能直径；模板示例 Dee=298.5 实测 0/−0.460 一致）";
const SOURCE_MINOR_TOL: &str =
    "ISO 286 H7（模板示例 Die=282 实测 +0.052/0 一致；与 NF 内卡小径同口径）";
const SOURCE_W_TOL: &str = "NF E22-141 p29（检查尺寸的公差值：外花键 E 的偏差，微米；按所选配合）";

/// 18 项口径（顺序 = `attdefs()`）。
pub const NF_EXT_COLUMNS: &[NfExtColumnSpec] = &[
    NfExtColumnSpec { tag: "执行标准", label: "执行标准", unit: "", formula: "固定 NF E22-141（p01 封面）", source: "NF E22-141" },
    NfExtColumnSpec { tag: "定心方式", label: "定心方式", unit: "", formula: "缺省齿面定心（Dee=A−0.2m，模板）；外径定心 Dee=A", source: "p04/p07；模板示例「齿形定心」" },
    NfExtColumnSpec { tag: "模数", label: "模数 m", unit: "mm", formula: "输入（NF 模数档 0.50/0.75/1/1.25/1.667/2.5/3.75/5/7.5/10）", source: "p20–p22 尺寸表实际 m 列" },
    NfExtColumnSpec { tag: "齿数", label: "齿数 z", unit: "", formula: "输入或取 p20–p22 表 N；与表值不一致直接报错", source: SOURCE_DIMS },
    NfExtColumnSpec { tag: "压力角", label: "压力角 a", unit: "°", formula: "NF E22-141 全表 20°", source: "p07" },
    NfExtColumnSpec { tag: "齿根样式", label: "齿根样式", unit: "", formula: "平齿根（缺省）/ 圆齿根；对应 Die=A−2.4m / A−2.694m", source: "p05 / p07" },
    NfExtColumnSpec { tag: "加工方法", label: "加工方法", unit: "", formula: "滚齿（外花键模板口径）", source: "模板；p19 图三十二/三十三（滚齿/插齿）" },
    NfExtColumnSpec { tag: "大径Dee", label: "大径 Dee（外花键齿顶圆）", unit: "mm", formula: "齿面定心 Dee=A−0.2m；外径定心 Dee=A", source: "p04 / p07；p22 表对账" },
    NfExtColumnSpec { tag: "小径Die", label: "小径 Die（外花键齿根圆）", unit: "mm", formula: "平齿根 Die=A−2.4m；圆齿根 Die=A−2.694m", source: "p05 / p07；p22 表平/圆齿根列对账" },
    NfExtColumnSpec { tag: "基准尺寸", label: "基准尺寸 Do", unit: "mm", formula: "Do = A（NF 主参数）", source: "p07" },
    NfExtColumnSpec { tag: "跨测齿数K", label: "跨测齿数 K", unit: "", formula: "p23–p25 检查表 K 列（表外 → 「—」，不外推）", source: SOURCE_CHECK },
    NfExtColumnSpec { tag: "公法线W", label: "公法线 W", unit: "mm", formula: "p23–p25 检查表 E 列 = K 齿公法线长度（表外 → 「—」）", source: SOURCE_CHECK },
    NfExtColumnSpec { tag: "大径上差", label: "大径上差", unit: "mm", formula: "ISO 286 h12（模板实测口径）", source: SOURCE_MAJOR_TOL },
    NfExtColumnSpec { tag: "大径下差", label: "大径下差", unit: "mm", formula: "ISO 286 h12（模板实测口径）", source: SOURCE_MAJOR_TOL },
    NfExtColumnSpec { tag: "小径上差", label: "小径上差", unit: "mm", formula: "ISO 286 H7（模板实测口径）", source: SOURCE_MINOR_TOL },
    NfExtColumnSpec { tag: "小径下差", label: "小径下差", unit: "mm", formula: "ISO 286 H7（模板实测口径）", source: SOURCE_MINOR_TOL },
    NfExtColumnSpec { tag: "公法线上差", label: "公法线上差", unit: "mm", formula: "p29 外花键 E 偏差上差（µm→mm；按所选配合）", source: SOURCE_W_TOL },
    NfExtColumnSpec { tag: "公法线下差", label: "公法线下差", unit: "mm", formula: "p29 外花键 E 偏差下差（µm→mm；按所选配合）", source: SOURCE_W_TOL },
];

/// NF 外花键卡的表达式策略（体系 SPLINE、方向恒外、α=20、直齿；变位进 A 公式）。
pub const EXPR_POLICY: crate::card_expr::ExprPolicy = crate::card_expr::ExprPolicy {
    card: "NF 外花键参数表",
    mark: crate::card_expr::ExprMark::Spline,
    alphas: &[20.0],
    spur: true,
    allow_shift: true,
    map: &[
        crate::card_expr::ExprRule {
            target: "a",
            label: "公称直径 A",
            op: crate::card_expr::ExprOp::NfBaseA,
        },
        crate::card_expr::ExprRule {
            target: "m",
            label: "模数 m",
            op: crate::card_expr::ExprOp::Module,
        },
        crate::card_expr::ExprRule {
            target: "z",
            label: "齿数 z",
            op: crate::card_expr::ExprOp::Teeth,
        },
    ],
};

pub const FORM: crate::card::CardFormSpec = crate::card::CardFormSpec {
    fields: &[
        crate::card::CardFieldSpec {
            key: "expr",
            label: "齿形表达式（九字段；可从轴/齿轮生成器 GUI 复制）",
            kind: "textarea",
            placeholder: "SPLINE EX M7.5 Z38 ALPHA20 X0.8 BETA0 H30",
            default: "",
            title: "九字段统一齿形表达式（MARK KIND M Z ALPHA X DA DF BETA H）；粘贴后自动反解 A=m(z+0.4+2x)、m、z；NF 压力角恒 20°、方向恒外（EX）",
            options: &[],
            options_from: "",
            min: 0.0,
            step: 0.0,
            required: false,
        },
        crate::card::CardFieldSpec {
            key: "a",
            label: "公称直径 A",
            kind: "number",
            placeholder: "NF 主参数（表值）",
            default: "300",
            title: "NF 主参数 A；同一 A 可对应不同模数（p20–p22 尺寸表）",
            options: &[],
            options_from: "",
            min: 0.0,
            step: 0.001,
            required: true,
        },
        crate::card::CardFieldSpec {
            key: "m",
            label: "模数 m",
            kind: "number",
            placeholder: "NF 模数档",
            default: "7.5",
            title: "NF 模数档（p20–p22 表实际 m 列：0.50/0.75/1/1.25/1.667/2.5/3.75/5/7.5/10）",
            options: &[],
            options_from: "",
            min: 0.0,
            step: 0.001,
            required: true,
        },
        crate::card::CardFieldSpec {
            key: "z",
            label: "齿数 z",
            kind: "number",
            placeholder: "选填，按 p20–p22 表核对",
            default: "38",
            title: "选填；与 p20–p22 表 N 不一致直接报错（避免 K/W 取错行）",
            options: &[],
            options_from: "",
            min: 3.0,
            step: 1.0,
            required: false,
        },
        crate::card::CardFieldSpec {
            key: "centering",
            label: "定心方式",
            kind: "select",
            placeholder: "",
            default: "flank",
            title: "NF E22-141 外花键定心方式（p04/p07）；缺省齿面定心 Dee=A−0.2m（模板示例）",
            options: &[],
            options_from: "nf_ext_centering",
            min: 0.0,
            step: 0.0,
            required: false,
        },
        crate::card::CardFieldSpec {
            key: "root",
            label: "齿根样式",
            kind: "select",
            placeholder: "",
            default: "flat",
            title: "行 7 文本；对应 Die=A−2.4m（平）/A−2.694m（圆）",
            options: &[],
            options_from: "nf_ext_roots",
            min: 0.0,
            step: 0.0,
            required: false,
        },
        crate::card::CardFieldSpec {
            key: "fit",
            label: "配合类别",
            kind: "select",
            placeholder: "",
            default: "fixed",
            title: "NF E22-141 p31/p34：松动/滑动/固定/压；决定 p29 外花键 E 偏差（公法线公差）",
            options: &[
                ("loose", "松动"),
                ("slide", "滑动"),
                ("fixed", "固定"),
                ("press", "压"),
            ],
            options_from: "",
            min: 0.0,
            step: 0.0,
            required: false,
        },
    ],
    note: "粘九字段表达式（自动反解 A/m/z）或直接填 A/m（z 选填核对）→ 选定心/齿根/配合 → 点「出表」回到图纸放置。\
           公差：大径 h12 / 小径 H7（模板实测口径）+ 公法线 = p29 外花键 E 偏差。",
    missing_note: "K/W 只取 p23–p25 检查表值，(m,A) 不在表内显示「—」；\
                   p29 表外或 ISO 档缺时对应公差格显示「—」，不外推。",
};

/// NF 外花键卡的选项/口径 JSON（随 `/api/spline_options` 下发；页面只渲染）。
pub fn options_json() -> serde_json::Value {
    serde_json::json!({
        "columns": NF_EXT_COLUMNS.iter().map(|c| serde_json::json!({
            "tag": c.tag,
            "label": c.label,
            "unit": c.unit,
            "formula": c.formula,
            "source": c.source,
        })).collect::<Vec<_>>(),
        "modules": crate::invol_spline::nf_e22141_modules(),
        "centering": [
            {"id": "flank", "label": "齿面定心（Dee=A−0.2m，模板）"},
            {"id": "outer", "label": "外径定心（Dee=A）"},
        ],
        "roots": [
            {"id": "flat", "label": "平齿根"},
            {"id": "fillet", "label": "圆齿根"},
        ],
        // p31/p34 配合名（NF E22-141 译本体系；四档按 p29 外花键 E 分列）
        "fits": FitClass::ALL.iter().map(|f| serde_json::json!({
            "id": f.id(),
            "label": f.label(),
        })).collect::<Vec<_>>(),
        "missing_note": "公差口径（模板实测 + ISO 286 + NF E22-141 p29）：大径上/下差 = ISO 286 h12\
                         （模板示例 Dee=298.5 实测 0/−0.460）；小径上/下差 = ISO 286 H7\
                         （模板示例 Die=282 实测 +0.052/0）；公法线上/下差 = p29 检查尺寸的公差值里\
                         **外花键 E** 的偏差（µm→mm，按所选配合）。K/W 取 p23–p25 检查表；\
                         (m,A) 表外或 ISO 档缺 → 对应格「—」，不外推；\
                         元素（ATTDEF）始终存在，可在 CAD 里改写。",
        "note": "版面照模板 `外花键参数表NF.dxf` 原版（13 行 × 2 列，66 线 + 13 标签 + 18 属性）；\
                 文字样式一律 OCSM_GB；模板末行标签出框已归位；\
                 符号用 NF 原文（Dee/Die/K/W，不用 GB 符号）。",
    })
}

/// CLI/HTTP 表单模型（字段与 GUI 控件一一对应）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct NfExtTableModel {
    /// 卡类型（GUI 回传；后端按 renderer 分派，这里只记不看）。
    #[serde(default)]
    pub card: String,
    /// 九字段统一齿形表达式（可空；给了则覆盖 A/m/z）。
    #[serde(default)]
    pub expr: Option<String>,
    /// 公称直径 `A`。
    pub a: f64,
    /// 模数 `m`。
    pub m: f64,
    /// 齿数 `z`（选填）。
    #[serde(default)]
    pub z: Option<u32>,
    /// 定心方式（`flank`/`outer`/`齿面`/`外径`；缺省 flank）。
    #[serde(default)]
    pub centering: Option<String>,
    /// 齿根样式（`flat`/`fillet`/`平`/`圆`；缺省 flat）。
    #[serde(default)]
    pub root: Option<String>,
    /// 配合类别（`loose`/`slide`/`fixed`/`press` 或中文；缺省 fixed）。
    #[serde(default)]
    pub fit: Option<String>,
    /// 显式落点；缺省 = 待放置件。
    #[serde(default)]
    pub at: Option<[f64; 2]>,
    #[serde(default)]
    pub rot: f64,
}

impl NfExtTableModel {
    /// →（校验过的 `NfExtTableSpec`）。
    pub fn spec(&self) -> Result<NfExtTableSpec, String> {
        let centering = match self.centering.as_deref().map(str::trim) {
            None | Some("") => NfExtTableSpec::default_centering(),
            Some(t) => Centering::from_token(t).ok_or_else(|| {
                format!("NF 外花键参数表：定心方式「{t}」非法（可用 齿面 / 外径）")
            })?,
        };
        let root = match self.root.as_deref().map(str::trim) {
            None | Some("") => RootStyle::Flat,
            Some(t) => RootStyle::from_token(t).ok_or_else(|| {
                format!("NF 外花键参数表：齿根样式「{t}」非法（可用 平 / 圆）")
            })?,
        };
        let fit = match self.fit.as_deref().map(str::trim) {
            None | Some("") => FitClass::default(),
            Some(t) => FitClass::from_token(t).ok_or_else(|| {
                format!("NF 外花键参数表：配合类别「{t}」非法（可用 松动 / 滑动 / 固定 / 压）")
            })?,
        };
        let (a, m, z) = match self.expr.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(e) => {
                let r = crate::card_expr::resolve(&EXPR_POLICY, e, Some(false))?;
                (
                    r.value("a").ok_or_else(|| {
                        "NF 外花键参数表：表达式映射表缺 A（内部错误）".to_string()
                    })?,
                    r.value("m").ok_or_else(|| {
                        "NF 外花键参数表：表达式映射表缺 m（内部错误）".to_string()
                    })?,
                    Some(
                        r.value("z").ok_or_else(|| {
                            "NF 外花键参数表：表达式映射表缺 z（内部错误）".to_string()
                        })? as u32,
                    ),
                )
            }
            None => (self.a, self.m, self.z),
        };
        let spec = NfExtTableSpec {
            a,
            m,
            z,
            centering,
            root,
            fit,
            at: self.at,
            rot: self.rot,
        };
        derive(&spec)?; // 立即校验（A/m/z 与表值）
        Ok(spec)
    }

    /// 预览 JSON（不碰图纸）。
    pub fn preview_json(&self) -> Result<serde_json::Value, String> {
        let spec = self.spec()?;
        let d = derive(&spec)?;
        let vals = values(&spec)?;
        let mut items = Vec::with_capacity(NF_EXT_COLUMNS.len());
        let mut missing = Vec::new();
        for (c, (tag, value)) in NF_EXT_COLUMNS.iter().zip(vals) {
            debug_assert_eq!(c.tag, tag);
            if value == MISSING {
                missing.push(c.label.to_string());
            }
            items.push(serde_json::json!({
                "tag": tag,
                "label": c.label,
                "unit": c.unit,
                "value": value,
                "formula": c.formula,
                "source": c.source,
                "missing": value == MISSING,
            }));
        }
        let opt = |v: Option<f64>| v.map(crate::nf_table::fmt_mm).unwrap_or_else(|| MISSING.to_string());
        let dee_formula = match spec.centering {
            Centering::Flank => "Dee = A − 0.2m",
            Centering::Outer => "Dee = A",
        };
        let die_formula = match spec.root {
            RootStyle::Flat => "Die = A − 2.4m",
            RootStyle::Fillet => "Die = A − 2.694m",
        };
        // p29 外花键 E/xm 偏差读数（按所选配合）；内花键 E 作对照。
        let um = |p: crate::nf_table::DevPair| format!("{:+}/{:+}", p.upper, p.lower);
        let (p29_ext, p29_int) = match d.tol_row {
            Some(r) => (
                format!(
                    "E {}（{}/{} mm）；xm {}（µm）",
                    um(r.e_ext_for(spec.fit)),
                    crate::nf_table::fmt_um_mm(r.e_ext_for(spec.fit).upper),
                    crate::nf_table::fmt_um_mm(r.e_ext_for(spec.fit).lower),
                    um(r.xm_ext_for(spec.fit))
                ),
                format!(
                    "E {}（{}/{} mm）；xm {}（µm；对照）",
                    um(r.e_int),
                    crate::nf_table::fmt_um_mm(r.e_int.upper),
                    crate::nf_table::fmt_um_mm(r.e_int.lower),
                    um(r.xm_int)
                ),
            ),
            None => (MISSING.to_string(), MISSING.to_string()),
        };
        let readout = serde_json::json!([
            {"k": "公称直径 A（主参数）", "v": crate::nf_table::fmt_mm(spec.a)},
            {"k": format!("大径 Dee（{dee_formula}）"), "v": crate::nf_table::fmt_mm(d.dee)},
            {"k": format!("小径 Die（{die_formula}）"), "v": crate::nf_table::fmt_mm(d.die)},
            {"k": "p20–p22 表齿根圆交叉核对（平 / 圆）", "v": format!("{} / {}", opt(d.table_flat_root), opt(d.table_round_root))},
            {"k": "跨测齿数 K / 公法线 W", "v": format!("{} / {}", opt(d.k), opt(d.w))},
            {"k": "变位系数 x（p20–p22）", "v": opt(d.x)},
            {"k": "分度圆 d / 基圆 dB", "v": format!("{} / {}", opt(d.d), opt(d.db))},
            {"k": "分度圆弧齿厚 s / 基圆 sB", "v": format!("{} / {}", opt(d.s), opt(d.sb))},
            {"k": "齿根圆角 Rf / Rr", "v": format!("{} / {}", opt(d.rf), opt(d.rr))},
            {"k": "齿顶倒角高度 h", "v": opt(d.h)},
            {"k": format!("p29 外花键·{}偏差（µm）", spec.fit.label()), "v": p29_ext},
            {"k": "p29 内花键 E 偏差（µm；对照）", "v": p29_int},
            {"k": "大径上/下差", "v": d.major_tol.map(|l| { let (u, lo) = l.display(); format!("{u} / {lo}") }).unwrap_or_else(|| MISSING.to_string())},
            {"k": "小径上/下差", "v": d.minor_tol.map(|l| { let (u, lo) = l.display(); format!("{u} / {lo}") }).unwrap_or_else(|| MISSING.to_string())},
            {"k": "p20–p22 行", "v": d.dims_source},
            {"k": "p23–p25 行", "v": d.check_source},
        ]);
        let expr_echo = self
            .expr
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let fields = if expr_echo.is_some() {
            serde_json::json!({
                "a": spec.a,
                "m": spec.m,
                "z": spec.z,
            })
        } else {
            serde_json::Value::Null
        };
        Ok(serde_json::json!({
            "ok": true,
            "card": "NF外花键参数表",
            "renderer": "nf_ext_table",
            "expr": expr_echo,
            "fields": fields,
            "title": format!(
                "NF E22-141 外花键参数表（A={} m={} z={}，{}配合）",
                crate::nf_table::fmt_mm(spec.a),
                crate::nf_table::fmt_mm(spec.m),
                d.z.map(|z| z.to_string()).unwrap_or_else(|| MISSING.to_string()),
                spec.fit.label()
            ),
            "a": spec.a,
            "m": spec.m,
            "z": d.z,
            "centering": spec.centering.id(),
            "root": spec.root.id(),
            "fit": spec.fit.id(),
            "readout": readout,
            "items": items,
            "missing": missing,
            "missing_note": "公差口径：大径 h12 / 小径 H7（ISO 286，模板实测） + 公法线 = p29 外花键 E 偏差（按配合）；\
                             (m,A) 不在 p29 或 ISO 档缺 → 对应格「—」，不外推；\
                             K/W 只取 p23–p25 检查表值，表外显示「—」。",
        }))
    }

    /// 待放置件要带的 18 个 ATTRIB（tag → 值 + ATTDEF 模板）。
    pub fn pending_attrs(&self) -> Result<Vec<(AttributeDefinition, String)>, String> {
        let spec = self.spec()?;
        let vals = values(&spec)?;
        let mut out = Vec::with_capacity(vals.len());
        for ad in attdefs() {
            let v = vals
                .iter()
                .find(|(tag, _)| tag == &ad.tag)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            out.push((ad, v));
        }
        Ok(out)
    }

    /// 直接落点用的 `INSERT`（CLI 与 GUI 同一条路径）。
    pub fn build_insert(&self) -> Result<Insert, String> {
        let spec = self.spec()?;
        let at = self.at.unwrap_or([0.0, 0.0]);
        build_insert(&spec, at, self.rot)
    }

    /// 插入回执里的一段。
    pub fn echo_note(&self) -> Result<String, String> {
        let spec = self.spec()?;
        let d = derive(&spec)?;
        Ok(format!(
            "NF E22-141 外花键 A={} m={} z={}（{}，滚齿；{}配合；K/W {}）",
            crate::nf_table::fmt_mm(spec.a),
            crate::nf_table::fmt_mm(spec.m),
            d.z.map(|z| z.to_string()).unwrap_or_else(|| MISSING.to_string()),
            spec.centering.label(),
            spec.fit.label(),
            match (d.k, d.w) {
                (Some(k), Some(w)) => format!("{} / {}", crate::nf_table::fmt_mm(k), crate::nf_table::fmt_mm(w)),
                _ => MISSING.to_string(),
            }
        ))
    }

    /// 待放置件的 `OCSM_PART` 元数据（薄台账）。
    pub fn part_meta_json(&self) -> Result<String, String> {
        let spec = self.spec()?;
        let d = derive(&spec)?;
        Ok(serde_json::json!({
            "family": "nf_ext_table",
            "card": "NF外花键参数表",
            "a": spec.a,
            "m": spec.m,
            "z": d.z,
            "centering": spec.centering.id(),
            "root": spec.root.id(),
            "fit": spec.fit.id(),
        })
        .to_string())
    }
}

/// 表达式截取用：外花键卡选项关键字（`A300`/`M7.5`/`Z38`/`中心`/`根`/`配合`/`at`/`rot`…）。
fn is_option_token(t: &str) -> bool {
    let l = t.to_ascii_lowercase();
    if t.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return true;
    }
    if matches!(
        l.as_str(),
        "at" | "rot" | "旋转" | "中心" | "定心" | "centering" | "根" | "齿根" | "root" | "配合"
            | "fit" | "直径" | "公称直径" | "模数" | "齿数"
    ) {
        return true;
    }
    for p in ['a', 'm', 'z'] {
        if l == p.to_string() {
            return true;
        }
        if let Some(rest) = l.strip_prefix(p) {
            if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

/// 大小写不敏感地剥前缀（`A300` → `300`；非 ASCII 边界返回 `None`）。
fn strip_prefix_ci<'a>(t: &'a str, p: &str) -> Option<&'a str> {
    t.get(..p.len())
        .filter(|head| head.eq_ignore_ascii_case(p))
        .map(|_| &t[p.len()..])
}

pub fn usage() -> String {
    "智能卡片「NF外花键参数表」用法：\
     `OCSMCARD NF外花键参数表 <九字段齿形表达式> [中心 齿面|外径] [根 平|圆] [配合 松动|滑动|固定|压] [at x,y] [rot 度]`\
     （如 `OCSMCARD NF外花键参数表 SPLINE EX M7.5 Z38 ALPHA20 X0.8 BETA0 H30`；\
     也可沿用 `A300 M7.5 [Z38]` 写法）。表达式反解 A=m(z+0.4+2x)、m、z（NF 压力角恒 20°、方向恒外 EX）；\
     定心方式缺省「齿面定心」（Dee=A−0.2m，模板口径；外径定心 Dee=A）；加工方法照模板 = 滚齿；\
     小径 Die 平齿根 A−2.4m / 圆齿根 A−2.694m；K/W 取 p23–p25 检查表 —— 表外显示「—」，不外推；\
     公差：大径 h12 / 小径 H7（ISO 286，模板实测口径），公法线 = p29 外花键 E 偏差（按配合类别，缺省固定）。"
        .to_string()
}

impl NfExtTableSpec {
    /// 解析 `A<公称直径> M<模数> [Z<齿数>] [中心 齿面|外径] [根 平|圆] [配合 …] [at x,y] [rot 度]`。
    pub fn parse(text: &str) -> Result<Self, String> {
        let tokens: Vec<&str> = text.split_whitespace().collect();
        if tokens.is_empty() {
            return Err(usage());
        }
        let mut a: Option<f64> = None;
        let mut m: Option<f64> = None;
        let mut z: Option<u32> = None;
        let mut centering = NfExtTableSpec::default_centering();
        let mut root = RootStyle::Flat;
        let mut fit = FitClass::default();
        let mut at: Option<[f64; 2]> = None;
        let mut rot = 0.0f64;
        let mut i = 0usize;
        // 表达式形态：`<九字段表达式> [中心 …] [根 …] [配合 …] [at …] [rot …]`。
        let mut expr: Option<String> = None;
        let tokens = if let Some((e, rest)) = crate::card_expr::split_expr(&tokens, is_option_token) {
            expr = Some(e);
            rest
        } else {
            tokens
        };
        let need = |i: &mut usize, tokens: &[&str], what: &str| -> Result<String, String> {
            *i += 1;
            tokens
                .get(*i)
                .map(|s| s.to_string())
                .ok_or_else(|| format!("NF 外花键参数表：{what} 缺少数值/选项"))
        };
        while i < tokens.len() {
            let t = tokens[i];
            let key = t.to_ascii_lowercase();
            if key == "at" {
                let v = need(&mut i, &tokens, "at")?;
                let (x, y) = v
                    .split_once(',')
                    .ok_or_else(|| format!("NF 外花键参数表：at「{v}」应为 `x,y`"))?;
                let x: f64 = x
                    .trim()
                    .parse()
                    .map_err(|e| format!("NF 外花键参数表：at x={x} 不是数字：{e}"))?;
                let y: f64 = y
                    .trim()
                    .parse()
                    .map_err(|e| format!("NF 外花键参数表：at y={y} 不是数字：{e}"))?;
                at = Some([x, y]);
            } else if key == "rot" || key == "旋转" {
                let v = need(&mut i, &tokens, "rot")?;
                rot = v
                    .parse()
                    .map_err(|e| format!("NF 外花键参数表：rot={v} 不是数字：{e}"))?;
            } else if key == "中心" || key == "定心" || key == "centering" {
                let v = need(&mut i, &tokens, "中心")?;
                centering = Centering::from_token(&v).ok_or_else(|| {
                    format!("NF 外花键参数表：定心方式「{v}」非法（可用 齿面 / 外径）")
                })?;
            } else if key == "根" || key == "齿根" || key == "root" {
                let v = need(&mut i, &tokens, "根")?;
                root = RootStyle::from_token(&v)
                    .ok_or_else(|| format!("NF 外花键参数表：齿根样式「{v}」非法（可用 平 / 圆）"))?;
            } else if key == "配合" || key == "fit" {
                let v = need(&mut i, &tokens, "配合")?;
                fit = FitClass::from_token(&v).ok_or_else(|| {
                    format!("NF 外花键参数表：配合类别「{v}」非法（可用 松动 / 滑动 / 固定 / 压）")
                })?;
            } else if key == "直径" || key == "公称直径" || key == "a" {
                let v = need(&mut i, &tokens, "A")?;
                a = Some(
                    v.parse()
                        .map_err(|e| format!("NF 外花键参数表：A={v} 不是数字：{e}"))?,
                );
            } else if key == "模数" || key == "m" {
                let v = need(&mut i, &tokens, "m")?;
                m = Some(
                    v.parse()
                        .map_err(|e| format!("NF 外花键参数表：m={v} 不是数字：{e}"))?,
                );
            } else if key == "齿数" || key == "z" {
                let v = need(&mut i, &tokens, "z")?;
                z = Some(
                    v.parse()
                        .map_err(|e| format!("NF 外花键参数表：z={v} 不是整数：{e}"))?,
                );
            } else if let Some(v) = strip_prefix_ci(t, "a") {
                let v = if v.is_empty() { need(&mut i, &tokens, "A")? } else { v.to_string() };
                a = Some(
                    v.parse()
                        .map_err(|e| format!("NF 外花键参数表：A={v} 不是数字：{e}"))?,
                );
            } else if let Some(v) = strip_prefix_ci(t, "m") {
                let v = if v.is_empty() { need(&mut i, &tokens, "M")? } else { v.to_string() };
                m = Some(
                    v.parse()
                        .map_err(|e| format!("NF 外花键参数表：M={v} 不是数字：{e}"))?,
                );
            } else if let Some(v) = strip_prefix_ci(t, "z") {
                let v = if v.is_empty() { need(&mut i, &tokens, "Z")? } else { v.to_string() };
                z = Some(
                    v.parse()
                        .map_err(|e| format!("NF 外花键参数表：Z={v} 不是整数：{e}"))?,
                );
            } else {
                return Err(format!(
                    "NF 外花键参数表：不认识的参数「{t}」。\n{}",
                    usage()
                ));
            }
            i += 1;
        }
        // 表达式 > 旧 A/M/Z；显式值与表达式反解不一致 → 报错（帮排错）。
        let (a, m, z) = match expr {
            Some(e) => {
                let r = crate::card_expr::resolve(&EXPR_POLICY, &e, Some(false))?;
                let ea = r.value("a").ok_or_else(|| {
                    "NF 外花键参数表：表达式映射表缺 A（内部错误）".to_string()
                })?;
                let em = r.value("m").ok_or_else(|| {
                    "NF 外花键参数表：表达式映射表缺 m（内部错误）".to_string()
                })?;
                let ez = r.value("z").ok_or_else(|| {
                    "NF 外花键参数表：表达式映射表缺 z（内部错误）".to_string()
                })? as u32;
                if let Some(given) = a {
                    if (given - ea).abs() > 1e-6 {
                        return Err(format!(
                            "NF 外花键参数表：显式 A={} 与表达式反解的 A={} 不一致",
                            crate::partgen_kit::trim(given),
                            crate::partgen_kit::trim(ea)
                        ));
                    }
                }
                if let Some(given) = m {
                    if (given - em).abs() > 1e-9 {
                        return Err(format!(
                            "NF 外花键参数表：显式 m={} 与表达式反解的 m={} 不一致",
                            crate::partgen_kit::trim(given),
                            crate::partgen_kit::trim(em)
                        ));
                    }
                }
                if let Some(given) = z {
                    if given != ez {
                        return Err(format!(
                            "NF 外花键参数表：显式 z={given} 与表达式反解的 z={ez} 不一致"
                        ));
                    }
                }
                (ea, em, Some(ez))
            }
            None => {
                let a = a.ok_or_else(|| format!("NF 外花键参数表：缺公称直径 A（写法 `A300` / `直径 300`，或直接给九字段表达式）。\n{}", usage()))?;
                let m = m.ok_or_else(|| format!("NF 外花键参数表：缺模数 m（写法 `M7.5` / `模数 7.5`；同一 A 可对应不同模数，必填）。\n{}", usage()))?;
                (a, m, z)
            }
        };
        let spec = NfExtTableSpec {
            a,
            m,
            z,
            centering,
            root,
            fit,
            at,
            rot,
        };
        derive(&spec)?; // 立即校验（表值/ z 核对）
        Ok(spec)
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 测试
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nf_table::{
        FRAME, MID_X, TEXT_H, TOL_H, TOL_WIDTH_FACTOR,
    };
    use ocs_plugin_api::host::acadrust::entities::VerticalAlignment;
    use std::collections::HashMap;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn near5(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-5
    }

    fn anchor_spec() -> NfExtTableSpec {
        NfExtTableSpec {
            a: 300.0,
            m: 7.5,
            z: Some(38),
            centering: Centering::Flank,
            root: RootStyle::Flat,
            fit: FitClass::Fixed,
            at: None,
            rot: 0.0,
        }
    }

    fn value_map(spec: &NfExtTableSpec, at: [f64; 2]) -> HashMap<String, String> {
        build_insert(spec, at, 0.0)
            .unwrap()
            .attributes
            .iter()
            .map(|a| (a.tag.clone(), a.value.clone()))
            .collect()
    }

    /// 13 行结构 + 逐图元对模板（1e-5）：66 线（外框/中分隔/行分隔）、标题 + 12 标签、
    /// 12 值 ATTDEF、6 公差 ATTDEF；末行标签归位；样式一律 OCSM_GB。
    #[test]
    fn nf_ext_template_geometry_line_by_line() {
        let ents = block_entities();
        assert_eq!(
            ents.len(),
            66 + 13 + 18,
            "模板 97 图元（66 线 + 25 MTEXT + 6 TEXT）→ 66 线 + 13 标签 + 18 属性"
        );

        // ── 66 条 LINE：逐条在集合里能按坐标+图层找到（1e-5）──
        let line_hits = |a: [f64; 2], b: [f64; 2], layer: &str| -> bool {
            ents.iter().any(|e| {
                matches!(e, EntityType::Line(l)
                    if l.common.layer == layer
                        && ((near5(l.start.x, a[0]) && near5(l.start.y, a[1])
                            && near5(l.end.x, b[0]) && near5(l.end.y, b[1]))
                            || (near5(l.start.x, b[0]) && near5(l.start.y, b[1])
                                && near5(l.end.x, a[0]) && near5(l.end.y, a[1]))))
            })
        };
        assert_eq!(crate::nf_table::NF_LINES.len(), 66);
        for (a, b, layer) in crate::nf_table::NF_LINES {
            assert!(line_hits(*a, *b, layer), "缺线 {a:?}→{b:?} @{layer}");
        }
        // 13 行 = 14 条水平边界（标题 + 12 内容行）
        let mut sep_y: Vec<f64> = crate::nf_table::NF_LINES
            .iter()
            .filter(|(a, b, _)| near5(a[1], b[1]))
            .map(|(_, b, _)| b[1])
            .collect();
        sep_y.sort_by(|a, b| b.partial_cmp(a).unwrap());
        sep_y.dedup_by(|a, b| near5(*a, *b));
        assert_eq!(sep_y.len(), 14, "13 行 → 14 条水平边界：{sep_y:?}");
        assert!(near5(sep_y[0], 0.0) && near5(sep_y[13], FRAME.3));

        // ── 13 条 MTEXT：标题 + 12 标签，全部 OCSM_GB / 6文字层（照模板原文）──
        let mtexts: Vec<&ocs_plugin_api::host::acadrust::entities::MText> = ents
            .iter()
            .filter_map(|e| match e {
                EntityType::MText(m) => Some(m),
                _ => None,
            })
            .collect();
        assert_eq!(mtexts.len(), 13, "标题 + 12 标签");
        let title = mtexts
            .iter()
            .find(|m| m.value == "外花键参数表")
            .expect("标题 MTEXT");
        assert_eq!(
            title.attachment_point,
            ocs_plugin_api::host::acadrust::entities::mtext::AttachmentPoint::MiddleCenter
        );
        assert!(near5(title.insertion_point.x, TITLE_AT.0) && near5(title.insertion_point.y, TITLE_AT.1));
        for m in &mtexts {
            assert_eq!(m.style, "OCSM_GB", "MTEXT 样式：{}", m.value);
            assert_eq!(m.common.layer, "6文字层");
            assert!(near5(m.height, TEXT_H));
        }
        for (label, _, y) in NF_EXT_ROWS {
            let m = mtexts
                .iter()
                .find(|m| m.value == *label)
                .unwrap_or_else(|| panic!("缺标签 MTEXT {label}"));
            assert!(
                near5(m.insertion_point.x, LABEL_X) && near5(m.insertion_point.y, *y),
                "{label} 位置异常：{:?}（末行必须归位 {LABEL_X}）",
                m.insertion_point
            );
        }
        // 模板末行标签原 x=−444.95 出框 → 不允许任何标签还在那个位置
        assert!(
            mtexts.iter().all(|m| (m.insertion_point.x + 444.9504118793738).abs() > 1.0),
            "末行标签仍在模板的出框位置"
        );

        // ── 18 个 ATTDEF：12 值（左中）+ 6 公差（基线），tag/位置/字高/字宽 ──
        let atts: Vec<AttributeDefinition> = attdefs();
        assert_eq!(atts.len(), 18);
        let want_tags: Vec<&str> = VALUE_TAGS.iter().chain(TOL_TAGS.iter()).copied().collect();
        let got_tags: Vec<&str> = atts.iter().map(|a| a.tag.as_str()).collect();
        assert_eq!(got_tags, want_tags, "ATTDEF tag 顺序");
        for (i, (_, tag, y)) in NF_EXT_ROWS.iter().enumerate() {
            let ad = &atts[i];
            assert_eq!(ad.tag, *tag);
            assert!(near5(ad.insertion_point.x, VALUE_X) && near5(ad.insertion_point.y, *y));
            assert!(near5(ad.height, TEXT_H));
            assert!(near5(ad.width_factor, VALUE_WIDTH_FACTOR));
            assert_eq!(ad.text_style, "OCSM_GB");
            assert_eq!(ad.common.layer, "6文字层");
            assert_eq!(ad.vertical_alignment, VerticalAlignment::Middle, "值行与模板 MTEXT 同为左中");
        }
        for (j, (tag, x, y)) in NF_EXT_TOL_ATTS.iter().enumerate() {
            let ad = &atts[12 + j];
            assert_eq!(ad.tag, *tag);
            assert!(near5(ad.insertion_point.x, *x) && near5(ad.insertion_point.y, *y));
            assert!(near5(ad.height, TOL_H));
            assert!(near5(ad.width_factor, TOL_WIDTH_FACTOR));
            assert_eq!(ad.vertical_alignment, VerticalAlignment::Baseline, "公差照模板 TEXT 基线");
            assert_eq!(ad.text_style, "OCSM_GB");
            assert_eq!(ad.common.layer, "6文字层");
        }
        // 公差 tag 分组：大径/小径/公法线 × 上/下
        for (i, pair) in [("大径上差", "大径下差"), ("小径上差", "小径下差"), ("公法线上差", "公法线下差")]
            .iter()
            .enumerate()
        {
            assert_eq!(atts[12 + 2 * i].tag, *pair.0);
            assert_eq!(atts[12 + 2 * i + 1].tag, *pair.1);
        }
    }

    /// ★ 锚点正向断言：模板示例 m=7.5 / A=300 / z=38 的每个标准/模板原值，与
    /// `assets/nf_external_card_anchor.csv`（逐行 source）交叉核对。
    #[test]
    fn nf_ext_anchor_values_match_standard_table() {
        let spec = anchor_spec();
        let d = derive(&spec).unwrap();
        let vals = value_map(&spec, [0.0, 0.0]);
        let get = |tag: &str| vals.get(tag).unwrap_or_else(|| panic!("缺 {tag}"));
        assert_eq!(get("执行标准"), "NF E22-141");
        assert_eq!(get("定心方式"), "齿面定心", "模板缺省口径");
        assert_eq!(get("模数"), "7.5");
        assert_eq!(get("齿数"), "38");
        assert_eq!(get("压力角"), "20°");
        assert_eq!(get("齿根样式"), "平齿根");
        assert_eq!(get("加工方法"), "滚齿", "外花键模板口径");
        assert_eq!(get("大径Dee"), "298.5", "A−0.2m（齿面定心）");
        assert_eq!(get("小径Die"), "282", "A−2.4m（平齿根）");
        assert_eq!(get("基准尺寸"), "300");
        assert_eq!(get("跨测齿数K"), "6");
        assert_eq!(get("公法线W"), "129.871");
        // 公差（模板实测 + ISO 286 + p29 外花键 E，µm→mm）
        assert_eq!(get("大径上差"), "0", "ISO 286 h12@Ø298.5");
        assert_eq!(get("大径下差"), "-0.520", "ISO 286 h12@Ø298.5（模板实测 −0.460 为模板另填）");
        assert_eq!(get("小径上差"), "+0.052", "ISO 286 H7@Ø282");
        assert_eq!(get("小径下差"), "0");
        assert_eq!(get("公法线上差"), "+0.042", "p29 m=7.5/A=300 外花键 E 固定 +42/−42");
        assert_eq!(get("公法线下差"), "-0.042");
        // 表值与公式交叉核对（p22 行）
        assert_eq!(d.table_flat_root, Some(282.0));
        assert_eq!(d.table_round_root, Some(279.795));
        assert_eq!(d.x, Some(0.8));
        assert_eq!(d.d, Some(285.0));
        assert_eq!(d.db, Some(267.812));
        assert_eq!(d.s, Some(16.149));
        assert_eq!(d.sb, Some(19.166));
        assert_eq!(d.rf, Some(2.25));
        assert_eq!(d.rr, Some(3.96));
        assert_eq!(d.h, None, "p22 锚点行齿顶倒角高度列为空（如实）");
        assert_eq!(d.k, Some(6.0));
        assert_eq!(d.w, Some(129.871));
        assert!(near(d.dee, 298.5));
        assert!(near(d.die, 282.0));
        // 定心/齿根变体
        let mut outer = spec.clone();
        outer.centering = Centering::Outer;
        let do_ = derive(&outer).unwrap();
        assert!(near(do_.dee, 300.0), "外径定心 Dee=A=300");
        assert_eq!(value_map(&outer, [0.0, 0.0])["定心方式"], "外径定心");
        let mut fillet = spec.clone();
        fillet.root = RootStyle::Fillet;
        let df = derive(&fillet).unwrap();
        assert!(near(df.die, 279.795), "圆齿根 Die=A−2.694m");
        assert_eq!(value_map(&fillet, [0.0, 0.0])["齿根样式"], "圆齿根");
        // p25 检查行对账（K/E；U/F 模板无该行、只作对账）
        let check = crate::invol_spline::nf_check_by_a_m(300.0, 7.5)
            .into_iter()
            .find(|r| r.page == 25 && r.has_check_dims())
            .expect("p25 检查行");
        assert!(near(check.e.unwrap(), 129.871), "模板 W 与 p25 E 逐位一致");
        assert!(near(check.k.unwrap(), 6.0));
        assert!(near(check.u.unwrap(), 13.5));
        assert!(near(check.f.unwrap(), 314.669));

        // ── 逐行对照 assets/nf_external_card_anchor.csv（field→值；1e-9）──
        let text = include_str!("../assets/nf_external_card_anchor.csv");
        let mut checked = 0usize;
        for line in text.lines() {
            if line.trim().is_empty() || line.starts_with('#') || line.starts_with("field,") {
                continue;
            }
            let f: Vec<&str> = line.split(',').collect();
            assert_eq!(f.len(), 10, "锚点 CSV 列数：{line}");
            let field = f[0];
            let want: f64 = f[6].parse().unwrap_or_else(|e| panic!("{line}: {e}"));
            let got: f64 = match field {
                "大径Dee_齿面定心" => d.dee,
                "大径Dee_外径定心" => spec.a,
                "小径Die_平齿根" => d.die,
                "小径Die_圆齿根" => spec.a - 2.694 * spec.m,
                "基准尺寸Do" => spec.a,
                "跨测齿数K" => d.k.unwrap(),
                "公法线W" => d.w.unwrap(),
                "分度圆d" => d.d.unwrap(),
                "基圆dB" => d.db.unwrap(),
                "变位系数x" => d.x.unwrap(),
                "压力角a" => vals["压力角"].trim_end_matches('°').parse().unwrap(),
                "外花键量棒U_对账" => check.u.unwrap(),
                "外花键跨量棒距F_对账" => check.f.unwrap(),
                "大径上差_卡" => {
                    let (u, _) = d.major_tol.unwrap().display();
                    u.parse().unwrap()
                }
                "大径下差_卡" => {
                    let (_, l) = d.major_tol.unwrap().display();
                    l.parse().unwrap()
                }
                "小径上差_卡" => {
                    let (u, _) = d.minor_tol.unwrap().display();
                    u.parse().unwrap()
                }
                "小径下差_卡" => {
                    let (_, l) = d.minor_tol.unwrap().display();
                    l.parse().unwrap()
                }
                other => panic!("锚点 CSV 有未识别的 field「{other}」"),
            };
            assert!(near(got, want), "锚点 {field}: 卡取 {got} ≠ 标准/模板 {want}（{line}）");
            checked += 1;
        }
        assert_eq!(checked, 17, "锚点行数");
    }

    /// 标缺行为：表外 (m,A) 的 K/W/z 一律「—」，p29 表外公法线公差「—」，不外推；
    /// 配合类别只影响公法线公差（不影响其他值）；公式量照给。
    #[test]
    fn nf_ext_missing_is_dash_and_no_extrapolation() {
        // ① 锚点：18 项全有值
        let spec = anchor_spec();
        let j = NfExtTableModel {
            card: "NF外花键参数表".into(),
            expr: None,
            a: spec.a,
            m: spec.m,
            z: spec.z,
            centering: None,
            root: None,
            fit: None,
            at: None,
            rot: 0.0,
        }
        .preview_json()
        .unwrap();
        assert_eq!(j["items"].as_array().unwrap().len(), 18);
        assert_eq!(j["missing"].as_array().unwrap().len(), 0, "锚点 18 项齐全");
        assert_eq!(j["fit"], "fixed", "配合缺省固定");
        assert_eq!(j["centering"], "flank", "定心缺省齿面（模板口径）");
        assert_eq!(j["root"], "flat", "齿根缺省平齿根");
        assert!(
            j["readout"].as_array().unwrap().iter().any(|r| r["v"]
                .as_str()
                .unwrap()
                .contains("6 / 129.871")),
            "读数应含 K/W：{j}"
        );
        assert!(
            j["readout"].as_array().unwrap().iter().any(|r| r["v"]
                .as_str()
                .unwrap()
                .contains("E +42/-42")),
            "读数应含 p29 外花键 E 偏差：{j}"
        );
        // ② 表外：A=210 m=7.5 不在 p20–p22/p23–p25（表内只有 200/220/250…）→
        //    公式量照给，K/W/z 与公法线公差标缺；ISO h12/H7 与 A 无关，不连坐。
        let off = NfExtTableSpec {
            a: 210.0,
            m: 7.5,
            z: None,
            centering: Centering::Flank,
            root: RootStyle::Flat,
            fit: FitClass::Fixed,
            at: None,
            rot: 0.0,
        };
        let d = derive(&off).unwrap();
        assert!(near(d.dee, 208.5) && near(d.die, 192.0), "公式量 Dee/Die 仍给");
        assert_eq!(d.z, None);
        assert_eq!(d.k, None);
        assert_eq!(d.w, None);
        assert!(d.dims_source.contains("不在 p20–p22"));
        assert!(d.check_source.contains("不在 p23–p25"));
        assert!(d.tol_row.is_none(), "A=210 不在 p29 表 → E 公差标缺（不外推）");
        let vals = values(&off).unwrap();
        let g = |t: &str| vals.iter().find(|(x, _)| x == t).unwrap().1.clone();
        assert_eq!(g("齿数"), MISSING);
        assert_eq!(g("跨测齿数K"), MISSING);
        assert_eq!(g("公法线W"), MISSING);
        assert_eq!(g("大径Dee"), "208.5");
        assert_eq!(g("小径Die"), "192");
        assert_eq!(g("公法线上差"), MISSING, "p29 表外 → 公法线公差标缺");
        assert_eq!(g("公法线下差"), MISSING);
        assert_ne!(g("大径下差"), MISSING, "ISO 286 与 A 无关，不连坐");
        assert_ne!(g("小径上差"), MISSING);
        // ③ 配合类别只影响公法线公差
        for (fit, want_u, want_l) in [
            (FitClass::Loose, "-0.11", "-0.194"),
            (FitClass::Slide, "-0.02", "-0.104"),
            (FitClass::Fixed, "+0.042", "-0.042"),
            (FitClass::Press, "+0.138", "+0.054"),
        ] {
            let mut s = spec.clone();
            s.fit = fit;
            let vals = values(&s).unwrap();
            let g = |t: &str| vals.iter().find(|(x, _)| x == t).unwrap().1.clone();
            assert_eq!(g("公法线上差"), want_u, "{fit:?}");
            assert_eq!(g("公法线下差"), want_l, "{fit:?}");
            assert_eq!(g("大径下差"), "-0.520", "{fit:?} 不影响直径公差");
            let pj = NfExtTableModel {
                card: "NF外花键参数表".into(),
                expr: None,
                a: s.a,
                m: s.m,
                z: s.z,
                centering: None,
                root: None,
                fit: Some(fit.label().to_string()),
                at: None,
                rot: 0.0,
            }
            .preview_json()
            .unwrap();
            assert_eq!(pj["fit"], fit.id());
            assert!(
                pj["readout"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["k"].as_str().unwrap().contains(fit.label())),
                "{fit:?} 外花键偏差读数应在"
            );
        }
    }

    /// 报错路径：z 与表值不一致 / z 越界 / 缺 m / 不认识参数 / 非正数 / 选项非法。
    #[test]
    fn nf_ext_errors_are_loud() {
        let e = NfExtTableSpec::parse("A300 M7.5 Z39").unwrap_err();
        assert!(e.contains("不一致") && e.contains("N=38"), "{e}");
        let e = NfExtTableSpec::parse("A300 M7.5 Z2").unwrap_err();
        assert!(e.contains("超出范围"), "{e}");
        let e = NfExtTableSpec::parse("A300").unwrap_err();
        assert!(e.contains("缺模数"), "{e}");
        let e = NfExtTableSpec::parse("M7.5").unwrap_err();
        assert!(e.contains("缺公称直径"), "{e}");
        let e = NfExtTableSpec::parse("A300 M7.5 铭牌").unwrap_err();
        assert!(e.contains("不认识的参数"), "{e}");
        let e = NfExtTableSpec::parse("A-1 M7.5").unwrap_err();
        assert!(e.contains("正数"), "{e}");
        let e = NfExtTableSpec::parse("").unwrap_err();
        assert!(e.contains("OCSMCARD NF外花键参数表"), "空参应给用法：{e}");
        let e = NfExtTableSpec::parse("A300 M7.5 中心 偏置").unwrap_err();
        assert!(e.contains("定心方式"), "{e}");
        let e = NfExtTableSpec::parse("A300 M7.5 根 尖").unwrap_err();
        assert!(e.contains("齿根样式"), "{e}");
        let e = NfExtTableSpec::parse("A300 M7.5 配合 抱").unwrap_err();
        assert!(e.contains("配合类别"), "{e}");
    }

    /// CLI 解析：短记法 + 中文键 + at/rot；数字与别名。
    #[test]
    fn nf_ext_parse_cli_forms() {
        let s = NfExtTableSpec::parse("A300 M7.5 Z38").unwrap();
        assert_eq!(s, anchor_spec());
        let s = NfExtTableSpec::parse("直径 300 模数 7.5 齿数 38 中心 外径 根 圆 at 10,20 rot 30").unwrap();
        assert!(near(s.a, 300.0) && near(s.m, 7.5) && s.z == Some(38));
        assert_eq!(s.centering, Centering::Outer);
        assert_eq!(s.root, RootStyle::Fillet);
        assert_eq!(s.at, Some([10.0, 20.0]));
        assert!(near(s.rot, 30.0));
        let s = NfExtTableSpec::parse("a300 m7.5 z38").unwrap();
        assert_eq!(s, anchor_spec());
        let s = NfExtTableSpec::parse("A300 M7.5 中心 齿面 根 flat").unwrap();
        assert_eq!(s.centering, Centering::Flank);
        assert_eq!(s.root, RootStyle::Flat);
        let s = NfExtTableSpec::parse("A300 M7.5 Z38 配合 松动").unwrap();
        assert_eq!(s.fit, FitClass::Loose);
        let s = NfExtTableSpec::parse("A300 M7.5 fit slide").unwrap();
        assert_eq!(s.fit, FitClass::Slide);
        assert_eq!(strip_prefix_ci("A300", "a").unwrap(), "300");
        assert!(strip_prefix_ci("M7.5", "a").is_none());
        // 用法文案包含关键口径
        let u = usage();
        assert!(u.contains("NF外花键参数表"), "{u}");
        assert!(u.contains("A300 M7.5"), "{u}");
        assert!(u.contains("表外"), "{u}");
        assert!(u.contains("h12") && u.contains("H7"), "{u}");
        assert!(u.contains("滚齿"), "{u}");
        assert!(u.contains("齿面定心"), "{u}");
    }

    /// 文字干涉几何检查（照 NF 内卡/GB 花键口径）：值框不进公差格、标签不越中分隔、
    /// 全部文字在表格行带与外框内；覆盖全量 p20–p22 外花键表行 + 齿面/圆齿根 + 表外长值。
    #[test]
    fn nf_ext_values_clear_tolerance_column() {
        assert_eq!(VALUE_WIDTH_FACTOR, 0.6, "值列实体级字宽（外卡最长值 279.795）");
        assert_eq!(TOL_WIDTH_FACTOR, 0.667, "公差列照模板 TEXT");
        let mut sep: Vec<f64> = crate::nf_table::NF_LINES
            .iter()
            .filter(|(a, b, _)| near5(a[1], b[1]))
            .map(|(_, b, _)| b[1])
            .collect();
        sep.sort_by(|a, b| b.partial_cmp(a).unwrap());
        sep.dedup_by(|a, b| near5(*a, *b));
        let band_of = |y: f64| -> (f64, f64) {
            for w in sep.windows(2) {
                if y <= w[0] + 1e-9 && y >= w[1] - 1e-9 {
                    return (w[0], w[1]);
                }
            }
            panic!("y={y} 不在任何行带");
        };
        // 全量用例：每个 p20–p22 外花键尺寸行（有 N 的全给） + 锚点 + 圆齿根 + 表外
        let mut specs: Vec<NfExtTableSpec> = Vec::new();
        for r in crate::invol_spline::nf_e22141_rows() {
            if (20..=22).contains(&r.page) && r.flat_root.is_some() {
                specs.push(NfExtTableSpec {
                    a: r.a,
                    m: r.m,
                    z: None,
                    centering: Centering::Flank,
                    root: RootStyle::Flat,
                    fit: FitClass::Fixed,
                    at: None,
                    rot: 0.0,
                });
            }
        }
        specs.push(anchor_spec());
        let mut fillet = anchor_spec();
        fillet.root = RootStyle::Fillet;
        specs.push(fillet);
        let mut outer = anchor_spec();
        outer.centering = Centering::Outer;
        specs.push(outer);
        // 表外长值：A=210 → “208.5”/“192”，加 K/W 标缺的「—」
        specs.push(NfExtTableSpec {
            a: 210.0,
            m: 7.5,
            z: None,
            centering: Centering::Flank,
            root: RootStyle::Flat,
            fit: FitClass::Fixed,
            at: None,
            rot: 0.0,
        });
        assert!(specs.len() > 140, "覆盖全量 p20–p22 表行（实 {}）", specs.len());

        let overlap = |a: [f64; 4], b: [f64; 4]| a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
        let mid_box = |x: f64, y: f64, h: f64, wf: f64, v: &str| {
            [x, y - h / 2.0, x + text_extent(v, h, wf), y + h / 2.0]
        };
        let base_box = |x: f64, y: f64, h: f64, wf: f64, v: &str| {
            [x, y, x + text_extent(v, h, wf), y + h]
        };
        for spec in &specs {
            let ins = build_insert(spec, [0.0, 0.0], 0.0).unwrap();
            let attrs: HashMap<String, _> =
                ins.attributes.iter().map(|a| (a.tag.clone(), a.clone())).collect();
            let vals = values(spec).unwrap();
            let vmap: HashMap<&str, &str> = vals.iter().map(|(t, v)| (t.as_str(), v.as_str())).collect();
            for (i, (_, tag, y)) in NF_EXT_ROWS.iter().enumerate() {
                let v = vmap[*tag];
                let b = mid_box(VALUE_X, *y, TEXT_H, VALUE_WIDTH_FACTOR, v);
                let (top, bottom) = band_of(*y);
                assert!(
                    b[1] >= bottom - 1e-9 && b[3] <= top + 1e-9,
                    "{tag}={v} 框 {b:?} 出行为 [{bottom}, {top}]"
                );
                assert!(b[2] <= 0.0 - 0.5, "{tag}={v} 右边界 {:.3} 出右框", b[2]);
                assert!(b[0] >= MID_X + 0.5, "{tag}={v} 左边界进标签列");
                // 同行公差洞位（大径行=第 8 行 / 小径=9 / 公法线=11，0-based）
                if let Some(tol_pairs) = [
                    (8usize, "大径上差", "大径下差"),
                    (9, "小径上差", "小径下差"),
                    (11, "公法线上差", "公法线下差"),
                ]
                .iter()
                .find(|(ri, _, _)| *ri == i)
                {
                    for tol_tag in [tol_pairs.1, tol_pairs.2] {
                        let ta = attrs.get(tol_tag).unwrap();
                        let tb = base_box(
                            ta.insertion_point.x,
                            ta.insertion_point.y,
                            TOL_H,
                            TOL_WIDTH_FACTOR,
                            &ta.value,
                        );
                        assert!(
                            !overlap(b, tb),
                            "{tag}={v} 与 {tol_tag}={} 叠字：值右 {:.3} / 公差左 {:.3}",
                            ta.value,
                            b[2],
                            tb[0]
                        );
                        let (ttop, tbottom) = band_of(ta.insertion_point.y + TOL_H / 2.0);
                        assert!(
                            tb[1] >= tbottom - 1e-9 && tb[3] <= ttop + 1e-9,
                            "{tol_tag} 框出行为 [{tbottom}, {ttop}]"
                        );
                    }
                }
            }
            for (label, _, y) in NF_EXT_ROWS {
                let b = mid_box(LABEL_X, *y, TEXT_H, 0.7, label);
                assert!(b[0] >= FRAME.0 + 0.5, "标签 {label} 出左框");
                assert!(b[2] <= MID_X - 0.5, "标签 {label} 越中分隔：右 {:.3}", b[2]);
                let (top, bottom) = band_of(*y);
                assert!(b[1] >= bottom - 1e-9 && b[3] <= top + 1e-9, "标签 {label} 出行为");
            }
        }
        // 标题框：行 1 内、不越外框
        let half = text_extent("外花键参数表", TEXT_H, 0.7) / 2.0;
        let tbox = [MID_X - half, TITLE_AT.1 - TEXT_H / 2.0, MID_X + half, TITLE_AT.1 + TEXT_H / 2.0];
        assert!(tbox[0] >= FRAME.0 + 0.5 && tbox[2] <= FRAME.1 - 0.5, "标题出框");
        let (ttop, tbottom) = band_of(TITLE_AT.1);
        assert!(tbox[1] >= tbottom - 1e-9 && tbox[3] <= ttop + 1e-9, "标题出行为");
    }

    /// 值/标签文本宽度估计：字符推进宽度（em，保守上限，与 NF 内卡/GB 花键卡同款）。
    fn char_em(c: char) -> f64 {
        if c.is_ascii_digit() || c == '.' || c == '+' || c == '-' {
            0.55
        } else if c == ' ' {
            0.25
        } else if c.is_ascii() {
            0.75
        } else {
            1.0 // CJK / 全角
        }
    }

    fn text_extent(value: &str, height: f64, width_factor: f64) -> f64 {
        value.chars().map(char_em).sum::<f64>() * height * width_factor
    }

    /// 预览 JSON 形状（GUI 只渲染）+ 出表 ATTRIB 数 + 待放置件元数据。
    #[test]
    fn nf_ext_preview_and_pending_shape() {
        let m = NfExtTableModel {
            card: "NF外花键参数表".into(),
            expr: None,
            a: 300.0,
            m: 7.5,
            z: Some(38),
            centering: None,
            root: None,
            fit: None,
            at: Some([12.0, 34.0]),
            rot: 0.0,
        };
        let j = m.preview_json().unwrap();
        assert_eq!(j["ok"], true);
        assert_eq!(j["card"], "NF外花键参数表");
        assert_eq!(j["renderer"], "nf_ext_table");
        assert_eq!(j["centering"], "flank");
        assert_eq!(j["root"], "flat");
        assert_eq!(j["items"].as_array().unwrap().len(), 18);
        assert!(
            j["readout"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["v"].as_str().unwrap().contains("298.5")),
            "读数应含 Dee：{j}"
        );
        assert!(
            j["readout"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["v"].as_str().unwrap().contains("129.871")),
            "读数应含 W：{j}"
        );
        let ins = m.build_insert().unwrap();
        assert_eq!(ins.block_name, BLOCK);
        assert_eq!(ins.attributes.len(), 18);
        // 多重集断言：preview items 的 tag 集合 == ATTDEF tag 集合
        let mut item_tags: Vec<String> = j["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|it| it["tag"].as_str().unwrap().to_string())
            .collect();
        let mut att_tags: Vec<String> = attdefs().iter().map(|ad| ad.tag.clone()).collect();
        item_tags.sort();
        att_tags.sort();
        assert_eq!(item_tags, att_tags, "口径表 ↔ 属性表多重集不一致");
        assert_eq!(att_tags.len(), 18);
        assert_eq!(m.pending_attrs().unwrap().len(), 18);
        let meta = m.part_meta_json().unwrap();
        assert!(meta.contains("\"family\":\"nf_ext_table\""));
        let echo = m.echo_note().unwrap();
        assert!(echo.contains("A=300") && echo.contains("z=38") && echo.contains("滚齿"), "{echo}");
        let md = markdown_table(&m.spec().unwrap()).unwrap();
        assert_eq!(md.lines().count(), 20, "18 行 + 表头 + 分隔");
    }

    fn expr_model(expr: &str) -> NfExtTableModel {
        NfExtTableModel {
            card: "NF外花键参数表".into(),
            expr: Some(expr.into()),
            a: 0.0,
            m: 0.0,
            z: None,
            centering: None,
            root: None,
            fit: None,
            at: None,
            rot: 0.0,
        }
    }

    /// ★ 表达式 → 字段 → 卡内值：A=m(z+0.4+2x)、m、z；锚点 A300/M7.5/Z38 逐项；
    /// KIND 恒外（EX）、MARK/α 错误路径。
    #[test]
    fn nf_ext_expr_maps_to_anchor_and_errors() {
        let spec = expr_model("SPLINE EX M7.5 Z38 ALPHA20 X0.8 BETA0 H30")
            .spec()
            .unwrap();
        assert!((spec.a - 300.0).abs() < 1e-9, "A={}", spec.a);
        assert!((spec.m - 7.5).abs() < 1e-12);
        assert_eq!(spec.z, Some(38));
        assert_eq!(spec.centering, Centering::Flank, "缺省齿面定心");
        let vals = values(&spec).unwrap();
        let get = |tag: &str| vals.iter().find(|(t, _)| t == tag).unwrap().1.clone();
        assert_eq!(get("模数"), "7.5");
        assert_eq!(get("齿数"), "38");
        assert_eq!(get("压力角"), "20°");
        assert_eq!(get("大径Dee"), "298.5");
        assert_eq!(get("小径Die"), "282");
        assert_eq!(get("跨测齿数K"), "6");
        assert_eq!(get("公法线W"), "129.871");
        // 与旧 A/M/Z 输入同值（CLI 两条路）
        let old = NfExtTableSpec::parse("A300 M7.5 Z38").unwrap();
        assert_eq!(values(&old).unwrap(), values(&spec).unwrap());
        // 预览回填 fields + expr 回显
        let pj = expr_model("SPLINE EX M7.5 Z38 ALPHA20 X0.8 BETA0 H30")
            .preview_json()
            .unwrap();
        assert!(near(pj["fields"]["a"].as_f64().unwrap(), 300.0));
        assert!(near(pj["fields"]["m"].as_f64().unwrap(), 7.5));
        assert_eq!(pj["fields"]["z"], 38);
        assert!(pj["expr"].as_str().unwrap().starts_with("SPLINE EX"));
        // CLI 表达式写法（+ 选项）
        let cli = NfExtTableSpec::parse("SPLINE EX M7.5 Z38 ALPHA20 X0.8 BETA0 H30 中心 外径").unwrap();
        assert!(near(cli.a, 300.0) && cli.z == Some(38));
        assert_eq!(cli.centering, Centering::Outer);
        // KIND 写 IN → 与「外」不符
        let e = expr_model("SPLINE IN M7.5 Z38 ALPHA20 X0.8 BETA0 H30")
            .spec()
            .unwrap_err();
        assert!(e.contains("KIND") && e.contains("IN（内）") && e.contains("「外」"), "{e}");
        // MARK 体系不符
        let e = expr_model("GEAR EX M7.5 Z38 ALPHA20 X0.8 BETA0 H30")
            .spec()
            .unwrap_err();
        assert!(e.contains("MARK") && e.contains("GEAR（齿轮）"), "{e}");
        // α 必须 20
        let e = expr_model("SPLINE EX M7.5 Z38 ALPHA30 X0.8 BETA0 H30")
            .spec()
            .unwrap_err();
        assert!(e.contains("压力角 30°") && e.contains("α=20"), "{e}");
        // CLI 显式冲突
        let e = NfExtTableSpec::parse("SPLINE EX M7.5 Z38 ALPHA20 X0.8 BETA0 H30 Z40").unwrap_err();
        assert!(e.contains("显式 z=40") && e.contains("z=38"), "{e}");
        // 表外（表达式 z=40 → A=315，不在 p20–p22 表）→ K/W 如实标「—」，不报错也不臆造
        let off = expr_model("SPLINE EX M7.5 Z40 ALPHA20 X0.8 BETA0 H30")
            .spec()
            .unwrap();
        assert!(near(off.a, 315.0));
        let vals = values(&off).unwrap();
        let get = |tag: &str| vals.iter().find(|(t, _)| t == tag).unwrap().1.clone();
        assert_eq!(get("跨测齿数K"), MISSING);
        assert_eq!(get("公法线W"), MISSING);
    }
}
