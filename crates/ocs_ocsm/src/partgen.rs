//! 标准件**参数化生成**（GB/T 4458 简化画法，与库内 ACM 件风格一致）。
//!
//! 为什么参数化：图库缺的恰好是最常用的族（普通六角螺母/平键/轴承/挡圈 =
//! 0 个目录；GB 5782/5783 只有 M5/M6；内六角螺钉只有 M1.6/M2）。
//! 尺寸数据取国标的采标源（GB/T 5782←ISO 4014、GB/T 6170←ISO 4032…），
//! 见 `src/tables/parts*.json`。
//!
//! ## 画法约定（全族统一，便于选择器/BOM 复用）
//! - **基点落在原点**：螺栓 = 头部支承面 × 轴线；螺母 = 底面 × 轴线；垫圈/销 = 端面中心。
//! - **图层**：轮廓 → `1轮廓实线层`；牙底/细线 → `2细线层`；中心线 → `3中心线层`；
//!   颜色/线型/线宽一律 ByLayer。
//! - **螺纹画法（GB/T 4459.1）**：外螺纹 牙顶（大径 d）粗实线、牙底（小径 d1）细实线；
//!   内螺纹反过来（小径粗、大径细）。小径 d1 ≈ d − 1.0825·P。
//! - 螺栓主视图：轴线水平、头在 −x 侧、杆向 +x 伸长；六角头端面画 s 宽，
//!   外轮廓画对角宽 e = s/cos30°，两端 30° 倒角斜线。
//! - 螺母主视图：底面 y=0、顶面 y=m，对边宽 s 居中，四角 30° 倒角。

use ocs_plugin_api::host::acadrust;
use ocs_plugin_api::host::acadrust::entities::{Arc, Circle, EntityType, Line};
use ocs_plugin_api::host::acadrust::types::{Color, LineWeight, Vector3};

/// 轮廓（粗实线）层。
pub const LAYER_MAIN: &str = "1轮廓实线层";
/// 细实线层（牙底、细线）。
pub const LAYER_THIN: &str = "2细线层";
/// 中心线层（点划线）。
pub const LAYER_CENTER: &str = "3中心线层";

const COS30: f64 = 0.866_025_403_784_438_6;
/// 库内 ACM 件实测：六角头倒角弧半径 = 0.72 × 对边宽。
const CHAMFER_R: f64 = 0.72;
/// 库内 ACM 件实测：倒角弧的一半张角（度）。
const CHAMFER_HALF_DEG: f64 = 20.3;

/// 外/内螺纹小径（GB/T 197 简化式）。
pub fn minor_dia(d: f64, pitch: f64) -> f64 {
    d - 1.0825 * pitch
}

/// 六角头对角宽度（公称，CAD 画图用；表里的 e 是公差下限）。
pub fn across_corners(s: f64) -> f64 {
    s / COS30
}

/// 一条线（落在指定图层）。
fn line(a: [f64; 2], b: [f64; 2], layer: &str) -> EntityType {
    let mut l = Line::from_points(Vector3::new(a[0], a[1], 0.0), Vector3::new(b[0], b[1], 0.0));
    set_layer(&mut l, layer);
    EntityType::Line(l)
}

/// 圆弧：入参用**度**（与 DXF/GB 画法一致），内部转弧度（acadrust 的角度字段是弧度）。
fn arc(c: [f64; 2], r: f64, start_deg: f64, end_deg: f64, layer: &str) -> EntityType {
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
fn circle(c: [f64; 2], r: f64, layer: &str) -> EntityType {
    let mut e = Circle::from_center_radius(Vector3::new(c[0], c[1], 0.0), r);
    set_layer(&mut e, layer);
    EntityType::Circle(e)
}

fn set_layer<T: CommonLayer>(e: &mut T, layer: &str) {
    let c = e.common_mut();
    c.layer = layer.to_string();
    c.color = Color::ByLayer;
    c.linetype = "ByLayer".to_string();
    c.line_weight = LineWeight::ByLayer;
}

/// 让 set_layer 同时适配 Line/Arc/Circle。
trait CommonLayer {
    fn common_mut(&mut self) -> &mut acadrust::entities::EntityCommon;
}

impl CommonLayer for Line {
    fn common_mut(&mut self) -> &mut acadrust::entities::EntityCommon {
        &mut self.common
    }
}

impl CommonLayer for Arc {
    fn common_mut(&mut self) -> &mut acadrust::entities::EntityCommon {
        &mut self.common
    }
}

impl CommonLayer for Circle {
    fn common_mut(&mut self) -> &mut acadrust::entities::EntityCommon {
        &mut self.common
    }
}

// ── 数据表 ─────────────────────────────────────────────────────────────────

/// 六角头螺栓一行（GB/T 5782 ← ISO 4014）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BoltRow {
    /// 公称直径
    pub d: f64,
    /// 螺距
    #[serde(rename = "P")]
    pub pitch: f64,
    /// 对边宽度
    pub s: f64,
    /// 头高
    pub k: f64,
    /// 对角宽度（最小）
    pub e: f64,
    /// 垫圈面直径
    pub dw: f64,
    /// 螺纹长度（l ≤ 125）
    pub b1: f64,
    /// 螺纹长度（125 < l ≤ 200）
    pub b2: f64,
    /// 螺纹长度（l > 200）
    pub b3: f64,
    pub l_min: f64,
    pub l_max: f64,
}

/// 1 型六角螺母一行（GB/T 6170 ← ISO 4032）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct NutRow {
    pub d: f64,
    #[serde(rename = "P")]
    pub pitch: f64,
    pub s: f64,
    pub e: f64,
    /// 螺母高度
    pub m: f64,
    /// 扳手面高度
    pub mw: f64,
    /// 孔口直径上限
    pub damax: f64,
    pub dw: f64,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Table<T> {
    #[allow(dead_code)]
    family: String,
    #[allow(dead_code)]
    code: String,
    #[allow(dead_code)]
    iso: String,
    rows: Vec<T>,
}

fn hex_bolt_table() -> &'static Table<BoltRow> {
    static T: std::sync::OnceLock<Table<BoltRow>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        serde_json::from_str(include_str!("tables/partsHexBolt.json")).expect("partsHexBolt.json")
    })
}

fn hex_nut_table() -> &'static Table<NutRow> {
    static T: std::sync::OnceLock<Table<NutRow>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        serde_json::from_str(include_str!("tables/partsHexNut.json")).expect("partsHexNut.json")
    })
}

/// 六角头螺栓可选公称直径。
pub fn bolt_diameters() -> Vec<f64> {
    hex_bolt_table().rows.iter().map(|r| r.d).collect()
}

/// 六角螺母可选公称直径。
pub fn nut_diameters() -> Vec<f64> {
    hex_nut_table().rows.iter().map(|r| r.d).collect()
}

pub fn bolt_row(d: f64) -> Option<&'static BoltRow> {
    hex_bolt_table().rows.iter().find(|r| (r.d - d).abs() < 1e-9)
}

pub fn nut_row(d: f64) -> Option<&'static NutRow> {
    hex_nut_table().rows.iter().find(|r| (r.d - d).abs() < 1e-9)
}

/// 标准长度系列（简化规则：≤20 步长 2 / ≤70 步长 5 / ≤160 步长 10 / 其余步长 20），
/// 裁剪到该直径的 l_min…l_max。
pub fn standard_lengths(row: &BoltRow) -> Vec<f64> {
    // 步长按"当前值所在区间"取，保证 70 的下一档是 80（而不是 75），与 ISO/GB 的首选系列一致。
    let step = |l: f64| if l < 20.0 { 2.0 } else if l < 70.0 { 5.0 } else if l < 160.0 { 10.0 } else { 20.0 };
    let mut out = Vec::new();
    let mut l = row.l_min;
    while l <= row.l_max + 1e-9 {
        out.push(l);
        l += step(l);
    }
    if out.last().map(|x| (*x - row.l_max).abs() > 1e-9).unwrap_or(false) {
        out.push(row.l_max);
    }
    out
}

/// 螺纹长度（GB/T 5782 规则：l≤125 → b1；125<l≤200 → b2；l>200 → b3）。
pub fn thread_length(row: &BoltRow, l: f64) -> f64 {
    if l <= 125.0 {
        row.b1
    } else if l <= 200.0 {
        row.b2
    } else {
        row.b3
    }
}

// ── 生成结果 ───────────────────────────────────────────────────────────────

/// 零件的 BOM 元数据（写入 `OCSM_PART`；明细表/序号直接取用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartMeta {
    /// 现行代号，如 `GB/T 5782-2016`
    pub code: String,
    /// 名称，如 `六角头螺栓`
    pub name: String,
    /// 规格，如 `M10x40`
    pub spec: String,
    /// 材料（默认空，由数据表补）
    pub material: String,
    /// 单件重量 kg（估算值，前缀 `≈`）
    pub weight: String,
}

/// 一次生成的插入方案。
#[derive(Debug, Clone)]
pub struct GenPart {
    /// 图元（已落在 OCSM 图层上，ByLayer）
    pub entities: Vec<EntityType>,
    pub meta: PartMeta,
    /// 包围盒 [xmin, ymin, xmax, ymax]
    pub bbox: [f64; 4],
}

/// 规格文本：螺栓 `M10x40`，螺母 `M10`。
pub fn spec_text_bolt(d: f64, l: f64) -> String {
    format!("M{}x{}", trim_num(d), trim_num(l))
}

pub fn spec_text_dia(d: f64) -> String {
    format!("M{}", trim_num(d))
}

fn trim_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// 六角头螺栓（`GB/T 5782-2016`；`full_thread=true` 时按全螺纹 `GB/T 5783` 画）。
pub fn hex_bolt(d: f64, l: f64, full_thread: bool) -> Result<GenPart, String> {
    let Some(row) = bolt_row(d) else {
        return Err(format!("GB/T 5782 数据表里没有 M{}", trim_num(d)));
    };
    if l < row.l_min - 1e-9 || l > row.l_max + 1e-9 {
        return Err(format!(
            "M{} 的长度应在 {}~{}（标准范围）",
            trim_num(d),
            trim_num(row.l_min),
            trim_num(row.l_max)
        ));
    }
    let (s, k) = (row.s, row.k);
    let d1 = minor_dia(d, row.pitch);
    let b = if full_thread { l } else { thread_length(row, l) };
    let end = (d * 0.1).max(0.5); // 杆端倒角

    let mut entities = Vec::new();
    // 头部（端面在 x=-k，支承面在 y 轴）——按库内 ACM 件实测画法：
    // 外形宽 = 对边 s；端面竖线跨 s；上下轮廓从 x=-k+R(1-cos20.3°) 起；
    // 倒角弧 圆心 (-k+R, ±0.25s)、半径 R=s×0.72、张角 ±20.3°（两弧在轴线相接）。
    let (rr, half) = (s * CHAMFER_R, CHAMFER_HALF_DEG);
    let x_out = -k + rr * (1.0 - half.to_radians().cos());
    entities.push(line([x_out, s / 2.0], [0.0, s / 2.0], LAYER_MAIN));
    entities.push(line([x_out, -s / 2.0], [0.0, -s / 2.0], LAYER_MAIN));
    entities.push(line([-k, s / 2.0], [-k, -s / 2.0], LAYER_MAIN));
    entities.push(arc([-k + rr, s * 0.25], rr, 180.0 - half, 180.0 + half, LAYER_MAIN));
    entities.push(arc([-k + rr, -s * 0.25], rr, 180.0 - half, 180.0 + half, LAYER_MAIN));
    // 头部中棱线（两棱面交线的投影）
    entities.push(line([x_out, 0.0], [0.0, 0.0], LAYER_MAIN));
    // 头/杆交界
    entities.push(line([0.0, s / 2.0], [0.0, -s / 2.0], LAYER_MAIN));
    entities.push(line([0.0, d / 2.0], [0.0, -d / 2.0], LAYER_MAIN));
    // 杆（光杆段轮廓 + 杆端倒角 + 端面）
    entities.push(line([0.0, d / 2.0], [l, d / 2.0], LAYER_MAIN));
    entities.push(line([0.0, -d / 2.0], [l, -d / 2.0], LAYER_MAIN));
    entities.push(line([l - end, d / 2.0], [l, d / 2.0 - end], LAYER_MAIN));
    entities.push(line([l - end, -d / 2.0], [l, -d / 2.0 + end], LAYER_MAIN));
    entities.push(line([l, d / 2.0 - end], [l, -d / 2.0 + end], LAYER_MAIN));
    // 螺纹：牙底细实线（GB/T 4459.1 外螺纹牙底）
    entities.push(line([l - b, d1 / 2.0], [l, d1 / 2.0], LAYER_THIN));
    entities.push(line([l - b, -d1 / 2.0], [l, -d1 / 2.0], LAYER_THIN));
    // 螺纹收尾斜线（细实线，斜接到大径）
    let runout = row.pitch * 1.5;
    if !full_thread && l - b - runout > 0.0 {
        entities.push(line([l - b - runout, d / 2.0], [l - b, d1 / 2.0], LAYER_THIN));
        entities.push(line([l - b - runout, -d / 2.0], [l - b, -d1 / 2.0], LAYER_THIN));
    }
    if !full_thread {
        // 螺纹终止线（粗实线）
        entities.push(line([l - b, d / 2.0], [l - b, -d / 2.0], LAYER_MAIN));
    }
    // 中心线（两端各出头 ~2d）
    let over = (d * 0.5).max(1.0);
    entities.push(line([-k - over, 0.0], [l + over, 0.0], LAYER_CENTER));

    let (code, name) = if full_thread {
        ("GB/T 5783-2016", "六角头螺栓 全螺纹")
    } else {
        ("GB/T 5782-2016", "六角头螺栓")
    };
    Ok(GenPart {
        entities,
        meta: PartMeta {
            code: code.to_string(),
            name: name.to_string(),
            spec: spec_text_bolt(d, l),
            material: String::new(),
            weight: format!("≈{}", format!("{:.3}", bolt_weight_kg(row, l))),
        },
        bbox: [-k, -s / 2.0, l, s / 2.0],
    })
}

/// 1 型六角螺母（`GB/T 6170-2015`）主视图：底面 y=0、顶面 y=m，对边宽 s。
pub fn hex_nut(d: f64) -> Result<GenPart, String> {
    let Some(row) = nut_row(d) else {
        return Err(format!("GB/T 6170 数据表里没有 M{}", trim_num(d)));
    };
    let (s, m) = (row.s, row.m);
    let d1 = minor_dia(d, row.pitch);

    let mut entities = Vec::new();
    // 外形
    entities.push(line([-s / 2.0, 0.0], [s / 2.0, 0.0], LAYER_MAIN));
    entities.push(line([-s / 2.0, m], [s / 2.0, m], LAYER_MAIN));
    entities.push(line([-s / 2.0, 0.0], [-s / 2.0, m], LAYER_MAIN));
    entities.push(line([s / 2.0, 0.0], [s / 2.0, m], LAYER_MAIN));
    // 四角 30° 倒角
    // 顶/底面的倒角弧（构造同螺栓头，旋转 90°）：圆心 (±0.25s, y0∓R)、半径 R=0.72s、张角 ±20.3°
    let (rr, half) = (s * CHAMFER_R, CHAMFER_HALF_DEG);
    let y_in_top = m - rr * (1.0 - half.to_radians().cos());
    let y_in_bot = rr * (1.0 - half.to_radians().cos());
    entities.push(arc([s * 0.25, m - rr], rr, 90.0 - half, 90.0 + half, LAYER_MAIN));
    entities.push(arc([-s * 0.25, m - rr], rr, 90.0 - half, 90.0 + half, LAYER_MAIN));
    entities.push(arc([s * 0.25, rr], rr, -90.0 - half, -90.0 + half, LAYER_MAIN));
    entities.push(arc([-s * 0.25, rr], rr, -90.0 - half, -90.0 + half, LAYER_MAIN));
    let _ = (y_in_top, y_in_bot);
    // 内螺纹：牙底（小径）粗实线、牙顶（大径）细实线
    entities.push(line([-d1 / 2.0, 0.0], [-d1 / 2.0, m], LAYER_MAIN));
    entities.push(line([d1 / 2.0, 0.0], [d1 / 2.0, m], LAYER_MAIN));
    entities.push(line([-d / 2.0, 0.0], [-d / 2.0, m], LAYER_THIN));
    entities.push(line([d / 2.0, 0.0], [d / 2.0, m], LAYER_THIN));
    // 中心线（竖）
    let over = (d * 0.5).max(1.0);
    entities.push(line([0.0, -over], [0.0, m + over], LAYER_CENTER));

    Ok(GenPart {
        entities,
        meta: PartMeta {
            code: "GB/T 6170-2015".to_string(),
            name: "1型六角螺母".to_string(),
            spec: spec_text_dia(d),
            material: String::new(),
            weight: format!("≈{}", format!("{:.3}", nut_weight_kg(row))),
        },
        bbox: [-s / 2.0, 0.0, s / 2.0, m],
    })
}

/// 单件重量估算（钢 7.85 g/cm³；螺栓 = 头 + 杆 − 螺纹牙底修正）。
pub fn bolt_weight_kg(row: &BoltRow, l: f64) -> f64 {
    let head = COS30 * row.s * row.s * row.k; // 六角棱柱体积
    let shank = std::f64::consts::PI / 4.0 * row.d * row.d * (l - row.k).max(0.0);
    (head + shank) * 7.85e-3 / 1000.0
}

/// 单件重量估算（钢）。
pub fn nut_weight_kg(row: &NutRow) -> f64 {
    let hex = COS30 * row.s * row.s * row.m;
    let hole = std::f64::consts::PI / 4.0 * minor_dia(row.d, row.pitch).powi(2) * row.m;
    (hex - hole).max(0.0) * 7.85e-3 / 1000.0
}

// ── 六角头螺栓 C 级（GB/T 5780-2016）─────────────────────────────────────────
//
// 画法由用户提供的三视图 DXF 反解（M5x25 逐条对齐），三条规则把全部圆弧唯一确定：
//   ① 倒角水平投影  Δ = (e − s)/2 · tan30°
//   ② 主视图（对角朝前，e 宽）：角弧半径 r= (e/8)/sin(θ/2 的两倍)…实际取 θ=2·atan(8Δ/e)、
//      r=(e/8)/sin θ；端面倒角大弧 r1 = u+Δ（u=((e/4)²−Δ²)/(2Δ)），张角 ±atan((e/4)/u)
//   ③ 俯视图（对边朝前，s 宽）：弧 r=(s/4)/sin θ、θ=2·atan(4Δ/s)，弧心 (−k+r, ±s/4)
// 螺纹按 GB/T 4459.1 简化画法：牙底 = 0.85d（细实线），收尾斜线长度取标准表 runout。

/// 螺栓视图。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoltView {
    /// 主视图：轴线水平、六角头对角朝前（外形宽 = 对角 e）
    Main,
    /// 俯视图：轴线水平、六角头对边朝前（外形宽 = 对边 s）
    Top,
    /// 左视图：端视图（六边形 + 端面倒角圆）
    End,
}

/// 六角头螺栓 C 级一行（GB/T 5780 ← ISO 4016）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BoltCRow {
    pub d: f64,
    #[serde(rename = "P")]
    pub pitch: f64,
    pub s: f64,
    pub k: f64,
    /// 对角宽度（公差下限）
    pub e: f64,
    pub b1: f64,
    pub b2: f64,
    pub b3: f64,
    pub l_min: f64,
    pub l_max: f64,
    /// 螺纹收尾（退刀）段长度
    pub runout: f64,
    /// 该直径的标准长度系列
    pub lengths: Vec<f64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct BoltCTable {
    #[allow(dead_code)]
    family: String,
    #[allow(dead_code)]
    grade: String,
    #[allow(dead_code)]
    code: String,
    #[allow(dead_code)]
    iso: String,
    rows: Vec<BoltCRow>,
}

fn hex_bolt_c_table() -> &'static BoltCTable {
    static T: std::sync::OnceLock<BoltCTable> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        serde_json::from_str(include_str!("tables/partsHexBoltC.json")).expect("partsHexBoltC.json")
    })
}

/// C 级螺栓可选的公称直径。
pub fn hex_bolt_c_diameters() -> Vec<f64> {
    hex_bolt_c_table().rows.iter().map(|r| r.d).collect()
}

/// 某直径的标准长度系列。
pub fn hex_bolt_c_lengths(d: f64) -> Vec<f64> {
    hex_bolt_c_table()
        .rows
        .iter()
        .find(|r| (r.d - d).abs() < 1e-9)
        .map(|r| r.lengths.clone())
        .unwrap_or_default()
}

pub fn hex_bolt_c_row(d: f64) -> Option<&'static BoltCRow> {
    hex_bolt_c_table().rows.iter().find(|r| (r.d - d).abs() < 1e-9)
}

/// 螺纹长度（GB/T 5780：l ≤ 125 → b1；125 < l ≤ 200 → b2；l > 200 → b3）。
pub fn thread_length_c(row: &BoltCRow, l: f64) -> f64 {
    if l <= 125.0 {
        row.b1
    } else if l <= 200.0 {
        row.b2
    } else {
        row.b3
    }
}

/// 六角头倒角水平投影 Δ = (e − s)/2 · tan30°（用户图实测：M5 0.357、M16 库件 0.716）。
pub fn chamfer_run(s: f64, e: f64) -> f64 {
    (e - s) / 2.0 * 30f64.to_radians().tan()
}

/// 生成 GB/T 5780-2016（C 级）六角头螺栓的一个视图。
///
/// - 基点：主/俯视图 = 头部支承面 × 轴线（原点）；左视图 = 头部中心（原点）
/// - 图元只落 `1轮廓实线层`/`2细线层`/`3中心线层`，颜色/线型/线宽 ByLayer
/// - **不含任何尺寸标注**（用户要求：调用零件时不显示尺寸）
pub fn hex_bolt_c(d: f64, l: f64, view: BoltView) -> Result<GenPart, String> {
    let Some(row) = hex_bolt_c_row(d) else {
        return Err(format!("GB/T 5780 数据表里没有 M{}", trim_num(d)));
    };
    if l < row.l_min - 1e-9 || l > row.l_max + 1e-9 {
        return Err(format!(
            "M{} 的长度应在 {}~{} 之间",
            trim_num(d),
            trim_num(row.l_min),
            trim_num(row.l_max)
        ));
    }
    let (s, k) = (row.s, row.k);
    // 几何用**公称对角宽** e = s/cos30°（表里的 e 是公差下限：M5 8.63 vs 公称 9.2376，
    // 用户图实测轮廓在 ±4.619 = ±e_公称/2）
    let e = across_corners(s);
    let dm = 0.85 * d; // GB/T 4459.1 简化画法：牙底 ≈ 0.85d
    let delta = chamfer_run(s, e);
    let x_out = -k + delta;
    // 中心线出头固定 3.0（实测：5780 M5 = 3.0、5783 M10 = 3.0、32.1 M18 = 3.0）
    let over = 3.0;

    let mut entities = Vec::new();
    match view {
        BoltView::Main => {
            // 头：端面（s 宽）、轮廓（e 宽）、斜线、角弧、端面倒角大弧、棱线、交界线
            entities.push(line([-k, s / 2.0], [-k, -s / 2.0], LAYER_MAIN));
            entities.push(line([x_out, e / 2.0], [0.0, e / 2.0], LAYER_MAIN));
            entities.push(line([x_out, -e / 2.0], [0.0, -e / 2.0], LAYER_MAIN));
            entities.push(line([-k, s / 2.0], [x_out, e / 2.0], LAYER_MAIN));
            entities.push(line([-k, -s / 2.0], [x_out, -e / 2.0], LAYER_MAIN));
            // 角弧：θ = 2·atan(8Δ/e)，r = (e/8)/sinθ，弧心 (−k+r, ±0.375e)，从 180°−θ 到 180°
            let th = 2.0 * (8.0 * delta / e).atan();
            let r2 = (e / 8.0) / th.sin();
            // 下角弧：棱线 (x_out, −e/4) → 端面 (−k, −0.375e)
            entities.push(arc([-k + r2, -0.375 * e], r2, 180.0 - th.to_degrees(), 180.0, LAYER_MAIN));
            // 上角弧：下角弧关于轴线的**镜像**（端面 (−k, +0.375e) → 棱线/大弧交点 (x_out, +e/4)）
            entities.push(arc([-k + r2, 0.375 * e], r2, 180.0, 180.0 + th.to_degrees(), LAYER_MAIN));
            // 端面倒角大弧：u = ((e/4)²−Δ²)/(2Δ)，r1 = u+Δ，张角 ±atan((e/4)/u)
            let u = ((e / 4.0).powi(2) - delta * delta) / (2.0 * delta);
            let r1 = u + delta;
            let al = (e / 4.0 / u).atan().to_degrees();
            entities.push(arc([-k + r1, 0.0], r1, 180.0 - al, 180.0 + al, LAYER_MAIN));
            // 头内部棱线（六边形侧棱的投影，位于 ±e/4）
            entities.push(line([x_out, e / 4.0], [0.0, e / 4.0], LAYER_MAIN));
            entities.push(line([x_out, -e / 4.0], [0.0, -e / 4.0], LAYER_MAIN));
            // 头/杆交界
            entities.push(line([0.0, e / 2.0], [0.0, -e / 2.0], LAYER_MAIN));
        }
        BoltView::Top => {
            entities.push(line([-k, s / 2.0], [-k, -s / 2.0], LAYER_MAIN));
            // 轮廓线从端面竖线 x=−k 起（用户规范图：穿过倒角弧终点直到底面）
            entities.push(line([-k, s / 2.0], [0.0, s / 2.0], LAYER_MAIN));
            entities.push(line([-k, -s / 2.0], [0.0, -s / 2.0], LAYER_MAIN));
            // 弧：θ = 2·atan(4Δ/s)，r = (s/4)/sinθ，弧心 (−k+r, ±s/4)
            let th = 2.0 * (4.0 * delta / s).atan();
            let r3 = (s / 4.0) / th.sin();
            // 俯视图的弧从"轮廓起点 (x_out, ±s/2)"跨过 (±e/4) 一直画到中棱线端 (x_out, 0)，
            // 即张角对称于 180°（用户图：159.74°→200.26°）
            entities.push(arc([-k + r3, s / 4.0], r3, 180.0 - th.to_degrees(), 180.0 + th.to_degrees(), LAYER_MAIN));
            entities.push(arc([-k + r3, -s / 4.0], r3, 180.0 - th.to_degrees(), 180.0 + th.to_degrees(), LAYER_MAIN));
            // 头内部中棱线
            entities.push(line([x_out, 0.0], [0.0, 0.0], LAYER_MAIN));
            entities.push(line([0.0, s / 2.0], [0.0, -s / 2.0], LAYER_MAIN));
        }
        BoltView::End => {
            // 六边形：上下顶点 ±e/2，左右对边 ±s/2（顶点在 (±s/2, ±e/4)）
            let v = [
                [0.0, e / 2.0],
                [s / 2.0, e / 4.0],
                [s / 2.0, -e / 4.0],
                [0.0, -e / 2.0],
                [-s / 2.0, -e / 4.0],
                [-s / 2.0, e / 4.0],
            ];
            for i in 0..6 {
                entities.push(line(v[i], v[(i + 1) % 6], LAYER_MAIN));
            }
            // 端面倒角圆（直径 = 对边 s）
            entities.push(circle([0.0, 0.0], s / 2.0, LAYER_MAIN));
            // 十字中心线：半长 = s/2 + 3（实测 5780 M5 = 7.0、5783 M10 = 11.0）
            let o = s / 2.0 + 3.0;
            entities.push(line([-o, 0.0], [o, 0.0], LAYER_CENTER));
            entities.push(line([0.0, o], [0.0, -o], LAYER_CENTER));
            let meta = PartMeta {
                code: "GB/T 5780-2016".to_string(),
                name: "六角头螺栓 C级".to_string(),
                spec: spec_text_bolt(d, l),
                material: String::new(),
                weight: format!("≈{}", format!("{:.3}", bolt_c_weight_kg(row, l))),
            };
            return Ok(GenPart { entities, meta, bbox: [-s / 2.0, -e / 2.0, s / 2.0, e / 2.0] });
        }
    }

    // 杆 + 螺纹（主/俯视图共用）
    let b = thread_length_c(row, l);
    let c = 0.075 * d; // 杆端倒角
    entities.push(line([0.0, d / 2.0], [l - c, d / 2.0], LAYER_MAIN));
    entities.push(line([0.0, -d / 2.0], [l - c, -d / 2.0], LAYER_MAIN));
    entities.push(line([l - c, d / 2.0], [l, dm / 2.0], LAYER_MAIN));
    entities.push(line([l - c, -d / 2.0], [l, -dm / 2.0], LAYER_MAIN));
    entities.push(line([l, dm / 2.0], [l, -dm / 2.0], LAYER_MAIN));
    // 杆端倒角起点竖线 + 螺纹起点竖线（用户规范图里两条都在，跨满杆径）
    entities.push(line([l - c, d / 2.0], [l - c, -d / 2.0], LAYER_MAIN));
    entities.push(line([l - b, d / 2.0], [l - b, -d / 2.0], LAYER_MAIN));
    // 牙底细实线 + 收尾斜线
    entities.push(line([l - b, dm / 2.0], [l, dm / 2.0], LAYER_THIN));
    entities.push(line([l - b, -dm / 2.0], [l, -dm / 2.0], LAYER_THIN));
    entities.push(line([l - b - row.runout, d / 2.0], [l - b, dm / 2.0], LAYER_THIN));
    entities.push(line([l - b - row.runout, -d / 2.0], [l - b, -dm / 2.0], LAYER_THIN));
    // 中心线
    entities.push(line([-k - over, 0.0], [l + over, 0.0], LAYER_CENTER));

    let half_h = if view == BoltView::Main { e / 2.0 } else { s / 2.0 };
    Ok(GenPart {
        entities,
        meta: PartMeta {
            code: "GB/T 5780-2016".to_string(),
            name: "六角头螺栓 C级".to_string(),
            spec: spec_text_bolt(d, l),
            material: String::new(),
            weight: format!("≈{}", format!("{:.3}", bolt_c_weight_kg(row, l))),
        },
        bbox: [-k, -half_h, l, half_h],
    })
}

/// C 级螺栓单件重量估算（钢 7.85 g/cm³）。
pub fn bolt_c_weight_kg(row: &BoltCRow, l: f64) -> f64 {
    let head = COS30 * row.s * row.s * row.k;
    let shank = std::f64::consts::PI / 4.0 * row.d * row.d * (l - row.k).max(0.0);
    (head + shank) * 7.85e-3 / 1000.0
}

// ── 选择器页接口（目录 JSON / 预览 SVG / 生成派发）─────────────────────────

/// 参数化零件目录（选择器页用）：文件树 + 各族可用规格。
///
/// 树形路径与用户约定一致：`零件库/螺栓/六角螺栓/六角头螺栓 C级 GB/T 5780-2016`。
/// 未实现的常用族也列在树里（`implemented:false`），便于按图索骥。
pub fn catalog_json() -> String {
    // ── 已实现族的规格表
    let mut sizes = Vec::new();
    for row in hex_bolt_c_table().rows.iter() {
        sizes.push(serde_json::json!({
            "d": row.d,
            "label": format!("M{}", trim_num(row.d)),
            "pitch": row.pitch,
            "l_min": row.l_min,
            "l_max": row.l_max,
            "lengths": row.lengths,
        }));
    }
    let views = serde_json::json!([
        { "id": "main", "name": "主视图" },
        { "id": "top",  "name": "俯视图" },
        { "id": "end",  "name": "左视图" },
    ]);
    // ── 文件树（path 的每一层；叶子 = 规格表）
    let mut fam_map = serde_json::Map::new();
    fam_map.insert(
        "hex_bolt_c".to_string(),
        serde_json::json!({
            "id": "hex_bolt_c",
            "name": "六角头螺栓 C级",
            "code": "GB/T 5780-2016",
            "iso": "ISO 4016:2011",
            "implemented": true,
            "views": views,
            "sizes": sizes,
            "len_label": "长度 l",
            "base_hint": "基点 = 头部支承面 × 轴线",
        }),
    );
    for (k, v) in crate::partgen_more::families_json() {
        fam_map.insert(k, v);
    }
    let families = serde_json::Value::Object(fam_map);
    let tree = serde_json::json!([
        { "name": "零件库", "children": [
            { "name": "螺栓", "children": [
                { "name": "六角螺栓", "children": [
                    { "name": "六角头螺栓 C级 GB/T 5780-2016", "family": "hex_bolt_c", "implemented": true },
                    { "name": "六角头螺栓 A/B级 GB/T 5782-2016", "implemented": false },
                    { "name": "六角头螺栓 全螺纹 GB/T 5783-2016", "family": "hex_bolt_b_full", "implemented": true },
                    { "name": "六角头头部带孔螺栓 GB/T 32.1-2020", "family": "hex_bolt_hole_a", "implemented": true }
                ]},
                { "name": "内六角", "children": [
                    { "name": "内六角圆柱头螺钉 GB/T 70.1-2008", "family": "socket_head", "implemented": true }
                ]}
            ]},
            { "name": "螺母", "children": [
                { "name": "六角螺母", "children": [
                    { "name": "1型六角螺母 GB/T 6170-2015", "implemented": false },
                    { "name": "六角螺母 C级 GB/T 41-2016", "family": "nut_c41", "implemented": true }
                ]},
                { "name": "六角薄螺母", "children": [
                    { "name": "六角薄螺母 GB/T 6172.1-2016", "family": "nut_61721", "implemented": true }
                ]}
            ]},
            { "name": "垫圈", "children": [
                { "name": "平垫圈", "children": [
                    { "name": "平垫圈 A级 GB/T 97.1-2002", "family": "washer_971", "implemented": true }
                ]},
                { "name": "弹性垫圈", "children": [
                    { "name": "标准型弹簧垫圈 GB/T 93-2025", "family": "washer_93", "implemented": true }
                ]}
            ]},
            { "name": "挡圈", "children": [
                { "name": "孔用弹性挡圈 GB/T 893-2017", "implemented": false },
                { "name": "轴用弹性挡圈 GB/T 894-2017", "implemented": false }
            ]},
            { "name": "销", "children": [
                { "name": "圆柱销", "children": [
                    { "name": "圆柱销 A型 GB/T 119.1-2000", "family": "pin_1191", "implemented": true },
                    { "name": "内螺纹圆柱销 GB/T 120.1-2000", "family": "pin_1201", "implemented": true }
                ]}
            ]}
        ]}
    ]);
    serde_json::json!({ "tree": tree, "families": families }).to_string()
}

/// 按族/规格/视图生成零件（预览与插入共用同一入口）。
pub fn generate(family: &str, d: f64, l: f64, view: &str) -> Result<GenPart, String> {
    if let Some(r) = crate::partgen_more::generate(family, d, l, view) {
        return r;
    }
    match (family, view) {
        ("hex_bolt_c", "main") => hex_bolt_c(d, l, BoltView::Main),
        ("hex_bolt_c", "top") => hex_bolt_c(d, l, BoltView::Top),
        ("hex_bolt_c", "end") => hex_bolt_c(d, l, BoltView::End),
        ("hex_bolt_c", other) => Err(format!("hex_bolt_c 没有视图 {other}")),
        (other, _) => Err(format!("零件族 {other} 尚未实现")),
    }
}

/// URL 查询串（`family=hex_bolt_c&d=5&l=25&view=main`）→ 预览 SVG。
pub fn preview_svg(query: &str) -> Result<String, String> {
    let get = |k: &str| {
        query
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(a, _)| *a == k)
            .map(|(_, v)| v.to_string())
    };
    let family = get("family").unwrap_or_else(|| "hex_bolt_c".to_string());
    let d: f64 = get("d").ok_or("缺少参数 d")?.parse().map_err(|_| "d 不是数字".to_string())?;
    let l: f64 = get("l").ok_or("缺少参数 l")?.parse().map_err(|_| "l 不是数字".to_string())?;
    let view = get("view").unwrap_or_else(|| "main".to_string());
    let part = generate(&family, d, l, &view)?;
    let title = format!("{} {} {}", part.meta.code, part.meta.name, part.meta.spec);
    Ok(to_svg(&part, &title, 520.0, 260.0))
}

// ── SVG 预览（选择器页 / 人工核对共用）────────────────────────────────────
//
// 按图层着色（1轮廓实线层=黑、2细线层=青、3中心线层=红点划线），自动适配包围盒。

/// 单个视图 → 独立 SVG（宽高像素由 `px_w`/`px_h` 控制）。
pub fn to_svg(part: &GenPart, title: &str, px_w: f64, px_h: f64) -> String {
    let [x0, y0, x1, y1] = part.bbox;
    let (w, h) = ((x1 - x0).max(1e-6), (y1 - y0).max(1e-6));
    let pad = 0.08 * w.max(h);
    let k = ((px_w - 2.0 * pad) / w).min((px_h - 2.0 * pad) / h);
    let tx = |x: f64| pad + (x - x0) * k + (px_w - 2.0 * pad - w * k) / 2.0;
    let ty = |y: f64| px_h - pad - (y - y0) * k - (px_h - 2.0 * pad - h * k) / 2.0;
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{px_w}\" height=\"{px_h}\" viewBox=\"0 0 {px_w} {px_h}\">\
         <rect width=\"{px_w}\" height=\"{px_h}\" fill=\"#fff\"/>\
         <text x=\"8\" y=\"18\" font-family=\"sans-serif\" font-size=\"13\" fill=\"#333\">{}</text>",
        title.replace('&', "&amp;").replace('<', "&lt;")
    );
    for e in &part.entities {
        let layer = e.common().layer.as_str();
        let (color, dash) = match layer {
            LAYER_THIN => ("#00b0c0", ""),
            LAYER_CENTER => ("#d02020", "stroke-dasharray=\"10 3 3 3\""),
            // 4虚线层（不可见轮廓）
            "4虚线层" => ("#1040c0", "stroke-dasharray=\"9 4\""),
            _ => ("#111111", ""),
        };
        match e {
            EntityType::Line(l) => out.push_str(&format!(
                "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{color}\" stroke-width=\"1.3\" {dash}/>",
                tx(l.start.x), ty(l.start.y), tx(l.end.x), ty(l.end.y)
            )),
            EntityType::Circle(c) => out.push_str(&format!(
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"1.3\" {dash}/>",
                tx(c.center.x), ty(c.center.y), c.radius * k
            )),
            EntityType::Arc(a) => {
                // DXF/acadrust 的弧一律 **start→end 逆时针（CCW）**：跨度 = (end − start) mod 360。
                // 屏幕坐标 y 向下 → 同一段弧在屏幕上变成顺时针（sweep-flag = 0）。
                let span = {
                    let d = a.end_angle - a.start_angle;
                    let d = d.rem_euclid(std::f64::consts::TAU);
                    if d == 0.0 {
                        std::f64::consts::TAU
                    } else {
                        d
                    }
                };
                let (a0, a1) = (-a.start_angle.to_degrees(), -(a.start_angle + span).to_degrees());
                let large = if span.to_degrees() > 180.0 { 1 } else { 0 };
                let dir = 0;
                let (sx, sy) = (
                    tx(a.center.x) + a.radius * k * a0.to_radians().cos(),
                    ty(a.center.y) + a.radius * k * a0.to_radians().sin(),
                );
                let (ex, ey) = (
                    tx(a.center.x) + a.radius * k * a1.to_radians().cos(),
                    ty(a.center.y) + a.radius * k * a1.to_radians().sin(),
                );
                out.push_str(&format!(
                    "<path d=\"M {sx:.2} {sy:.2} A {:.2} {:.2} 0 {large} {dir} {ex:.2} {ey:.2}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"1.3\" {dash}/>",
                    a.radius * k, a.radius * k
                ));
            }
            EntityType::Hatch(h) => {
                // 预览：把边界画出来 + 半透明填充（图案线由 CAD 按 ANSI31 生成）
                // 边界边支持 Line 与带 bulge 的 Polyline（120.1 局部剖的波浪线）；弧/样条边按端点连线近似。
                for path in &h.paths {
                    let mut pts: Vec<(f64, f64)> = Vec::new();
                    for e in &path.edges {
                        use ocs_plugin_api::host::acadrust::entities::hatch::BoundaryEdge as BE;
                        match e {
                            BE::Line(l) => {
                                pts.push((l.start.x, l.start.y));
                                pts.push((l.end.x, l.end.y));
                            }
                            BE::Polyline(pe) => {
                                for v in &pe.vertices {
                                    pts.push((v.x, v.y));
                                }
                            }
                            BE::CircularArc(a) => {
                                let c = a.center;
                                for step in 0..=8 {
                                    let t = step as f64 / 8.0;
                                    let ang = a.start_angle + (a.end_angle - a.start_angle) * t;
                                    pts.push((c.x + a.radius * ang.cos(), c.y + a.radius * ang.sin()));
                                }
                            }
                            _ => {}
                        }
                    }
                    if pts.len() >= 3 {
                        let d: Vec<String> = pts
                            .iter()
                            .enumerate()
                            .map(|(i, (x, y))| {
                                format!("{} {:.2} {:.2}", if i == 0 { "M" } else { "L" }, tx(*x), ty(*y))
                            })
                            .collect();
                        out.push_str(&format!(
                            "<path d=\"{} Z\" fill=\"#f0c040\" fill-opacity=\"0.35\" stroke=\"#c09000\" stroke-width=\"1\"/>",
                            d.join(" ")
                        ));
                    }
                }
            }
            EntityType::LwPolyline(pl) => {
                let d: Vec<String> = pl
                    .vertices
                    .iter()
                    .enumerate()
                    .map(|(i, v)| format!("{} {:.2} {:.2}", if i == 0 { "M" } else { "L" }, tx(v.location.x), ty(v.location.y)))
                    .collect();
                out.push_str(&format!(
                    "<path d=\"{}{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"1.3\" {dash}/>",
                    d.join(" "),
                    if pl.is_closed { " Z" } else { "" }
                ));
            }
            _ => {}
        }
    }
    out.push_str("</svg>");
    out
}

#[cfg(test)]
mod svg_dump {
    /// 手工核对用：把 C 级螺栓三视图写成 SVG（`cargo test -p ocs_ocsm dump_bolt_svg -- --ignored --nocapture`）。
    #[test]
    #[ignore]
    fn dump_bolt_svg() {
        let dir = std::path::Path::new("/tmp/partgen");
        std::fs::create_dir_all(dir).unwrap();
        let cases: &[(f64, f64, &str, super::BoltView)] = &[
            (5.0, 25.0, "M5x25-主视图", super::BoltView::Main),
            (5.0, 25.0, "M5x25-俯视图", super::BoltView::Top),
            (5.0, 25.0, "M5x25-左视图", super::BoltView::End),
            (10.0, 45.0, "M10x45-主视图", super::BoltView::Main),
            (16.0, 65.0, "M16x65-主视图", super::BoltView::Main),
            (24.0, 100.0, "M24x100-主视图", super::BoltView::Main),
            (16.0, 65.0, "M16x65-左视图", super::BoltView::End),
        ];
        for (d, l, name, v) in cases {
            let p = super::hex_bolt_c(*d, *l, *v).unwrap();
            let svg = super::to_svg(&p, &format!("{} {}", p.meta.code, name), 460.0, 300.0);
            let f = dir.join(format!("boltC-{name}.svg"));
            std::fs::write(&f, svg).unwrap();
            println!("写出 {}", f.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 与用户提供的规范图逐条对齐（`六角头螺栓 C级 GBT5780-2016_主视图.dxf`，M5x25）。
    #[test]
    fn hex_bolt_c_main_view_matches_user_dxf() {
        let p = hex_bolt_c(5.0, 25.0, BoltView::Main).expect("M5x25 主视图");
        let ls = lines(&p);
        let near = |a: f64, b: f64| (a - b).abs() < 1e-3;
        let has_line = |x1: f64, y1: f64, x2: f64, y2: f64| {
            ls.iter().any(|(a, b, _)| {
                (near(a[0], x1) && near(a[1], y1) && near(b[0], x2) && near(b[1], y2))
                    || (near(a[0], x2) && near(a[1], y2) && near(b[0], x1) && near(b[1], y1))
            })
        };
        // 头：端面 / 轮廓 / 斜线 / 棱线 / 交界
        assert!(has_line(-3.5, 4.0, -3.5, -4.0), "端面 x=-k，跨对边 s");
        assert!(has_line(-3.143, 4.619, 0.0, 4.619), "上轮廓到 x=0（e/2 高）");
        assert!(has_line(-3.143, -4.619, 0.0, -4.619), "下轮廓");
        assert!(has_line(-3.5, 4.0, -3.143, 4.619), "倒角斜线（上）");
        assert!(has_line(-3.5, -4.0, -3.143, -4.619), "倒角斜线（下）");
        assert!(has_line(-3.143, 2.309, 0.0, 2.309), "头内棱线 +e/4");
        assert!(has_line(-3.143, -2.309, 0.0, -2.309), "头内棱线 -e/4");
        assert!(has_line(0.0, 4.619, 0.0, -4.619), "头杆交界 x=0");
        // 杆 / 螺纹 / 中心线
        assert!(has_line(0.0, 2.5, 24.625, 2.5), "杆轮廓（上）");
        assert!(has_line(24.625, 2.5, 25.0, 2.125), "杆端倒角");
        assert!(has_line(25.0, 2.125, 25.0, -2.125), "杆端面");
        assert!(has_line(9.0, 2.125, 25.0, 2.125), "牙底细实线（l−b → l）");
        assert!(has_line(9.0, 2.5, 9.0, -2.5), "螺纹起点竖线（l−b 处满杆径）");
        assert!(has_line(24.625, 2.5, 24.625, -2.5), "杆端倒角起点竖线");
        assert!(has_line(5.0, 2.5, 9.0, 2.125), "螺纹收尾斜线（runout=4）");
        assert!(has_line(-6.5, 0.0, 28.0, 0.0), "中心线出头 0.375s");
        // 圆弧：角弧 / 端面倒角大弧
        let arcs: Vec<(f64, f64, f64, f64, f64, String)> = p
            .entities
            .iter()
            .filter_map(|e| match e {
                EntityType::Arc(a) => Some((
                    a.center.x,
                    a.center.y,
                    a.radius,
                    a.start_angle.to_degrees(),
                    a.end_angle.to_degrees(),
                    a.common.layer.clone(),
                )),
                _ => None,
            })
            .collect();
        // 角度按 0.05° 容差比较（DXF 里角度只存两位小数）
        let near_ang = |a: f64, b: f64| (a - b).abs() < 0.05;
        let found = |cx: f64, cy: f64, r: f64, a0: f64, a1: f64| {
            arcs.iter().any(|(x, y, rr, s0, s1, _)| {
                near(*x, cx) && near(*y, cy) && near(*rr, r) && near_ang(*s0, a0) && near_ang(*s1, a1)
            })
        };
        // 上角弧 = 下角弧的镜像（180° → 180°+34.38°），终点落在棱线/大弧交点
        assert!(found(-1.455, 3.464, 2.045, 180.0, 214.38), "角弧（上，镜像）");
        assert!(found(-1.455, -3.464, 2.045, 145.62, 180.0), "角弧（下）");
        assert!(found(4.143, 0.0, 7.643, 162.41, 197.59), "端面倒角大弧");
        // 不含任何尺寸标注
        assert!(!p.entities.iter().any(|e| matches!(e, EntityType::Dimension(_))));
        assert_eq!(p.meta.code, "GB/T 5780-2016");
        assert_eq!(p.meta.spec, "M5x25");
    }

    /// 俯视图 / 左视图同样与用户 DXF 对齐。
    #[test]
    fn hex_bolt_c_top_and_end_view_match_user_dxf() {
        let top = hex_bolt_c(5.0, 25.0, BoltView::Top).expect("俯视图");
        let ls = lines(&top);
        let near = |a: f64, b: f64| (a - b).abs() < 1e-3;
        let has = |x1: f64, y1: f64, x2: f64, y2: f64| {
            ls.iter().any(|(a, b, _)| {
                (near(a[0], x1) && near(a[1], y1) && near(b[0], x2) && near(b[1], y2))
                    || (near(a[0], x2) && near(a[1], y2) && near(b[0], x1) && near(b[1], y1))
            })
        };
        assert!(has(-3.5, 4.0, -3.5, -4.0), "俯视图端面跨对边");
        assert!(has(-3.5, 4.0, 0.0, 4.0), "俯视图轮廓从端面 x=−k 起（对边 s）");
        assert!(has(-3.143, 0.0, 0.0, 0.0), "俯视图头内中棱线");
        let arcs: Vec<(f64, f64, f64, f64, f64)> = top
            .entities
            .iter()
            .filter_map(|e| match e {
                EntityType::Arc(a) => Some((
                    a.center.x,
                    a.center.y,
                    a.radius,
                    a.start_angle.to_degrees(),
                    a.end_angle.to_degrees(),
                )),
                _ => None,
            })
            .collect();
        let near_ang = |a: f64, b: f64| (a - b).abs() < 0.05;
        let found = |cx: f64, cy: f64, r: f64, a0: f64, a1: f64| {
            arcs.iter().any(|(x, y, rr, s0, s1)| {
                near(*x, cx) && near(*y, cy) && near(*rr, r) && near_ang(*s0, a0) && near_ang(*s1, a1)
            })
        };
        assert!(found(2.277, 2.0, 5.777, 159.74, 200.26), "俯视图倒角弧（上）");
        assert!(found(2.277, -2.0, 5.777, 159.74, 200.26), "俯视图倒角弧（下）");

        let end = hex_bolt_c(5.0, 25.0, BoltView::End).expect("左视图");
        let els = lines(&end);
        let has_e = |x1: f64, y1: f64, x2: f64, y2: f64| {
            els.iter().any(|(a, b, _)| {
                (near(a[0], x1) && near(a[1], y1) && near(b[0], x2) && near(b[1], y2))
                    || (near(a[0], x2) && near(a[1], y2) && near(b[0], x1) && near(b[1], y1))
            })
        };
        assert!(has_e(0.0, 4.619, -4.0, 2.309), "六边形边（上-左）");
        assert!(has_e(4.0, -2.309, 4.0, 2.309), "六边形右侧对边");
        assert!(has_e(-7.0, 0.0, 7.0, 0.0), "水平中心线 ±0.875s");
        assert!(has_e(0.0, 7.0, 0.0, -7.0), "竖直中心线");
        let circle = end.entities.iter().find_map(|e| match e {
            EntityType::Circle(c) => Some((c.center.x, c.center.y, c.radius, c.common.layer.clone())),
            _ => None,
        });
        assert_eq!(circle, Some((0.0, 0.0, 4.0, LAYER_MAIN.to_string())), "端面倒角圆 r=s/2");
    }

    /// 画法关键比例（跨直径自洽）：Δ=(e−s)/2·tan30°。
    #[test]
    fn chamfer_run_rule_matches_samples() {
        assert!((chamfer_run(8.0, across_corners(8.0)) - 0.3573).abs() < 1e-3, "M5");
        assert!((chamfer_run(16.0, across_corners(16.0)) - 0.7146).abs() < 1e-3, "M10");
        assert!((chamfer_run(24.0, across_corners(24.0)) - 1.0719).abs() < 1e-3, "M16");
    }

    /// 长度系列与长度校验。
    #[test]
    fn hex_bolt_c_lengths_and_validation() {
        let ls = hex_bolt_c_lengths(10.0);
        assert!(ls.contains(&40.0) && ls.contains(&100.0), "M10 长度系列");
        assert_eq!(ls.first().copied(), Some(40.0));
        assert!(hex_bolt_c(10.0, 30.0, BoltView::Main).is_err(), "低于 l_min");
        assert!(hex_bolt_c(10.0, 200.0, BoltView::Main).is_err(), "超过 l_max");
        assert!(hex_bolt_c(7.0, 40.0, BoltView::Main).is_err(), "无此直径");
        // 螺纹长度规则
        let row = hex_bolt_c_row(10.0).unwrap();
        assert_eq!(thread_length_c(row, 100.0), 26.0);
        assert_eq!(thread_length_c(row, 150.0), 32.0);
        assert_eq!(thread_length_c(row, 220.0), 45.0);
    }

    /// 所有生成的图元都必须是 ByLayer + 三个 OCSM 图层之一（不含尺寸标注）。
    #[test]
    fn hex_bolt_c_output_is_clean() {
        for view in [BoltView::Main, BoltView::Top, BoltView::End] {
            let p = hex_bolt_c(24.0, 100.0, view).unwrap();
            assert!(!p.entities.is_empty());
            for e in &p.entities {
                let c = e.common();
                assert_eq!(c.color, Color::ByLayer);
                assert_eq!(c.linetype, "ByLayer");
                assert_eq!(c.line_weight, LineWeight::ByLayer);
                assert!([LAYER_MAIN, LAYER_THIN, LAYER_CENTER].contains(&c.layer.as_str()));
                assert!(!matches!(e, EntityType::Dimension(_)));
            }
        }
    }

    use ocs_plugin_api::host::acadrust::entities::EntityType;

    fn layer_of(e: &EntityType) -> &str {
        &e.common().layer
    }

    fn lines(part: &GenPart) -> Vec<([f64; 2], [f64; 2], String)> {
        part.entities
            .iter()
            .filter_map(|e| match e {
                EntityType::Line(l) => Some((
                    [l.start.x, l.start.y],
                    [l.end.x, l.end.y],
                    l.common.layer.clone(),
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn hex_bolt_m10x100_matches_gb_dims() {
        let p = hex_bolt(10.0, 100.0, false).expect("生成 M10x100");
        // 头：对边 16、头高 6.4、对角 16/cos30 = 18.475
        // 对边视图：外形宽 = 对边 s = 16；对角宽 e = 18.475 只在"对角线视图"里出现
        assert!((p.bbox[0] + 6.4).abs() < 1e-9, "头部端面在 x=-k");
        assert!(p.bbox[1] == -8.0 && p.bbox[3] == 8.0, "头部外形宽 = 对边 16");
        assert!((across_corners(16.0) - 18.475).abs() < 1e-3);
        let ls = lines(&p);
        // 端面竖线 x=-6.4，长 16（对边）
        let face = ls
            .iter()
            .find(|(a, b, _)| (a[0] + 6.4).abs() < 1e-9 && (b[0] + 6.4).abs() < 1e-9)
            .expect("端面线");
        assert!((face.0[1].abs() - 8.0).abs() < 1e-9 && (face.1[1].abs() - 8.0).abs() < 1e-9);
        // 螺纹：M10 螺距 1.5 → 小径 8.376；b1 = 26 → 牙底线 74…100
        let d1 = minor_dia(10.0, 1.5);
        // 细实线：2 条牙底线（74→100，d1/2）+ 2 条螺纹收尾斜线
        let thread: Vec<_> = ls
            .iter()
            .filter(|(a, b, lay)| {
                lay == LAYER_THIN
                    && (a[0] - 74.0).abs() < 1e-9
                    && (b[0] - 100.0).abs() < 1e-9
            })
            .collect();
        assert_eq!(thread.len(), 2, "牙底线两条");
        for (a, _, _) in &thread {
            assert!((a[1].abs() - d1 / 2.0).abs() < 1e-9, "牙底细实线在 d1/2");
        }
        let runouts = ls
            .iter()
            .filter(|(_, _, lay)| lay == LAYER_THIN)
            .count();
        assert_eq!(runouts, 4, "2 条牙底线 + 2 条收尾斜线");
        // 螺纹终止线（粗实线）在 x=74
        assert!(ls.iter().any(|(a, b, lay)| lay == LAYER_MAIN
            && (a[0] - 74.0).abs() < 1e-9
            && (b[0] - 74.0).abs() < 1e-9));
        // 中心线在 y=0
        let center: Vec<_> = ls.iter().filter(|(_, _, lay)| lay == LAYER_CENTER).collect();
        assert_eq!(center.len(), 1);
        assert_eq!(center[0].0[1], 0.0);
        assert_eq!(p.meta.code, "GB/T 5782-2016");
        assert_eq!(p.meta.spec, "M10x100");
        assert!(p.meta.weight.starts_with('≈'));
    }

    #[test]
    fn hex_bolt_full_thread_runs_whole_length() {
        let p = hex_bolt(10.0, 100.0, true).expect("全螺纹");
        let ls = lines(&p);
        let thread: Vec<_> = ls.iter().filter(|(_, _, lay)| lay == LAYER_THIN).collect();
        assert_eq!(thread.len(), 2, "全螺纹只有 2 条牙底线（无收尾斜线）");
        for (a, b, _) in &thread {
            assert!((a[0] - 0.0).abs() < 1e-9 && (b[0] - 100.0).abs() < 1e-9, "牙底贯通");
        }
        assert_eq!(p.meta.code, "GB/T 5783-2016");
        // 全螺纹没有螺纹终止线（不应有 x=74 处的竖线；杆端面在 x=100 不算）
        let stops: Vec<_> = ls
            .iter()
            .filter(|(a, b, lay)| {
                lay == LAYER_MAIN
                    && (a[0] - b[0]).abs() < 1e-9
                    && (a[0] - 74.0).abs() < 1e-9
            })
            .collect();
        assert!(stops.is_empty(), "全螺纹不应有螺纹终止线");
    }

    #[test]
    fn hex_nut_m10_matches_gb_dims() {
        let p = hex_nut(10.0).expect("生成 M10 螺母");
        // 对边 16、高 8.4；底面 y=0
        assert!((p.bbox[0] + 8.0).abs() < 1e-9 && (p.bbox[2] - 8.0).abs() < 1e-9);
        assert!((p.bbox[1] - 0.0).abs() < 1e-9 && (p.bbox[3] - 8.4).abs() < 1e-9);
        let ls = lines(&p);
        let d1 = minor_dia(10.0, 1.5);
        // 内螺纹：小径粗实线、大径细实线
        assert!(ls.iter().any(|(a, b, lay)| lay == LAYER_MAIN
            && (a[0] - d1 / 2.0).abs() < 1e-9
            && (b[0] - d1 / 2.0).abs() < 1e-9
            && (a[1] - 0.0).abs() < 1e-9
            && (b[1] - 8.4).abs() < 1e-9));
        assert!(ls.iter().any(|(a, b, lay)| lay == LAYER_THIN
            && (a[0] - 5.0).abs() < 1e-9
            && (b[0] - 5.0).abs() < 1e-9));
        assert_eq!(p.meta.code, "GB/T 6170-2015");
        assert_eq!(p.meta.spec, "M10");
    }

    #[test]
    fn generated_entities_are_all_bylayer_on_ocsm_layers() {
        for part in [
            hex_bolt(6.0, 30.0, false).unwrap(),
            hex_bolt(24.0, 80.0, true).unwrap(),
            hex_nut(20.0).unwrap(),
        ] {
            for e in &part.entities {
                let c = e.common();
                assert_eq!(c.color, Color::ByLayer, "颜色应随层");
                assert_eq!(c.linetype, "ByLayer");
                assert_eq!(c.line_weight, LineWeight::ByLayer);
                assert!(
                    [LAYER_MAIN, LAYER_THIN, LAYER_CENTER].contains(&c.layer.as_str()),
                    "图层只允许 1轮廓实线层/2细线层/3中心线层，实际 {}",
                    c.layer
                );
            }
        }
    }

    #[test]
    fn out_of_range_length_is_rejected() {
        assert!(hex_bolt(10.0, 10.0, false).is_err());
        assert!(hex_bolt(99.0, 100.0, false).is_err());
    }

    #[test]
    fn standard_length_series_covers_range() {
        let row = bolt_row(10.0).unwrap();
        let ls = standard_lengths(row);
        assert_eq!(*ls.first().unwrap(), row.l_min);
        assert_eq!(*ls.last().unwrap(), row.l_max);
        assert!(ls.contains(&40.0) && ls.contains(&100.0));
        assert!(ls.windows(2).all(|w| w[1] > w[0]));
    }

    #[test]
    fn thread_length_follows_iso_rule() {
        let row = bolt_row(10.0).unwrap();
        assert_eq!(thread_length(row, 100.0), 26.0);
        assert_eq!(thread_length(row, 150.0), 32.0);
        assert_eq!(thread_length(row, 220.0), 45.0);
    }
    /// 防回归：acadrust 的角度字段是**弧度**（DXF 读取时已 `.to_radians()`），
    /// 曾经把度数直接写进去 → 宿主渲染时弧线跑飞（用户实测）。
    #[test]
    fn arc_angles_are_stored_in_radians() {
        for view in [BoltView::Main, BoltView::Top, BoltView::End] {
            let p = hex_bolt_c(5.0, 25.0, view).unwrap();
            for e in &p.entities {
                if let EntityType::Arc(a) = e {
                    assert!(
                        a.start_angle.abs() <= std::f64::consts::TAU + 1e-9
                            && a.end_angle.abs() <= std::f64::consts::TAU + 1e-9,
                        "弧角度必须是弧度（start {} end {}）",
                        a.start_angle,
                        a.end_angle
                    );
                }
            }
        }
    }

    /// 目录 JSON 的**全局防呆**：每个上架族都必须有视图、有规格、且第一个规格能出图。
    ///
    /// 回归背景（2026-09-15）：`families_json` 曾把已下架族（`nut_6170`）的空视图数组
    /// 错给到 `nut_61721` / `nut_c41` → GUI 视图按钮为空、`pickFamily` 抛异常、
    /// 长度与预览全不加载（面板静默失效）。此测试是那一类错误的语料级护栏。
    #[test]
    fn catalog_all_implemented_families_usable() {
        let cat: serde_json::Value = serde_json::from_str(&catalog_json()).expect("目录 JSON");
        let fams = cat["families"].as_object().expect("families");
        assert!(fams.len() >= 8, "上架族数异常：{}", fams.len());
        for (id, f) in fams {
            if f["implemented"] == serde_json::json!(false) {
                continue;
            }
            let views: Vec<String> = f["views"]
                .as_array()
                .unwrap_or_else(|| panic!("{id} 缺 views"))
                .iter()
                .map(|v| v["id"].as_str().unwrap().to_string())
                .collect();
            assert!(!views.is_empty(), "{id} 上架却没有视图（GUI 会静默失效）");
            assert_eq!(
                views,
                crate::partgen_more::family_views(id),
                "{id} 的 views 与视图注册表不一致"
            );
            let sizes = f["sizes"].as_array().unwrap_or_else(|| panic!("{id} 缺 sizes"));
            assert!(!sizes.is_empty(), "{id} 没有规格");
            let d = sizes[0]["d"].as_f64().unwrap();
            let l = sizes[0]["lengths"][0].as_f64().unwrap();
            for v in &views {
                generate(id, d, l, v)
                    .unwrap_or_else(|e| panic!("{id} Ø{d}×{l} {v} 生成失败: {e}"));
            }
            // 视图名齐全（GUI 按钮文案）
            for v in f["views"].as_array().unwrap() {
                assert!(!v["name"].as_str().unwrap_or("").is_empty(), "{id} 视图缺 name");
            }
        }
        // 树上：螺母两族、销两族都必须挂上 family（否则 GUI 里点不到）
        let tree = cat["tree"].to_string();
        for needle in [
            "六角螺母 C级 GB/T 41-2016",
            "六角薄螺母 GB/T 6172.1-2016",
            "1型六角螺母 GB/T 6170-2015",
            "nut_c41",
            "nut_61721",
        ] {
            assert!(tree.contains(needle), "树里缺 {needle}");
        }
    }
}

#[cfg(test)]
pub(crate) mod acceptance_dump {
    //! 出"参数化验收图"：用**与插件插入完全相同的生成代码**建块 + INSERT 并写盘，
    //! 供在 OCS 里打开检查画法（这是发现"弧线单位"这类宿主渲染问题最有效的方式）。
    //!
    //! `cargo test -p ocs_ocsm dump_acceptance_drawing -- --ignored --nocapture`

    use super::*;
    use ocs_plugin_api::host::acadrust::entities::{Block, BlockEnd, Insert};
    use ocs_plugin_api::host::acadrust::io::dxf::DxfWriter;
    use ocs_plugin_api::host::acadrust::io::dwg::DwgWriter;
    use ocs_plugin_api::host::acadrust::tables::{BlockRecord, Layer, LineType, LineTypeElement};
    use ocs_plugin_api::host::acadrust::xdata::{ExtendedDataRecord, XDataValue};
    use ocs_plugin_api::host::acadrust::CadDocument;

    /// 建块（与宿主 `AddBlockRecord` 同构：BlockRecord → Block → 成员 → BlockEnd）。
    pub(crate) fn add_block(doc: &mut CadDocument, name: &str, entities: Vec<EntityType>) {
        let mut br = BlockRecord::new(name);
        br.handle = doc.allocate_handle();
        let mut block = Block::new(name, Vector3::new(0.0, 0.0, 0.0));
        block.common.handle = doc.allocate_handle();
        block.common.owner_handle = br.handle;
        br.block_entity_handle = block.common.handle;
        doc.add_entity(EntityType::Block(block)).expect("block");
        let br_handle = br.handle;
        doc.block_records.add(br).expect("block record");
        let mut members = Vec::new();
        for mut e in entities {
            e.common_mut().owner_handle = br_handle;
            members.push(doc.add_entity(e).expect("block member"));
        }
        let mut end = BlockEnd::new();
        end.common.handle = doc.allocate_handle();
        end.common.owner_handle = br_handle;
        let end_handle = end.common.handle;
        doc.add_entity(EntityType::BlockEnd(end)).expect("block end");
        if let Some(b) = doc.block_records.get_mut(name) {
            b.entity_handles = members;
            b.block_end_handle = end_handle;
        }
    }

    /// 图层 + 线型（与插件运行时 `ensure_layers/ensure_linetypes` 同源）。
    ///
    /// 注意：**必须给 LTYPE/LAYER 分配句柄**，否则 DxfWriter 会写出 `handle=0`，
    /// ezdxf 等严格读取器会报 `Invalid handle 0`（验收 DXF 打不开）。
    pub(crate) fn add_ocsm_layers(doc: &mut CadDocument) {
        for def in crate::linetype_defs() {
            let mut lt = LineType::new(&def.name);
            lt.handle = doc.allocate_handle();
            lt.description = def.description.clone();
            lt.elements = def
                .elements
                .iter()
                .map(|v| LineTypeElement { length: *v, complex: None })
                .collect();
            lt.pattern_length = lt.elements.iter().map(|e| e.length.abs()).sum();
            doc.line_types.add_or_replace(lt);
        }
        for def in crate::layer_defs() {
            let mut l = Layer::new(&def.name);
            l.handle = doc.allocate_handle();
            l.color = def.color;
            l.line_type = def.linetype.clone();
            l.line_weight = def.lineweight;
            doc.layers.add_or_replace(l);
        }
    }

    #[test]
    #[ignore]
    fn dump_acceptance_drawing() {
        use BoltView::*;
        // (直径, 长度, 视图, 插入点x, 插入点y)
        let cases: &[(f64, f64, BoltView, f64, f64)] = &[
            (5.0, 25.0, Main, 20.0, 200.0),
            (10.0, 45.0, Main, 80.0, 200.0),
            (16.0, 65.0, Main, 150.0, 200.0),
            (10.0, 45.0, Top, 250.0, 200.0),
            (5.0, 25.0, Top, 20.0, 140.0),
            (5.0, 25.0, End, 80.0, 140.0),
            (24.0, 100.0, Main, 140.0, 140.0),
            (8.0, 40.0, Main, 20.0, 80.0),
            (16.0, 65.0, End, 100.0, 80.0),
            (24.0, 100.0, End, 160.0, 80.0),
        ];

        let mut doc = CadDocument::new();
        add_ocsm_layers(&mut doc);
        let mut lines = Vec::new();
        for (d, l, view, x, y) in cases {
            let part = hex_bolt_c(*d, *l, *view).expect("生成");
            let vname = match view {
                Main => "main",
                Top => "top",
                End => "end",
            };
            let block = format!(
                "OCSM_HEX_BOLT_C_{}_{}",
                part.meta.spec.replace(['.', ' ', '/', 'x', 'X'], "_"),
                vname.to_ascii_uppercase()
            );
            add_block(&mut doc, &block, part.entities.clone());
            let mut ins = Insert::new(&block, Vector3::new(*x, *y, 0.0));
            ins.common.layer = LAYER_MAIN.to_string();
            ins.common.color = ocs_plugin_api::host::acadrust::types::Color::ByLayer;
            ins.common.linetype = "ByLayer".to_string();
            ins.common.line_weight = ocs_plugin_api::host::acadrust::types::LineWeight::ByLayer;
            let mut rec = ExtendedDataRecord::new("OCSM_PART");
            rec.values.push(XDataValue::String(
                serde_json::json!({
                    "code": part.meta.code, "name": part.meta.name, "spec": part.meta.spec,
                    "weight": part.meta.weight, "d": d, "l": l, "view": vname,
                })
                .to_string(),
            ));
            ins.common.extended_data.add_record(rec);
            doc.add_entity(EntityType::Insert(ins)).expect("insert");
            lines.push(format!(
                "{:>6} {:>4}  {:>4}  @({x:.0},{y:.0})  块 {block}",
                part.meta.spec,
                vname,
                part.entities.len()
            ));
        }

        let dir = std::path::Path::new("/home/ysdirector/桌面/OCSM/test");
        std::fs::create_dir_all(dir).unwrap();
        let dwg = dir.join("参数化验收-六角头螺栓C级.dwg");
        let dxf = dir.join("参数化验收-六角头螺栓C级.dxf");
        DwgWriter::write_to_file(&dwg, &doc).expect("写 DWG");
        DxfWriter::new(&doc).write_to_file(&dxf).expect("写 DXF");
        println!("已写出：\n  {}\n  {}", dwg.display(), dxf.display());
        println!("内容：{} 个零件（块 + INSERT + OCSM_PART 记录）", lines.len());
        for l in &lines {
            println!("  {l}");
        }
    }
}

#[cfg(test)]
mod json_dump {
    use super::*;
    use ocs_plugin_api::host::acadrust::EntityType;

    /// 把 M5x25 主/俯视图的图元导出成 JSON（供与用户规范 DXF 叠加对比）。
    #[test]
    #[ignore]
    fn dump_bolt_json() {
        let mut all = Vec::new();
        for (name, v) in [("main", super::BoltView::Main), ("top", super::BoltView::Top), ("end", super::BoltView::End)] {
            let p = super::hex_bolt_c(5.0, 25.0, v).unwrap();
            let (mut lines, mut arcs, mut circles) = (Vec::new(), Vec::new(), Vec::new());
            for e in &p.entities {
                match e {
                    EntityType::Line(l) => lines.push(vec![l.start.x, l.start.y, l.end.x, l.end.y]),
                    EntityType::Arc(a) => arcs.push(vec![
                        a.center.x,
                        a.center.y,
                        a.radius,
                        a.start_angle.to_degrees(),
                        a.end_angle.to_degrees(),
                    ]),
                    EntityType::Circle(c) => circles.push(vec![c.center.x, c.center.y, c.radius]),
                _ => {}
                    _ => {}
                }
            }
            all.push(serde_json::json!({
                "name": name, "lines": lines, "arcs": arcs, "circles": circles
            }));
        }
        let out = serde_json::json!({ "views": all }).to_string();
        let f = std::path::Path::new("/tmp/partgen");
        std::fs::create_dir_all(f).unwrap();
        std::fs::write(f.join("mine.json"), out).unwrap();
        println!("已写出 /tmp/partgen/mine.json");
    }
}
