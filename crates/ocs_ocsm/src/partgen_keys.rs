//! 平键参数化（GB/T 1096-2003 普通平键 A/B/C 型；GB/T 1097-2003 导向平键 A/B 型）。
//!
//! ## 素材与依据
//!
//! - 几何反解：`~/桌面/OCSM/review/平键_几何反解.md`（逐图元 WCS 坐标 + 尺寸实测 + OCS 镜像坑）。
//! - 数据表：`平键_表格_1096{A,B,C}.csv`（各 20 档 `O.,b,h,L`）、
//!   `平键_表格_1097{A,B}.csv`（各 14 档 `O.,b,h,C,h1,d0,d1,D,C1,L0,L,L1,L2,L3`）。
//! - 本节规则由**用户逐条拍板**（2026-09 平键任务），与反解报告冲突处一律以用户口径为准：
//!
//! ### 用户已定口径（写死在代码里，勿自行"改正"）
//!
//! 1. **1097 起键螺纹孔的小径 3/4 圈**：源图画在 `4虚线层`，本库改到 **`2细线层`（细实线）**；
//!    其余一律照源图（**不**按 GB/T 4459.1 大改）。⚠️
//! 2. **1097 主视图剖切**：源图的"右半剖 + 45° 断裂折线"是规范画法 → **照抄**：
//!    右半 x>0 全剖（材料打剖面线），左半为外形，左端孔用虚线；
//!    断裂边界 = x=-x_break 竖线 + 上下折线（见 [`BREAK_GAP`]/[`BREAK_RUN`]）。
//! 3. **剖面线间距全局 3.0 mm**（与齿轮/花键/轴生成器统一；**不**照模板的 0.794/3.175）。
//! 4. **1096 表第三列就是 `L`**（源图 PNG 表头"L/2提示"是 OCR 误读）；L 取值按
//!    [`KEY_1096_L_SERIES`] + `L < 10b` 校验，档位默认 L 取表内该档值（PNG 第三列）。
//! 5. **1096 倒角 `c = 0.2` 固定**（GB/T 1096 表 1 的 `s` 其实是"按 b 分档的范围"，
//!    见 [`KEY_1096_CHAMFER_RANGES`]，只作 notes 留档，不据此改画法）。
//! 6. **1097 中央 `d0` 孔是"起键螺纹孔"** —— 它是**键自身**用于起键（撬出）的孔，
//!    **与轴无关**；**不是**把键固定到轴上的孔（反解报告 §3.5 的"固定用"推断**已作废**）。
//!    两端 `d1` 通孔 + `D×h1` 沉孔照源图（孔位 ±L1/2、距端面 L3）。
//!
//! ### 数据口径备忘
//!
//! - **1096 A 型源图末 4 行 h 印作 32/20/22/25（源图自身误印，已像素级证实）**：
//!   CSV 照录不改，但**库内取标准值 20/22/25/28**；JSON 表里 `h` 用标准值、
//!   `h_printed` 保留印刷值备查（见 `tables/partsKey1096A.json` 的 `source`）。
//! - 1097 表内各档 `L=100、L1=60、L2=50、L3=20`（即 specimen 行）；C 取表内范围**下限**。
//! - **1096 的 L 区间已到位**（GB/T 1096-2003 表 1 + 用户素材补 6、8）：
//!   系列见 [`KEY_1096_L_SERIES`]，约束 `L < 10·b`；档位默认 L 必须 ∈ 系列且 < 10b，
//!   不满足时就近取合法值（[`key_1096_legal_default`]，当前 20 档全部合法）。
//! - **1097 的 L 系列已到位**（易紧通 `info_42177` 长度下拉实测，2026-09）：
//!   系列见 [`key_1097_l_allowed`]（数据源 = `partsKey1097{A,B}.json` 的 `l_series`）；
//!   沿用标准注③ `L < 10·b`（>400 时按 GB/T 321 的 R20 选取，本库未列）；
//!   `l` 省略/传 0 时用表内该档 L（100；b=8/10 因 100≥10b 就近合法化，见
//!   [`key_1097_legal_default`]/[`key_1097_default_corrections`]）。
//!
//! ## 参数模型（GUI/CLI/HTTP 统一）
//!
//! 族 id 五枚：`key_1096_a` / `key_1096_b` / `key_1096_c` / `key_1097_a` / `key_1097_b`。
//! **自变量 = 键宽 `b` + 型别（编在族 id 里）**；`h` 由数据表派生；`L` 可选。
//! 既有 GUI/CLI 的两参数模型固定为 `<d> <l>`，所以本族 **`d` 槽位承载 `b`**：
//! `OCSMPART key_1096_a 4 8` = b=4、L=8；`l` 传 0/省略 = 用表内该档默认 L。
//! 1097：`OCSMPART key_1097_a 8 100`。
//!
//! ## 视图与基点（照源图坐标，另见各族 `base_hint`）
//!
//! - **1096（三视图）**：主视图 `L×h`（原点 = 左端 × 底面；A 两端 45° 切角、
//!   C 左端切角、B 无；y=c 与 y=h-c 两条倒角棱线）；俯视图 `L×b`（原点 = 左端 × 对称轴；
//!   A 双圆头 R=b/2、B 矩形、C 左圆右方；内缩 c 的内轮廓；中心线两端各伸 3）；
//!   剖视图 `b×h` 八边形（四角切 c）+ ANSI31 剖面线（**无中心线**，照源图）。
//! - **1097（两视图，模板无剖视图 → 不画）**：主视图 = 右半剖 + 左半外形（见口径 2）；
//!   俯视图 A 跑道形 R=b/2 / B 矩形 + 全部孔圈；中心线伸出 3。
//! - **不生成任何尺寸标注**；只用 OCSM 五层。
//!
//! ## OCS 镜像坑（1097 特有）
//!
//! 源图左半图元大量用镜像（`extrusion=(0,0,-1)`，DXF 存 OCS 坐标）。本库按报告
//! §3 的 **WCS 坐标**生成完整对称件，绝不把"镜像对"当重复实体。

use crate::partgen::{GenPart, PartMeta};
use crate::partgen_kit::{
    arc, circle, hatch_ansi31_scaled, line, polyline, trim, views_json, ANSI31_SPACING_MM,
    LAYER_CENTER, LAYER_HIDDEN, LAYER_MAIN, LAYER_THIN,
};
use ocs_plugin_api::host::acadrust::entities::EntityType;

// ══════════════════════════════════════════════════════════════════════════
// 常量（用户口径与源图推导）
// ══════════════════════════════════════════════════════════════════════════

/// 1096 倒角：用户拍板固定 0.2（不按 b 分档）。
pub const C_1096: f64 = 0.2;

/// GB/T 1096 表 1 `s`（倒角）= **按 b 分档的范围**——只作 notes 留档；
/// 用户已定 1096 倒角固定 [`C_1096`]，**不要**据此改画法。
pub const KEY_1096_CHAMFER_RANGES: &[(f64, f64, &str)] = &[
    (2.0, 4.0, "0.16~0.25"),
    (5.0, 8.0, "0.25~0.40"),
    (10.0, 18.0, "0.40~0.60"),
    (20.0, 32.0, "0.60~0.80"),
    (36.0, 50.0, "1.00~1.20"),
    (56.0, 70.0, "1.60~2.00"),
    (80.0, 100.0, "2.50~3.00"),
];

/// GB/T 1096-2003 表 1 长度 L 标准系列。
///
/// `6、8` 依据用户素材补充（素材 PNG 第三列含 6、8，DXF specimen 就是 L=6；
/// 在线来源的 L 行自 10 起），其余照表 1。`L > 500` 应改按 GB/T 321 R20
/// 选取（本系列最大 400，暂不涉及）。
pub const KEY_1096_L_SERIES: &[f64] = &[
    6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 25.0, 28.0, 32.0, 36.0, 40.0, 45.0,
    50.0, 56.0, 63.0, 70.0, 80.0, 90.0, 100.0, 110.0, 125.0, 140.0, 160.0, 180.0, 200.0, 220.0,
    250.0, 280.0, 320.0, 360.0, 400.0,
];

/// 1097 主视图局部剖断裂线：距左端 d1 孔外缘的间隙（源图 specimen：
/// L1=60、D=6 → x_break=−(L1/2−D/2−1)=−26）。
const BREAK_GAP: f64 = 1.0;
/// 1097 断裂线上下折线沿 x 的伸出量（源图 specimen：−26 → −24）。
const BREAK_RUN: f64 = 2.0;

/// 螺纹小径简化系数：0.85 × d0/2（报告 §3 实测）。
const MINOR_THREAD_FACTOR: f64 = 0.85;

/// 全局剖面线间距 3.0 mm（用户口径 3）→ ANSI31 pattern_scale。
fn hatch_scale_3mm() -> f64 {
    3.0 / ANSI31_SPACING_MM
}

// ══════════════════════════════════════════════════════════════════════════
// 数据表
// ══════════════════════════════════════════════════════════════════════════

/// JSON 表外壳（比 `partgen_kit::Table` 多 `source`/`note`，供手册与测试引用）。
#[derive(Debug, Clone, serde::Deserialize)]
struct KeyTable<T> {
    #[allow(dead_code)]
    family: String,
    #[allow(dead_code)]
    code: String,
    #[allow(dead_code)]
    iso: String,
    /// 数据来源与口径批注（源图误印等）。
    #[serde(default)]
    #[allow(dead_code)]
    source: String,
    /// 表注（L 系列补充说明等）。
    #[serde(default)]
    #[allow(dead_code)]
    note: String,
    /// 长度 L 标准系列（1097 两表的 `l_series`；1096 用代码常量 [`KEY_1096_L_SERIES`]）。
    #[serde(default)]
    l_series: Vec<f64>,
    rows: Vec<T>,
}

impl<T: serde::de::DeserializeOwned> KeyTable<T> {
    fn parse(json: &str) -> Self {
        serde_json::from_str(json).expect("平键数据表 JSON 解析失败")
    }
}

/// GB/T 1096-2003 一行（`O. b h L`）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Key1096Row {
    /// 键宽 b（本族的规格自变量）。
    pub b: f64,
    /// 键高 h（A 型末 4 行已用标准值 20/22/25/28；印刷值见 `h_printed`）。
    pub h: f64,
    /// 源图第三列 = 该档默认 L（"L/2提示"是 OCR 误读）。
    pub l: f64,
    /// **仅 A 型**：源图印刷的 h（末 4 行为 32/20/22/25，疑源图误印）；B/C 型无。
    #[serde(default)]
    pub h_printed: Option<f64>,
}

/// GB/T 1097-2003 一行（`O. b h C h1 d0 d1 D C1 L0 L L1 L2 L3`）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Key1097Row {
    /// 键宽 b（规格自变量）。
    pub b: f64,
    /// 键高 h。
    pub h: f64,
    /// 倒角 C（表内给范围，库内取**下限**，照源图 specimen）。
    pub c: f64,
    /// 表内 C 范围原文（留档）。
    #[serde(default)]
    #[allow(dead_code)]
    pub c_range: String,
    /// 沉孔深 h1。
    pub h1: f64,
    /// 起键螺纹孔大径 d0。
    pub d0: f64,
    /// 两端通孔直径 d1。
    pub d1: f64,
    /// 沉孔直径 D。
    #[serde(rename = "D")]
    pub d_sink: f64,
    /// 螺纹孔口 120° 锪锥深度 C1。
    pub c1: f64,
    /// 起键螺纹孔深度 L0（自顶面向下量）。
    #[serde(rename = "L0")]
    pub l0: f64,
    /// 表内长度 L（当前 14 档均为 100）。
    pub l: f64,
    /// 两端 d1 孔中心距 L1。
    pub l1: f64,
    /// 键中心 → 端面 L2（= L/2）。
    pub l2: f64,
    /// d1 孔中心 → 端面 L3。
    pub l3: f64,
}

fn table_1096(ty: KeyType) -> &'static KeyTable<Key1096Row> {
    macro_rules! once {
        ($json:literal) => {{
            static T: std::sync::OnceLock<KeyTable<Key1096Row>> = std::sync::OnceLock::new();
            T.get_or_init(|| KeyTable::parse(include_str!($json)))
        }};
    }
    match ty {
        KeyType::A => once!("tables/partsKey1096A.json"),
        KeyType::B => once!("tables/partsKey1096B.json"),
        KeyType::C => once!("tables/partsKey1096C.json"),
    }
}

fn table_1097(ty: KeyType) -> &'static KeyTable<Key1097Row> {
    macro_rules! once {
        ($json:literal) => {{
            static T: std::sync::OnceLock<KeyTable<Key1097Row>> = std::sync::OnceLock::new();
            T.get_or_init(|| KeyTable::parse(include_str!($json)))
        }};
    }
    match ty {
        KeyType::A => once!("tables/partsKey1097A.json"),
        KeyType::B => once!("tables/partsKey1097B.json"),
        KeyType::C => unreachable!("GB/T 1097 没有 C 型"),
    }
}

fn row_1096(ty: KeyType, b: f64) -> Option<&'static Key1096Row> {
    table_1096(ty).rows.iter().find(|r| (r.b - b).abs() < 1e-9)
}

fn row_1097(ty: KeyType, b: f64) -> Option<&'static Key1097Row> {
    table_1097(ty).rows.iter().find(|r| (r.b - b).abs() < 1e-9)
}

// ══════════════════════════════════════════════════════════════════════════
// 族 / 型别 / 视图注册
// ══════════════════════════════════════════════════════════════════════════

/// 平键型别（1096 三型；1097 只有 A/B）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    /// 圆头（双圆头）
    A,
    /// 方头（双平头）
    B,
    /// 单圆头（左圆右方）
    C,
}

/// 平键标准族。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyStandard {
    /// GB/T 1096-2003 普通平键（三视图）
    Gb1096,
    /// GB/T 1097-2003 导向平键（两视图，主视图为右半剖 + 左半外形）
    Gb1097,
}

/// 族 id → (标准, 型别)。
pub fn family_parts(family: &str) -> Option<(KeyStandard, KeyType)> {
    match family {
        "key_1096_a" => Some((KeyStandard::Gb1096, KeyType::A)),
        "key_1096_b" => Some((KeyStandard::Gb1096, KeyType::B)),
        "key_1096_c" => Some((KeyStandard::Gb1096, KeyType::C)),
        "key_1097_a" => Some((KeyStandard::Gb1097, KeyType::A)),
        "key_1097_b" => Some((KeyStandard::Gb1097, KeyType::B)),
        _ => None,
    }
}

/// 型别的中文后缀（族名/规格文本用）。
#[allow(dead_code)]
fn type_label(ty: KeyType) -> &'static str {
    match ty {
        KeyType::A => "A型（双圆头）",
        KeyType::B => "B型（双平头）",
        KeyType::C => "C型（单圆头）",
    }
}

/// 本模块声明的视图（`partgen_more::family_views` 会先问这里）。
pub fn family_views(family: &str) -> Vec<&'static str> {
    match family_parts(family) {
        // 1096 三视图（源图主/俯/剖三个 DXF）
        Some((KeyStandard::Gb1096, _)) => vec!["main", "top", "section"],
        // 1097 只有主视图 + 俯视图（模板没有剖视图文件 → 不画，报告 §6-A1）
        Some((KeyStandard::Gb1097, _)) => vec!["main", "top"],
        None => Vec::new(),
    }
}

/// 1096 三型的族条目（同一尺寸表，型别只改画法/名称，故共享一份 sizes 组装）。
fn sizes_1096(ty: KeyType) -> Vec<serde_json::Value> {
    table_1096(ty)
        .rows
        .iter()
        .map(|r| {
            let allowed = key_1096_l_allowed(r.b);
            let def = key_1096_legal_default(r);
            let mut lengths = vec![def];
            lengths.extend(allowed.iter().copied().filter(|v| (*v - def).abs() > 1e-9));
            serde_json::json!({
                "d": r.b,
                "label": format!("b={}（h={}）", trim(r.b), trim(r.h)),
                "pitch": 0.0,
                "l_min": allowed.first().copied().unwrap_or(def),
                "l_max": allowed.last().copied().unwrap_or(def),
                "lengths": lengths,
                "extra": format!("h={}；默认 L={}", trim(r.h), trim(def)),
            })
        })
        .collect()
}

/// 1097 两型的族条目。
fn sizes_1097(ty: KeyType) -> Vec<serde_json::Value> {
    table_1097(ty)
        .rows
        .iter()
        .map(|r| {
            let allowed = key_1097_l_allowed(r.b);
            let def = key_1097_legal_default(r);
            let mut lengths = vec![def];
            lengths.extend(allowed.iter().copied().filter(|v| (*v - def).abs() > 1e-9));
            serde_json::json!({
                "d": r.b,
                "label": format!("b={}（h={}）", trim(r.b), trim(r.h)),
                "pitch": 0.0,
                "l_min": allowed.first().copied().unwrap_or(def),
                "l_max": allowed.last().copied().unwrap_or(def),
                "lengths": lengths,
                "extra": format!(
                    "h={}；d0=M{} 起键孔、两端 D{}×h{} 沉孔（d1 {}）、孔距 L1={}",
                    trim(r.h), trim(r.d0), trim(r.d_sink), trim(r.h1), trim(r.d1), trim(r.l1)),
            })
        })
        .collect()
}

/// 1096 的型别下拉（parts_gui.html 按 `type_group` 渲染）。
fn type_group_1096() -> serde_json::Value {
    serde_json::json!([
        { "id": "key_1096_a", "name": "A型（双圆头）" },
        { "id": "key_1096_b", "name": "B型（双平头）" },
        { "id": "key_1096_c", "name": "C型（单圆头）" },
    ])
}

/// 1097 的型别下拉。
fn type_group_1097() -> serde_json::Value {
    serde_json::json!([
        { "id": "key_1097_a", "name": "A型（双圆头）" },
        { "id": "key_1097_b", "name": "B型（双平头）" },
    ])
}

/// 1096 族条目的 notes（倒角分档范围、h 误印、L 系列口径）。
fn notes_1096() -> Vec<String> {
    // 倒角范围从唯一数据源 `KEY_1096_CHAMFER_RANGES` 拼出（用户口径固定 0.2，范围只留档）。
    let ranges: Vec<String> = KEY_1096_CHAMFER_RANGES
        .iter()
        .map(|(lo, hi, r)| format!("b={}~{} {}", trim(*lo), trim(*hi), r))
        .collect();
    let mut notes = vec![
        format!(
            "倒角 c 固定 0.2（用户拍板）。注意：GB/T 1096 表 1 的 s（倒角）是按 b 分档的范围：{}\
             —— 供日后调整参考。",
            ranges.join("；")
        ),
        "L：GB/T 1096-2003 表 1 系列（6、8 依据用户素材补充；在线 L 行自 10 起）；\
         约束 L < 10·b；下拉里列该 b 的全部合法 L，默认值 = 源图第三列。"
            .to_string(),
    ];
    if table_1096(KeyType::A).rows.iter().any(|r| r.h_printed.is_some()) {
        notes.push(
            "A 型源图末 4 行 h 印作 32/20/22/25，疑源图误印（在线表与 B/C 型均为 20/22/25/28）；\
             库内取标准值 20/22/25/28，印刷值仅留档。"
                .to_string(),
        );
    }
    let fixed = key_1096_default_corrections();
    if !fixed.is_empty() {
        notes.push(format!(
            "以下档位默认 L 不在合法集内，已就近修正：{}",
            fixed
                .iter()
                .map(|(b, was, now)| format!("b={}：{}→{}", trim(*b), trim(*was), trim(*now)))
                .collect::<Vec<_>>()
                .join("；")
        ));
    }
    notes
}

/// 本组族的目录 JSON 片段（键 = 族 id）——`partgen::catalog_json` 会合并。
pub fn families_json() -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    let hint =
        "基点 = 左端面对称轴（俯视图）/ 左端面×底面（主视图）/ 截面左下角（剖视图）；d 槽位承载 b";
    let note_1097 = {
        let mut v = vec![
            "中央 d0 孔是键自身的**起键（撬出）用螺纹孔，与轴无关** —— 不是把键固定到轴上的孔。"
                .to_string(),
            "主视图照源图：右半剖 + 左半外形 + 45° 断裂折线（用户拍板为规范画法）。".to_string(),
            "螺纹小径 3/4 圈在 2细线层（用户拍板；源图在 4虚线层）。".to_string(),
            "L：GB/T 1097-2003 标准系列 25、28、…、400（易紧通 info_42177 长度行实测），\
             约束 L < 10·b（注③；>400 按 GB/T 321 R20 选取）；下拉列该 b 的全部合法 L。"
                .to_string(),
        ];
        // 源图 specimen 的 C 取表内下限；留档一行。
        if let Some(r) = table_1097(KeyType::A).rows.first() {
            v.push(format!(
                "C 取表内范围下限（b={}：{}）；剖面线间距统一 3.0mm。",
                trim(r.b), r.c_range
            ));
        }
        let fixed = key_1097_default_corrections();
        if !fixed.is_empty() {
            v.push(format!(
                "表内 L=100 在以下档位违反 L<10b，已就近修正：{}",
                fixed
                    .iter()
                    .map(|(b, was, now)| format!("b={}：{}→{}", trim(*b), trim(*was), trim(*now)))
                    .collect::<Vec<_>>()
                    .join("；")
            ));
        }
        v
    };

    for ty in [KeyType::A, KeyType::B, KeyType::C] {
        let id = match ty {
            KeyType::A => "key_1096_a",
            KeyType::B => "key_1096_b",
            KeyType::C => "key_1096_c",
        };
        m.insert(
            id.into(),
            serde_json::json!({
                "id": id,
                "name": format!("圆头普通平键 {}", if ty == KeyType::A { "A型" } else if ty == KeyType::B { "B型" } else { "C型" }),
                "code": "GB/T 1096-2003",
                "iso": "—",
                "implemented": true,
                "views": views_json(id),
                "sizes": sizes_1096(ty),
                "len_label": "长度 L",
                "base_hint": hint,
                "tree_dir": "零件库/键/平键",
                "type_group": type_group_1096(),
                "notes": notes_1096(),
                "source": table_1096(ty).source,
            }),
        );
    }
    for ty in [KeyType::A, KeyType::B] {
        let id = match ty {
            KeyType::A => "key_1097_a",
            KeyType::B => "key_1097_b",
            KeyType::C => unreachable!("GB/T 1097 没有 C 型"),
        };
        m.insert(
            id.into(),
            serde_json::json!({
                "id": id,
                "name": format!("导向平键 {}", if ty == KeyType::A { "A型" } else { "B型" }),
                "code": "GB/T 1097-2003",
                "iso": "—",
                "implemented": true,
                "views": views_json(id),
                "sizes": sizes_1097(ty),
                "len_label": "长度 L",
                "base_hint": hint,
                "tree_dir": "零件库/键/平键",
                "type_group": type_group_1097(),
                "notes": note_1097.clone(),
                "source": table_1097(ty).source,
            }),
        );
    }
    m
}

// ══════════════════════════════════════════════════════════════════════════
// 长度口径与校验
// ══════════════════════════════════════════════════════════════════════════

/// 1096 某 b 允许的 L（L 系列 ∩ `L < 10·b`），升序。
pub fn key_1096_l_allowed(b: f64) -> Vec<f64> {
    KEY_1096_L_SERIES
        .iter()
        .copied()
        .filter(|l| *l < 10.0 * b - 1e-9)
        .collect()
}

/// 1096 某 b 的合法 L 区间 `(min, max)`；b 不在表内 → `None`。
pub fn key_1096_l_range(b: f64) -> Option<(f64, f64)> {
    if row_1096(KeyType::A, b).is_none() {
        return None;
    }
    let allowed = key_1096_l_allowed(b);
    Some((*allowed.first()?, *allowed.last()?))
}

/// 档位默认 L（源图第三列）；若它不在合法集内则就近取合法值（用户 2026-09 口径）。
///
/// 当前 20 档默认值全部合法，`key_1096_default_corrections()` 为空。
pub fn key_1096_legal_default(row: &Key1096Row) -> f64 {
    let allowed = key_1096_l_allowed(row.b);
    if allowed.iter().any(|v| (*v - row.l).abs() < 1e-9) {
        return row.l;
    }
    *allowed
        .iter()
        .min_by(|a, b| {
            (*a - row.l)
                .abs()
                .partial_cmp(&(*b - row.l).abs())
                .unwrap()
        })
        .unwrap_or(&row.l)
}

/// (b, 印刷默认 L, 就近合法值) —— 供测试/报告列出被修正的档位；当前应为空。
pub fn key_1096_default_corrections() -> Vec<(f64, f64, f64)> {
    let mut out = Vec::new();
    for ty in [KeyType::A, KeyType::B, KeyType::C] {
        for r in &table_1096(ty).rows {
            let fixed = key_1096_legal_default(r);
            if (fixed - r.l).abs() > 1e-9 {
                out.push((r.b, r.l, fixed));
            }
        }
    }
    out
}

/// 1096 长度校验：必须是 L 系列值，且 `L < 10·b`。
pub fn check_length_1096(b: f64, l: f64) -> Result<(), String> {
    let Some((lo, hi)) = key_1096_l_range(b) else {
        return Err(format!("GB/T 1096-2003 数据表里没有 b={}", trim(b)));
    };
    if !(l > 0.0) || !l.is_finite() {
        return Err(format!("GB/T 1096 b={}：L 必须是正数（收到 {l}）", trim(b)));
    }
    if l < lo - 1e-9 || l > hi + 1e-9 {
        return Err(format!(
            "GB/T 1096 b={} 的 L 取值范围是 {}…{}（L 系列值且 L<10b），收到 {}",
            trim(b),
            trim(lo),
            trim(hi),
            trim(l)
        ));
    }
    if !key_1096_l_allowed(b).iter().any(|v| (*v - l).abs() < 1e-9) {
        return Err(format!(
            "GB/T 1096 b={} 的 L 须取标准系列值（6, 8, 10, …, 400；L<10b），收到 {}",
            trim(b),
            trim(l)
        ));
    }
    Ok(())
}

/// 1097 长度系列（数据源 = `partsKey1097{A,B}.json` 的 `l_series`；两表必须一致，测试有护栏）。
pub fn key_1097_l_series() -> &'static [f64] {
    &table_1097(KeyType::A).l_series
}

/// 1097 某 b 允许的 L（L 系列 ∩ `L < 10·b`），升序。
///
/// 标准注③：键长应小于 10 倍键宽；>400 时按 GB/T 321 的 R20 系列选取（本库未列）。
pub fn key_1097_l_allowed(b: f64) -> Vec<f64> {
    key_1097_l_series()
        .iter()
        .copied()
        .filter(|l| *l < 10.0 * b - 1e-9)
        .collect()
}

/// 1097 某 b 的合法 L 区间 `(min, max)`；b 不在表内 → `None`。
pub fn key_1097_l_range(b: f64) -> Option<(f64, f64)> {
    if row_1097(KeyType::A, b).is_none() {
        return None;
    }
    let allowed = key_1097_l_allowed(b);
    Some((*allowed.first()?, *allowed.last()?))
}

/// 1097 档位默认 L（表内 L=100；不合法时就近取合法值——b=8/10 会落到 70/90）。
pub fn key_1097_legal_default(row: &Key1097Row) -> f64 {
    let allowed = key_1097_l_allowed(row.b);
    if allowed.iter().any(|v| (*v - row.l).abs() < 1e-9) {
        return row.l;
    }
    *allowed
        .iter()
        .min_by(|a, b| {
            (*a - row.l)
                .abs()
                .partial_cmp(&(*b - row.l).abs())
                .unwrap()
        })
        .unwrap_or(&row.l)
}

/// (b, 表内 L, 就近合法值) —— 供测试/手册列出被修正的档位（b=8→70、b=10→90）。
pub fn key_1097_default_corrections() -> Vec<(f64, f64, f64)> {
    let mut out = Vec::new();
    // A/B 两表逐行相同（系列一致性有测试），只列一份。
    for r in &table_1097(KeyType::A).rows {
        let fixed = key_1097_legal_default(r);
        if (fixed - r.l).abs() > 1e-9 {
            out.push((r.b, r.l, fixed));
        }
    }
    out
}

/// 1097 长度校验：L 必须 ∈ 系列且 `L < 10·b`（标准注③）。
pub fn check_length_1097(b: f64, l: f64) -> Result<(), String> {
    let Some((lo, hi)) = key_1097_l_range(b) else {
        return Err(format!("GB/T 1097-2003 数据表里没有 b={}", trim(b)));
    };
    let row = row_1097(KeyType::A, b).expect("区间来自表行");
    // 尺寸链自检：L2=L/2、L1+2·L3=L（源图/表内各档应恒满足）。
    if (row.l2 - row.l / 2.0).abs() > 1e-9 || (row.l1 + 2.0 * row.l3 - row.l).abs() > 1e-9 {
        return Err(format!(
            "GB/T 1097 b={} 表内尺寸链不一致：L1={}、L2={}、L3={}、L={}（应满足 L2=L/2、L1+2L3=L）",
            trim(b),
            trim(row.l1),
            trim(row.l2),
            trim(row.l3),
            trim(row.l)
        ));
    }
    if !(l > 0.0) || !l.is_finite() {
        return Err(format!("GB/T 1097 b={}：L 必须是正数（收到 {l}）", trim(b)));
    }
    if l < lo - 1e-9 || l > hi + 1e-9 {
        return Err(format!(
            "GB/T 1097 b={} 的 L 取值范围是 {}…{}（L 系列值且 L<10b，GB/T 1097-2003 注③），收到 {}",
            trim(b),
            trim(lo),
            trim(hi),
            trim(l)
        ));
    }
    if !key_1097_l_allowed(b).iter().any(|v| (*v - l).abs() < 1e-9) {
        return Err(format!(
            "GB/T 1097 b={} 的 L 须取标准系列值（25, 28, …, 400；L<10b；>400 按 GB/T 321 R20），收到 {}",
            trim(b),
            trim(l)
        ));
    }
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════
// 生成派发
// ══════════════════════════════════════════════════════════════════════════

/// 本组族的生成派发（`partgen::generate` / `partgen_more::generate` 会先问这里）。
///
/// `d` 槽位承载 **b**（键宽）；`l <= 0` = 用表内该档默认 L。
pub fn generate(family: &str, d: f64, l: f64, view: &str) -> Option<Result<GenPart, String>> {
    let (standard, ty) = family_parts(family)?;
    let allowed = family_views(family);
    if !allowed.contains(&view) {
        return Some(Err(format!(
            "{family} 不提供视图 {view}（可用：{}）",
            allowed.join("/")
        )));
    }
    Some(match standard {
        KeyStandard::Gb1096 => gen_1096(ty, d, l, view),
        KeyStandard::Gb1097 => gen_1097(ty, d, l, view),
    })
}

// ── 1096 普通平键 ─────────────────────────────────────────────────────────

fn gen_1096(ty: KeyType, b: f64, l: f64, view: &str) -> Result<GenPart, String> {
    let row = row_1096(ty, b)
        .ok_or_else(|| format!("GB/T 1096-2003 数据表里没有 b={}", trim(b)))?;
    let l = if l > 0.0 { l } else { row.l };
    check_length_1096(b, l)?;
    let (h, c) = (row.h, C_1096);
    let entities = match view {
        "main" => main_1096(ty, b, h, l, c),
        "top" => top_1096(ty, b, l, c),
        "section" => section_1096(b, h, c),
        other => return Err(format!("key_1096 没有视图 {other}")),
    };
    let weight = weight_1096(ty, b, h, l);
    Ok(GenPart {
        bbox: bbox_of(&entities),
        entities,
        meta: PartMeta {
            code: "GB/T 1096-2003".into(),
            name: format!("圆头普通平键 {}", match ty {
                KeyType::A => "A型",
                KeyType::B => "B型",
                KeyType::C => "C型",
            }),
            spec: format!("{}×{}×{}", trim(b), trim(h), trim(l)),
            material: String::new(),
            weight: format!("{weight:.4}"),
        },
    })
}

/// 1096 主视图：`L×h` 带端部 45° 切角（仅圆头端）+ 两条倒角棱线；源图此视图无中心线。
fn main_1096(ty: KeyType, _b: f64, h: f64, l: f64, c: f64) -> Vec<EntityType> {
    // 端点顺序照源图 POLYLINE（A/C 圆头端 c×c 切角；B 方角）。
    let outline: Vec<[f64; 2]> = match ty {
        KeyType::A => vec![
            [0.0, c], [c, 0.0], [l - c, 0.0], [l, c],
            [l, h - c], [l - c, h], [c, h], [0.0, h - c],
        ],
        KeyType::B => vec![
            [0.0, c], [0.0, 0.0], [l, 0.0], [l, c],
            [l, h - c], [l, h], [0.0, h], [0.0, h - c],
        ],
        KeyType::C => vec![
            [0.0, c], [c, 0.0], [l, 0.0], [l, c],
            [l, h - c], [l, h], [c, h], [0.0, h - c],
        ],
    };
    vec![
        polyline(&outline, true, LAYER_MAIN),
        line([0.0, c], [l, c], LAYER_MAIN),
        line([l, h - c], [0.0, h - c], LAYER_MAIN),
    ]
}

/// 1096 俯视图：`L×b` 外形（A 双圆头 R=b/2｜B 双方头｜C 左圆右方）+ 内缩 c 的倒角棱线
/// + 中心线（两端各伸 3，源图实测）。
fn top_1096(ty: KeyType, b: f64, l: f64, c: f64) -> Vec<EntityType> {
    let half = b / 2.0;
    let ri = half - c;
    let mut en = vec![line([-3.0, 0.0], [l + 3.0, 0.0], LAYER_CENTER)];
    match ty {
        KeyType::A => {
            let (cl, cr) = (half, l - half);
            en.push(arc([cl, 0.0], half, 90.0, 270.0, LAYER_MAIN));
            en.push(arc([cr, 0.0], half, 270.0, 90.0, LAYER_MAIN));
            en.push(line([cl, -half], [cr, -half], LAYER_MAIN));
            en.push(line([cr, half], [cl, half], LAYER_MAIN));
            en.push(line([cl, -ri], [cr, -ri], LAYER_MAIN));
            en.push(line([cr, ri], [cl, ri], LAYER_MAIN));
            en.push(arc([cl, 0.0], ri, 90.0, 270.0, LAYER_MAIN));
            en.push(arc([cr, 0.0], ri, 270.0, 90.0, LAYER_MAIN));
        }
        KeyType::B => {
            // 源图 B 型俯视图为 7 条 LINE（非闭合多段线），逐条照抄。
            en.push(line([0.0, -half], [l, -half], LAYER_MAIN));
            en.push(line([l, half], [0.0, half], LAYER_MAIN));
            en.push(line([0.0, -ri], [l, -ri], LAYER_MAIN));
            en.push(line([0.0, ri], [l, ri], LAYER_MAIN));
            en.push(line([l, half], [l, -half], LAYER_MAIN));
            en.push(line([0.0, half], [0.0, -half], LAYER_MAIN));
        }
        KeyType::C => {
            // 左圆右方：左端半圆，右端方角（内轮廓线画到端面）。
            let cl = half;
            en.push(arc([cl, 0.0], half, 90.0, 270.0, LAYER_MAIN));
            en.push(line([cl, -half], [l, -half], LAYER_MAIN));
            en.push(line([l, half], [cl, half], LAYER_MAIN));
            en.push(line([cl, -ri], [l, -ri], LAYER_MAIN));
            en.push(line([cl, ri], [l, ri], LAYER_MAIN));
            en.push(arc([cl, 0.0], ri, 90.0, 270.0, LAYER_MAIN));
            en.push(line([l, half], [l, -half], LAYER_MAIN));
        }
    }
    en
}

/// 1096 剖视图：`b×h` 四角切 c 的八边形 + ANSI31 剖面线（**无中心线**，照源图）。
fn section_1096(b: f64, h: f64, c: f64) -> Vec<EntityType> {
    let verts: Vec<[f64; 2]> = vec![
        [0.0, c], [c, 0.0], [b - c, 0.0], [b, c],
        [b, h - c], [b - c, h], [c, h], [0.0, h - c],
    ];
    vec![
        polyline(&verts, true, LAYER_MAIN),
        hatch_ansi31_scaled(&verts, 0.0, hatch_scale_3mm()),
    ]
}

/// 1096 单件重量估算（钢 7.85 g/cm³；面积按端型取，忽略倒角）。
fn weight_1096(ty: KeyType, b: f64, h: f64, l: f64) -> f64 {
    let area = match ty {
        KeyType::A => (l - b).max(0.0) * b + std::f64::consts::PI * b * b / 4.0,
        KeyType::B => l * b,
        KeyType::C => (l - b / 2.0).max(0.0) * b + std::f64::consts::PI * b * b / 8.0,
    };
    area * h * 7.85e-3 / 1000.0
}

// ── 1097 导向平键 ─────────────────────────────────────────────────────────

fn gen_1097(ty: KeyType, b: f64, l: f64, view: &str) -> Result<GenPart, String> {
    let row = row_1097(ty, b)
        .ok_or_else(|| format!("GB/T 1097-2003 数据表里没有 b={}", trim(b)))?;
    // `l=0` = 该档默认 L（表内 100；b=8/10 就近合法化为 70/90）。
    let l = if l > 0.0 { l } else { key_1097_legal_default(row) };
    check_length_1097(b, l)?;
    let entities = match view {
        "main" => main_1097(ty, row, l),
        "top" => top_1097(ty, row, l),
        other => return Err(format!("key_1097 没有视图 {other}")),
    };
    let weight = weight_1097(ty, row, l);
    Ok(GenPart {
        bbox: bbox_of(&entities),
        entities,
        meta: PartMeta {
            code: "GB/T 1097-2003".into(),
            name: format!("导向平键 {}", if ty == KeyType::A { "A型" } else { "B型" }),
            spec: format!("{}×{}×{}", trim(row.b), trim(row.h), trim(l)),
            material: String::new(),
            weight: format!("{weight:.4}"),
        },
    })
}

/// 1097 主视图（源图 = 右半剖 + 左半外形 + 断裂折线；用户拍板照抄）。
///
/// 图元顺序照源图（WCS，已去镜像重复）；坐标详见反解报告 §3.2/§3.4。
fn main_1097(ty: KeyType, r: &Key1097Row, l: f64) -> Vec<EntityType> {
    let (h, c) = (r.h, r.c);
    let (xh, d0, d1, d) = (r.l1 / 2.0, r.d0, r.d1, r.d_sink);
    let (h1, c1, l0) = (r.h1, r.c1, r.l0);
    let rcs = d0 / 2.0 - c1 * 60f64.to_radians().tan(); // 锪锥小端半径
    let y0 = h - l0; // 起键孔底（当前各档 L0=h → 通高）
    // 断裂线：源图 specimen L1=60、D=6 → 26；按"d1 孔外缘偏 1mm"推广（大 D 档不会落进孔里）。
    let xb = xh - d / 2.0 - BREAK_GAP;
    let xf = xb - BREAK_RUN;
    let mut en = vec![
        line([0.0, -3.0], [0.0, h + 3.0], LAYER_CENTER),
        line([xh, -3.0], [xh, h + 3.0], LAYER_CENTER),
        line([-xh, -3.0], [-xh, h + 3.0], LAYER_CENTER),
    ];
    // 中央起键螺纹孔：口部 120° 锪锥 + 大径竖线 + 小端竖线（照源图；孔与轴无关）。
    en.push(polyline(
        &[[-rcs, y0], [-rcs, h - c1], [0.0, h - c1]],
        false,
        LAYER_MAIN,
    ));
    en.push(polyline(
        &[[-rcs, h - c1], [-d0 / 2.0, h], [-d0 / 2.0, y0]],
        false,
        LAYER_MAIN,
    ));
    // 左端孔：左半为外形 → 近/远两条隐藏线（4虚线层）。
    en.push(polyline(
        &[
            [-(xh - d1 / 2.0), 0.0],
            [-(xh - d1 / 2.0), h - h1],
            [-(xh - d / 2.0), h - h1],
            [-(xh - d / 2.0), h],
        ],
        false,
        LAYER_HIDDEN,
    ));
    // 右端孔：右半剖 → 可见轮廓（1轮廓实线层）。
    en.push(polyline(
        &[
            [xh - d1 / 2.0, 0.0],
            [xh - d1 / 2.0, h - h1],
            [xh - d / 2.0, h - h1],
            [xh - d / 2.0, h],
        ],
        false,
        LAYER_MAIN,
    ));
    en.push(line(
        [xh - d1 / 2.0, h - h1],
        [xh, h - h1],
        LAYER_MAIN,
    ));
    // 外形 + 倒角棱线（左半外形；断裂线止于 -xb）。
    match ty {
        KeyType::A => {
            en.push(polyline(
                &[
                    [0.0, 0.0],
                    [-(l / 2.0 - c), 0.0],
                    [-l / 2.0, c],
                    [-l / 2.0, h - c],
                    [-(l / 2.0 - c), h],
                    [0.0, h],
                ],
                false,
                LAYER_MAIN,
            ));
            en.push(line([-l / 2.0, h - c], [-xb, h - c], LAYER_MAIN));
            en.push(line([-l / 2.0, c], [-xb, c], LAYER_MAIN));
        }
        KeyType::B => {
            en.push(line([-l / 2.0, h - c], [-xb, h - c], LAYER_MAIN));
            en.push(line([-l / 2.0, c], [-xb, c], LAYER_MAIN));
        }
        KeyType::C => unreachable!("GB/T 1097 没有 C 型"),
    }
    // 材料剖面多边形（中段 / 右端 / 左中带断裂边界）。
    let mid = vec![
        [d0 / 2.0, 0.0],
        [d0 / 2.0, h],
        [xh - d / 2.0, h],
        [xh - d / 2.0, h - h1],
        [xh - d1 / 2.0, h - h1],
        [xh - d1 / 2.0, 0.0],
    ];
    let right = match ty {
        KeyType::A => vec![
            [xh + d1 / 2.0, 0.0],
            [xh + d1 / 2.0, h - h1],
            [xh + d / 2.0, h - h1],
            [xh + d / 2.0, h],
            [l / 2.0 - c, h],
            [l / 2.0, h - c],
            [l / 2.0, c],
            [l / 2.0 - c, 0.0],
        ],
        KeyType::B => vec![
            [xh + d1 / 2.0, 0.0],
            [xh + d1 / 2.0, h - h1],
            [xh + d / 2.0, h - h1],
            [xh + d / 2.0, h],
            [l / 2.0, h],
            [l / 2.0, 0.0],
        ],
        KeyType::C => unreachable!("GB/T 1097 没有 C 型"),
    };
    let left = vec![
        [-xf, h],
        [-xb, h - c],
        [-xb, c],
        [-xf, 0.0],
        [-d0 / 2.0, 0.0],
        [-d0 / 2.0, h],
    ];
    en.push(polyline(&mid, true, LAYER_MAIN));
    en.push(polyline(&right, true, LAYER_MAIN));
    en.push(polyline(&left, true, LAYER_MAIN));
    // 中央孔右半 + 左端孔远侧隐藏线 + 右端孔远侧可见轮廓 + 沉孔底（镜像对的 WCS 坐标）。
    en.push(polyline(
        &[[rcs, y0], [rcs, h - c1], [0.0, h - c1]],
        false,
        LAYER_MAIN,
    ));
    en.push(polyline(
        &[[rcs, h - c1], [d0 / 2.0, h], [d0 / 2.0, y0]],
        false,
        LAYER_MAIN,
    ));
    en.push(polyline(
        &[
            [-(xh + d1 / 2.0), 0.0],
            [-(xh + d1 / 2.0), h - h1],
            [-(xh + d / 2.0), h - h1],
            [-(xh + d / 2.0), h],
        ],
        false,
        LAYER_HIDDEN,
    ));
    en.push(polyline(
        &[
            [xh + d1 / 2.0, 0.0],
            [xh + d1 / 2.0, h - h1],
            [xh + d / 2.0, h - h1],
            [xh + d / 2.0, h],
        ],
        false,
        LAYER_MAIN,
    ));
    en.push(line(
        [xh + d1 / 2.0, h - h1],
        [xh, h - h1],
        LAYER_MAIN,
    ));
    match ty {
        KeyType::A => {
            en.push(polyline(
                &[
                    [0.0, 0.0],
                    [l / 2.0 - c, 0.0],
                    [l / 2.0, c],
                    [l / 2.0, h - c],
                    [l / 2.0 - c, h],
                    [0.0, h],
                ],
                false,
                LAYER_MAIN,
            ));
        }
        KeyType::B => {
            en.push(polyline(
                &[[-l / 2.0, 0.0], [-l / 2.0, h], [l / 2.0, h], [l / 2.0, 0.0]],
                true,
                LAYER_MAIN,
            ));
        }
        KeyType::C => unreachable!(),
    }
    // 3 片剖面线（中段 / 右端 / 左中），间距统一 3.0mm。
    en.push(hatch_ansi31_scaled(&mid, 0.0, hatch_scale_3mm()));
    en.push(hatch_ansi31_scaled(&right, 0.0, hatch_scale_3mm()));
    en.push(hatch_ansi31_scaled(&left, 0.0, hatch_scale_3mm()));
    en
}

/// 1097 俯视图：A 跑道形 R=b/2 / B 矩形 + 倒角内轮廓 + 三组中心线 + 全部孔圈。
fn top_1097(ty: KeyType, r: &Key1097Row, l: f64) -> Vec<EntityType> {
    let (b, c) = (r.b, r.c);
    let (xh, d0, d1, d) = (r.l1 / 2.0, r.d0, r.d1, r.d_sink);
    let rcs = d0 / 2.0 - r.c1 * 60f64.to_radians().tan();
    let rmin = MINOR_THREAD_FACTOR * d0 / 2.0;
    let half = b / 2.0;
    let ri = half - c;
    let over = half + 3.0;
    let mut en = Vec::new();
    // 中央起键螺纹孔三圈：大径实线、锪锥小端实线、小径 3/4 圈（**2细线层**，用户拍板）。
    en.push(circle([0.0, 0.0], d0 / 2.0, LAYER_MAIN));
    en.push(circle([0.0, 0.0], rcs, LAYER_MAIN));
    // 两端 d1 通孔 + D 沉孔圈。
    for x in [-xh, xh] {
        en.push(circle([x, 0.0], d1 / 2.0, LAYER_MAIN));
        en.push(circle([x, 0.0], d / 2.0, LAYER_MAIN));
    }
    // 中心线：纵向轴、两端孔轴；横向轴两端各伸 3。
    en.push(line([0.0, -over], [0.0, over], LAYER_CENTER));
    en.push(line([-(l / 2.0 + 3.0), 0.0], [l / 2.0 + 3.0, 0.0], LAYER_CENTER));
    for x in [-xh, xh] {
        en.push(line([x, -over], [x, over], LAYER_CENTER));
    }
    // 螺纹小径 3/4 圈：缺口在右下象限（0°→270° 逆时针），照源图方位。
    en.push(arc([0.0, 0.0], rmin, 0.0, 270.0, LAYER_THIN));
    // 外形 + 倒角内轮廓。
    match ty {
        KeyType::A => {
            // 左右半各画一段：源图用镜像生成，断裂在 x=0（不是重复实体，是两段共线轮廓）。
            let (cl, cr) = (l / 2.0 - half, -(l / 2.0 - half));
            en.push(arc([cr, 0.0], half, 90.0, 270.0, LAYER_MAIN));
            en.push(arc([cl, 0.0], half, 270.0, 90.0, LAYER_MAIN));
            for x in [cr, cl] {
                en.push(line([x, -half], [0.0, -half], LAYER_MAIN));
                en.push(line([x, half], [0.0, half], LAYER_MAIN));
                en.push(line([x, -ri], [0.0, -ri], LAYER_MAIN));
                en.push(line([x, ri], [0.0, ri], LAYER_MAIN));
            }
            en.push(arc([cr, 0.0], ri, 90.0, 270.0, LAYER_MAIN));
            en.push(arc([cl, 0.0], ri, 270.0, 90.0, LAYER_MAIN));
        }
        KeyType::B => {
            en.push(polyline(
                &[
                    [-l / 2.0, half],
                    [l / 2.0, half],
                    [l / 2.0, -half],
                    [-l / 2.0, -half],
                ],
                true,
                LAYER_MAIN,
            ));
            en.push(line([-l / 2.0, ri], [l / 2.0, ri], LAYER_MAIN));
            en.push(line([-l / 2.0, -ri], [l / 2.0, -ri], LAYER_MAIN));
        }
        KeyType::C => unreachable!("GB/T 1097 没有 C 型"),
    }
    en
}

/// 1097 单件重量估算（钢 7.85 g/cm³；跑道/矩形面积 − 孔体积）。
fn weight_1097(ty: KeyType, r: &Key1097Row, l: f64) -> f64 {
    let area = match ty {
        KeyType::A => (l - r.b).max(0.0) * r.b + std::f64::consts::PI * r.b * r.b / 4.0,
        KeyType::B => r.l * r.b,
        KeyType::C => unreachable!(),
    };
    let mut vol = area * r.h;
    vol -= std::f64::consts::PI * (r.d0 / 2.0).powi(2) * r.l0;
    for _ in 0..2 {
        vol -= std::f64::consts::PI * (r.d1 / 2.0).powi(2) * r.h;
        vol -= std::f64::consts::PI / 4.0 * (r.d_sink * r.d_sink - r.d1 * r.d1) * r.h1;
    }
    (vol.max(0.0)) * 7.85e-3 / 1000.0
}

// ══════════════════════════════════════════════════════════════════════════
// 包围盒
// ══════════════════════════════════════════════════════════════════════════

/// 包围盒：Line/LwPolyline 取顶点；Circle 取 center±r；Arc 取采样点（16 段）。
fn bbox_of(entities: &[EntityType]) -> [f64; 4] {
    let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    let mut push = |x: f64, y: f64| {
        b[0] = b[0].min(x);
        b[1] = b[1].min(y);
        b[2] = b[2].max(x);
        b[3] = b[3].max(y);
    };
    for e in entities {
        match e {
            EntityType::Line(l) => {
                push(l.start.x, l.start.y);
                push(l.end.x, l.end.y);
            }
            EntityType::Circle(c) => {
                push(c.center.x - c.radius, c.center.y - c.radius);
                push(c.center.x + c.radius, c.center.y + c.radius);
            }
            EntityType::Arc(a) => {
                let (s, e) = (a.start_angle, a.end_angle);
                let mut sweep = e - s;
                while sweep <= 0.0 {
                    sweep += std::f64::consts::TAU;
                }
                for i in 0..=16 {
                    let ang = s + sweep * (i as f64 / 16.0);
                    push(
                        a.center.x + a.radius * ang.cos(),
                        a.center.y + a.radius * ang.sin(),
                    );
                }
            }
            EntityType::LwPolyline(p) => {
                for v in &p.vertices {
                    push(v.location.x, v.location.y);
                }
            }
            // HATCH 的边界由轮廓多段线覆盖，不另计。
            _ => {}
        }
    }
    b
}

// ══════════════════════════════════════════════════════════════════════════
// 测试
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::partgen::generate as gen_all;
    use crate::partgen_kit::{hatch_perpendicular_spacing, part_svg};
    use ocs_plugin_api::host::acadrust::types::Color;

    const OCSM_LAYERS: [&str; 5] = [
        LAYER_MAIN,
        LAYER_THIN,
        LAYER_CENTER,
        LAYER_HIDDEN,
        crate::partgen_kit::LAYER_HATCH,
    ];

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-4
    }

    /// 实体规范化字符串（Line 端点方向无关；Polyline 保留顶点顺序与闭合位；含图层）。
    fn canon(e: &EntityType) -> String {
        let layer = e.common().layer.clone();
        match e {
            EntityType::Line(l) => {
                let (mut a, mut b) = ((l.start.x, l.start.y), (l.end.x, l.end.y));
                if a > b {
                    std::mem::swap(&mut a, &mut b);
                }
                format!("LINE ({:.4},{:.4})-({:.4},{:.4}) [{layer}]", a.0, a.1, b.0, b.1)
            }
            EntityType::Arc(a) => format!(
                "ARC c=({:.4},{:.4}) r={:.6} {:.3}..{:.3} [{layer}]",
                a.center.x,
                a.center.y,
                a.radius,
                a.start_angle.to_degrees(),
                a.end_angle.to_degrees()
            ),
            EntityType::Circle(c) => format!(
                "CIRCLE c=({:.4},{:.4}) r={:.6} [{layer}]",
                c.center.x, c.center.y, c.radius
            ),
            EntityType::LwPolyline(pl) => {
                let vs: Vec<String> = pl
                    .vertices
                    .iter()
                    .map(|v| format!("({:.4},{:.4})", v.location.x, v.location.y))
                    .collect();
                format!(
                    "POLYLINE{}{} [{layer}]",
                    if pl.is_closed { " closed" } else { "" },
                    vs.join("")
                )
            }
            EntityType::Hatch(h) => format!(
                "HATCH angle={:.3} scale={:.6} [{layer}]",
                h.pattern_angle.to_degrees(),
                h.pattern_scale
            ),
            other => format!("OTHER {:?} [{layer}]", std::mem::discriminant(other)),
        }
    }

    fn canonical(entities: &[EntityType]) -> Vec<String> {
        let mut v: Vec<String> = entities.iter().map(canon).collect();
        v.sort();
        v
    }

    /// 逐图元对模板：实体数 + 规范化集合（类型/坐标/图层）完全一致。
    fn assert_entities(entities: &[EntityType], expected: &[EntityType], what: &str) {
        assert!(
            !entities.iter().any(|e| matches!(e, EntityType::Dimension(_))),
            "{what}: 不应有任何尺寸标注"
        );
        assert_eq!(entities.len(), expected.len(), "{what}: 图元数不一致");
        let (got, want) = (canonical(entities), canonical(expected));
        if got != want {
            let mut msg = format!("{what}: 逐图元不一致\n");
            for c in &want {
                if !got.contains(c) {
                    msg.push_str(&format!("  缺: {c}\n"));
                }
            }
            for c in &got {
                if !want.contains(c) {
                    msg.push_str(&format!("  多: {c}\n"));
                }
            }
            panic!("{msg}");
        }
    }

    fn assert_template(p: &GenPart, expected: &[EntityType], what: &str) {
        assert_entities(&p.entities, expected, what);
    }

    // ── 1096 specimen b=2、h=2、L=6（模板坐标见 平键_几何反解.md §2）──

    fn exp_1096_main(ty: KeyType) -> Vec<EntityType> {
        let outline: Vec<[f64; 2]> = match ty {
            KeyType::A => vec![
                [0.0, 0.2], [0.2, 0.0], [5.8, 0.0], [6.0, 0.2],
                [6.0, 1.8], [5.8, 2.0], [0.2, 2.0], [0.0, 1.8],
            ],
            KeyType::B => vec![
                [0.0, 0.2], [0.0, 0.0], [6.0, 0.0], [6.0, 0.2],
                [6.0, 1.8], [6.0, 2.0], [0.0, 2.0], [0.0, 1.8],
            ],
            KeyType::C => vec![
                [0.0, 0.2], [0.2, 0.0], [6.0, 0.0], [6.0, 0.2],
                [6.0, 1.8], [6.0, 2.0], [0.2, 2.0], [0.0, 1.8],
            ],
        };
        vec![
            polyline(&outline, true, LAYER_MAIN),
            line([0.0, 0.2], [6.0, 0.2], LAYER_MAIN),
            line([6.0, 1.8], [0.0, 1.8], LAYER_MAIN),
        ]
    }

    fn exp_1096_top(ty: KeyType) -> Vec<EntityType> {
        let mut en = vec![line([-3.0, 0.0], [9.0, 0.0], LAYER_CENTER)];
        match ty {
            KeyType::A => {
                en.push(arc([1.0, 0.0], 1.0, 90.0, 270.0, LAYER_MAIN));
                en.push(arc([5.0, 0.0], 1.0, 270.0, 90.0, LAYER_MAIN));
                en.push(line([1.0, -1.0], [5.0, -1.0], LAYER_MAIN));
                en.push(line([5.0, 1.0], [1.0, 1.0], LAYER_MAIN));
                en.push(line([1.0, -0.8], [5.0, -0.8], LAYER_MAIN));
                en.push(line([1.0, 0.8], [5.0, 0.8], LAYER_MAIN));
                en.push(arc([1.0, 0.0], 0.8, 90.0, 270.0, LAYER_MAIN));
                en.push(arc([5.0, 0.0], 0.8, 270.0, 90.0, LAYER_MAIN));
            }
            KeyType::B => {
                en.push(line([0.0, -1.0], [6.0, -1.0], LAYER_MAIN));
                en.push(line([6.0, 1.0], [0.0, 1.0], LAYER_MAIN));
                en.push(line([0.0, -0.8], [6.0, -0.8], LAYER_MAIN));
                en.push(line([0.0, 0.8], [6.0, 0.8], LAYER_MAIN));
                en.push(line([6.0, 1.0], [6.0, -1.0], LAYER_MAIN));
                en.push(line([0.0, 1.0], [0.0, -1.0], LAYER_MAIN));
            }
            KeyType::C => {
                en.push(arc([1.0, 0.0], 1.0, 90.0, 270.0, LAYER_MAIN));
                en.push(line([1.0, -1.0], [6.0, -1.0], LAYER_MAIN));
                en.push(line([6.0, 1.0], [1.0, 1.0], LAYER_MAIN));
                en.push(line([1.0, -0.8], [6.0, -0.8], LAYER_MAIN));
                en.push(line([1.0, 0.8], [6.0, 0.8], LAYER_MAIN));
                en.push(arc([1.0, 0.0], 0.8, 90.0, 270.0, LAYER_MAIN));
                en.push(line([6.0, 1.0], [6.0, -1.0], LAYER_MAIN));
            }
        }
        en
    }

    fn exp_1096_section() -> Vec<EntityType> {
        let pts = vec![
            [0.0, 0.2], [0.2, 0.0], [1.8, 0.0], [2.0, 0.2],
            [2.0, 1.8], [1.8, 2.0], [0.2, 2.0], [0.0, 1.8],
        ];
        vec![
            polyline(&pts, true, LAYER_MAIN),
            // 源图 ANSI31 scale=0.25（≈0.79mm）；用户口径③改为全局 3.0mm。
            hatch_ansi31_scaled(&pts, 0.0, 3.0 / ANSI31_SPACING_MM),
        ]
    }

    #[test]
    fn template_1096_specimen_b2_h2_l6() {
        for (fam, ty) in [
            ("key_1096_a", KeyType::A),
            ("key_1096_b", KeyType::B),
            ("key_1096_c", KeyType::C),
        ] {
            let main = gen_all(fam, 2.0, 6.0, "main").unwrap();
            assert_template(&main, &exp_1096_main(ty), &format!("{fam} 主视图"));
            let top = gen_all(fam, 2.0, 6.0, "top").unwrap();
            assert_template(&top, &exp_1096_top(ty), &format!("{fam} 俯视图"));
            let sec = gen_all(fam, 2.0, 6.0, "section").unwrap();
            assert_template(&sec, &exp_1096_section(), &format!("{fam} 剖视图"));
            // 剖视图三型逐图元同构（源图实测）。
            assert_eq!(main.meta.spec, "2×2×6", "{fam} 规格文本");
        }
    }

    // ── 1097 specimen b=8、h=7、L=100（模板见 平键_几何反解.md §3）──

    fn exp_1097_main_a() -> Vec<EntityType> {
        let (h, l, c) = (7.0, 100.0, 0.25);
        let (xh, d0, d1, d) = (30.0, 3.0, 3.4, 6.0);
        let (h1, c1, l0) = (2.4, 0.3, 7.0);
        let rcs = d0 / 2.0 - c1 * 60f64.to_radians().tan();
        let y0 = h - l0;
        let xb = xh - d / 2.0 - BREAK_GAP;
        let xf = xb - BREAK_RUN;
        let mid = vec![
            [d0 / 2.0, 0.0], [d0 / 2.0, h], [xh - d / 2.0, h],
            [xh - d / 2.0, h - h1], [xh - d1 / 2.0, h - h1], [xh - d1 / 2.0, 0.0],
        ];
        let right = vec![
            [xh + d1 / 2.0, 0.0], [xh + d1 / 2.0, h - h1], [xh + d / 2.0, h - h1],
            [xh + d / 2.0, h], [l / 2.0 - c, h], [l / 2.0, h - c],
            [l / 2.0, c], [l / 2.0 - c, 0.0],
        ];
        let left = vec![
            [-xf, h], [-xb, h - c], [-xb, c], [-xf, 0.0],
            [-d0 / 2.0, 0.0], [-d0 / 2.0, h],
        ];
        vec![
            line([0.0, -3.0], [0.0, h + 3.0], LAYER_CENTER),
            line([xh, -3.0], [xh, h + 3.0], LAYER_CENTER),
            line([-xh, -3.0], [-xh, h + 3.0], LAYER_CENTER),
            polyline(&[[-rcs, y0], [-rcs, h - c1], [0.0, h - c1]], false, LAYER_MAIN),
            polyline(&[[-rcs, h - c1], [-d0 / 2.0, h], [-d0 / 2.0, y0]], false, LAYER_MAIN),
            polyline(
                &[[-(xh - d1 / 2.0), 0.0], [-(xh - d1 / 2.0), h - h1], [-(xh - d / 2.0), h - h1], [-(xh - d / 2.0), h]],
                false, LAYER_HIDDEN,
            ),
            polyline(
                &[[xh - d1 / 2.0, 0.0], [xh - d1 / 2.0, h - h1], [xh - d / 2.0, h - h1], [xh - d / 2.0, h]],
                false, LAYER_MAIN,
            ),
            line([xh - d1 / 2.0, h - h1], [xh, h - h1], LAYER_MAIN),
            polyline(
                &[[0.0, 0.0], [-(l / 2.0 - c), 0.0], [-l / 2.0, c], [-l / 2.0, h - c], [-(l / 2.0 - c), h], [0.0, h]],
                false, LAYER_MAIN,
            ),
            line([-l / 2.0, h - c], [-xb, h - c], LAYER_MAIN),
            line([-l / 2.0, c], [-xb, c], LAYER_MAIN),
            polyline(&mid, true, LAYER_MAIN),
            polyline(&right, true, LAYER_MAIN),
            polyline(&left, true, LAYER_MAIN),
            polyline(&[[rcs, y0], [rcs, h - c1], [0.0, h - c1]], false, LAYER_MAIN),
            polyline(&[[rcs, h - c1], [d0 / 2.0, h], [d0 / 2.0, y0]], false, LAYER_MAIN),
            polyline(
                &[[-(xh + d1 / 2.0), 0.0], [-(xh + d1 / 2.0), h - h1], [-(xh + d / 2.0), h - h1], [-(xh + d / 2.0), h]],
                false, LAYER_HIDDEN,
            ),
            polyline(
                &[[xh + d1 / 2.0, 0.0], [xh + d1 / 2.0, h - h1], [xh + d / 2.0, h - h1], [xh + d / 2.0, h]],
                false, LAYER_MAIN,
            ),
            line([xh + d1 / 2.0, h - h1], [xh, h - h1], LAYER_MAIN),
            polyline(
                &[[0.0, 0.0], [l / 2.0 - c, 0.0], [l / 2.0, c], [l / 2.0, h - c], [l / 2.0 - c, h], [0.0, h]],
                false, LAYER_MAIN,
            ),
            hatch_ansi31_scaled(&mid, 0.0, 3.0 / ANSI31_SPACING_MM),
            hatch_ansi31_scaled(&right, 0.0, 3.0 / ANSI31_SPACING_MM),
            hatch_ansi31_scaled(&left, 0.0, 3.0 / ANSI31_SPACING_MM),
        ]
    }

    fn exp_1097_main_b() -> Vec<EntityType> {
        let (h, l, c) = (7.0, 100.0, 0.25);
        let (xh, d0, d1, d) = (30.0, 3.0, 3.4, 6.0);
        let (h1, c1, l0) = (2.4, 0.3, 7.0);
        let rcs = d0 / 2.0 - c1 * 60f64.to_radians().tan();
        let y0 = h - l0;
        let xb = xh - d / 2.0 - BREAK_GAP;
        let xf = xb - BREAK_RUN;
        let mid = vec![
            [d0 / 2.0, 0.0], [d0 / 2.0, h], [xh - d / 2.0, h],
            [xh - d / 2.0, h - h1], [xh - d1 / 2.0, h - h1], [xh - d1 / 2.0, 0.0],
        ];
        let right = vec![
            [xh + d1 / 2.0, 0.0], [xh + d1 / 2.0, h - h1], [xh + d / 2.0, h - h1],
            [xh + d / 2.0, h], [l / 2.0, h], [l / 2.0, 0.0],
        ];
        let left = vec![
            [-xf, h], [-xb, h - c], [-xb, c], [-xf, 0.0],
            [-d0 / 2.0, 0.0], [-d0 / 2.0, h],
        ];
        vec![
            line([0.0, -3.0], [0.0, h + 3.0], LAYER_CENTER),
            line([xh, -3.0], [xh, h + 3.0], LAYER_CENTER),
            line([-xh, -3.0], [-xh, h + 3.0], LAYER_CENTER),
            polyline(&[[-rcs, y0], [-rcs, h - c1], [0.0, h - c1]], false, LAYER_MAIN),
            polyline(&[[-rcs, h - c1], [-d0 / 2.0, h], [-d0 / 2.0, y0]], false, LAYER_MAIN),
            polyline(
                &[[-(xh - d1 / 2.0), 0.0], [-(xh - d1 / 2.0), h - h1], [-(xh - d / 2.0), h - h1], [-(xh - d / 2.0), h]],
                false, LAYER_HIDDEN,
            ),
            polyline(
                &[[xh - d1 / 2.0, 0.0], [xh - d1 / 2.0, h - h1], [xh - d / 2.0, h - h1], [xh - d / 2.0, h]],
                false, LAYER_MAIN,
            ),
            line([xh - d1 / 2.0, h - h1], [xh, h - h1], LAYER_MAIN),
            line([-l / 2.0, h - c], [-xb, h - c], LAYER_MAIN),
            line([-l / 2.0, c], [-xb, c], LAYER_MAIN),
            polyline(&mid, true, LAYER_MAIN),
            polyline(&right, true, LAYER_MAIN),
            polyline(&left, true, LAYER_MAIN),
            polyline(&[[rcs, y0], [rcs, h - c1], [0.0, h - c1]], false, LAYER_MAIN),
            polyline(&[[rcs, h - c1], [d0 / 2.0, h], [d0 / 2.0, y0]], false, LAYER_MAIN),
            polyline(
                &[[-(xh + d1 / 2.0), 0.0], [-(xh + d1 / 2.0), h - h1], [-(xh + d / 2.0), h - h1], [-(xh + d / 2.0), h]],
                false, LAYER_HIDDEN,
            ),
            polyline(
                &[[xh + d1 / 2.0, 0.0], [xh + d1 / 2.0, h - h1], [xh + d / 2.0, h - h1], [xh + d / 2.0, h]],
                false, LAYER_MAIN,
            ),
            line([xh + d1 / 2.0, h - h1], [xh, h - h1], LAYER_MAIN),
            polyline(&[[-l / 2.0, 0.0], [-l / 2.0, h], [l / 2.0, h], [l / 2.0, 0.0]], true, LAYER_MAIN),
            hatch_ansi31_scaled(&mid, 0.0, 3.0 / ANSI31_SPACING_MM),
            hatch_ansi31_scaled(&right, 0.0, 3.0 / ANSI31_SPACING_MM),
            hatch_ansi31_scaled(&left, 0.0, 3.0 / ANSI31_SPACING_MM),
        ]
    }

    fn exp_1097_top_a() -> Vec<EntityType> {
        let (b, l, c) = (8.0, 100.0, 0.25);
        let (xh, d0, d1, d) = (30.0, 3.0, 3.4, 6.0);
        let rcs = d0 / 2.0 - 0.3 * 60f64.to_radians().tan();
        let rmin = 0.85 * d0 / 2.0;
        let half = b / 2.0;
        let ri = half - c;
        let over = half + 3.0;
        let cl = l / 2.0 - half;
        vec![
            circle([0.0, 0.0], d0 / 2.0, LAYER_MAIN),
            circle([0.0, 0.0], rcs, LAYER_MAIN),
            circle([-xh, 0.0], d1 / 2.0, LAYER_MAIN),
            circle([-xh, 0.0], d / 2.0, LAYER_MAIN),
            circle([xh, 0.0], d1 / 2.0, LAYER_MAIN),
            circle([xh, 0.0], d / 2.0, LAYER_MAIN),
            line([0.0, -over], [0.0, over], LAYER_CENTER),
            line([-(l / 2.0 + 3.0), 0.0], [l / 2.0 + 3.0, 0.0], LAYER_CENTER),
            line([-xh, -over], [-xh, over], LAYER_CENTER),
            line([xh, -over], [xh, over], LAYER_CENTER),
            arc([0.0, 0.0], rmin, 0.0, 270.0, LAYER_THIN),
            arc([-cl, 0.0], half, 90.0, 270.0, LAYER_MAIN),
            arc([cl, 0.0], half, 270.0, 90.0, LAYER_MAIN),
            line([-cl, -half], [0.0, -half], LAYER_MAIN),
            line([-cl, half], [0.0, half], LAYER_MAIN),
            line([cl, -half], [0.0, -half], LAYER_MAIN),
            line([cl, half], [0.0, half], LAYER_MAIN),
            line([-cl, -ri], [0.0, -ri], LAYER_MAIN),
            line([-cl, ri], [0.0, ri], LAYER_MAIN),
            line([cl, -ri], [0.0, -ri], LAYER_MAIN),
            line([cl, ri], [0.0, ri], LAYER_MAIN),
            arc([-cl, 0.0], ri, 90.0, 270.0, LAYER_MAIN),
            arc([cl, 0.0], ri, 270.0, 90.0, LAYER_MAIN),
        ]
    }

    fn exp_1097_top_b() -> Vec<EntityType> {
        let (b, l, c) = (8.0, 100.0, 0.25);
        let (xh, d0, d1, d) = (30.0, 3.0, 3.4, 6.0);
        let rcs = d0 / 2.0 - 0.3 * 60f64.to_radians().tan();
        let rmin = 0.85 * d0 / 2.0;
        let half = b / 2.0;
        let ri = half - c;
        let over = half + 3.0;
        vec![
            circle([0.0, 0.0], d0 / 2.0, LAYER_MAIN),
            circle([0.0, 0.0], rcs, LAYER_MAIN),
            circle([-xh, 0.0], d1 / 2.0, LAYER_MAIN),
            circle([-xh, 0.0], d / 2.0, LAYER_MAIN),
            circle([xh, 0.0], d1 / 2.0, LAYER_MAIN),
            circle([xh, 0.0], d / 2.0, LAYER_MAIN),
            line([0.0, -over], [0.0, over], LAYER_CENTER),
            line([-(l / 2.0 + 3.0), 0.0], [l / 2.0 + 3.0, 0.0], LAYER_CENTER),
            line([-xh, -over], [-xh, over], LAYER_CENTER),
            line([xh, -over], [xh, over], LAYER_CENTER),
            arc([0.0, 0.0], rmin, 0.0, 270.0, LAYER_THIN),
            polyline(&[[-l / 2.0, half], [l / 2.0, half], [l / 2.0, -half], [-l / 2.0, -half]], true, LAYER_MAIN),
            line([-l / 2.0, ri], [l / 2.0, ri], LAYER_MAIN),
            line([-l / 2.0, -ri], [l / 2.0, -ri], LAYER_MAIN),
        ]
    }

    #[test]
    fn template_1097_specimen_b8_h7_l100() {
        // 源模板 specimen 就是 b=8×h=7×L=100；但 L=100 对 b=8 违反标准注③ `L<10b`
        //（模板自身如此），所以逐图元比对直接走视图函数，公共入口的该校验另行单测。
        let row_a = row_1097(KeyType::A, 8.0).unwrap();
        let row_b = row_1097(KeyType::B, 8.0).unwrap();
        assert_entities(
            &main_1097(KeyType::A, row_a, 100.0),
            &exp_1097_main_a(),
            "key_1097_a 主视图（specimen L=100）",
        );
        assert_entities(
            &top_1097(KeyType::A, row_a, 100.0),
            &exp_1097_top_a(),
            "key_1097_a 俯视图（specimen L=100）",
        );
        assert_entities(
            &main_1097(KeyType::B, row_b, 100.0),
            &exp_1097_main_b(),
            "key_1097_b 主视图（specimen L=100）",
        );
        assert_entities(
            &top_1097(KeyType::B, row_b, 100.0),
            &exp_1097_top_b(),
            "key_1097_b 俯视图（specimen L=100）",
        );
        // 公共入口：specimen 尺寸除 L 外都应能出（L=100 被注③挡住，见 length 测试）。
        let p = gen_all("key_1097_a", 8.0, 70.0, "top").unwrap();
        assert_eq!(p.meta.spec, "8×7×70");
    }

    /// 3/4 螺纹小径圈必须在 `2细线层`（用户口径①；源图在 4虚线层），缺口右下 0°→270°。
    #[test]
    fn thread_minor_arc_in_thin_layer() {
        for fam in ["key_1097_a", "key_1097_b"] {
            // 用小径圈无关的合法 L=70（specimen 的 L=100 违反注③；逐图元比对另测）。
            let p = gen_all(fam, 8.0, 70.0, "top").unwrap();
            let arcs: Vec<_> = p
                .entities
                .iter()
                .filter_map(|e| match e {
                    EntityType::Arc(a) => Some(a),
                    _ => None,
                })
                .collect();
            let expect_arcs = if fam == "key_1097_a" { 5 } else { 1 }; // B 型方头无端部半圆
            assert_eq!(arcs.len(), expect_arcs, "{fam} 俯视图弧段数（3/4 小径 + 端部半圆）");
            let thin: Vec<_> = arcs
                .iter()
                .filter(|a| a.common.layer == LAYER_THIN)
                .collect();
            assert_eq!(thin.len(), 1, "{fam}：只有 1 段弧在 2细线层");
            let a = thin[0];
            assert!(near(a.center.x, 0.0) && near(a.center.y, 0.0), "小径圈圆心在键中心");
            assert!(near(a.radius, 0.85 * 3.0 / 2.0), "小径半径 = 0.85·d0/2");
            assert!(near(a.start_angle.to_degrees(), 0.0) && near(a.end_angle.to_degrees(), 270.0), "3/4 圈缺口在右下（0→270°）");
            assert!(
                arcs.iter().all(|a| a.common.layer != LAYER_HIDDEN),
                "{fam}：4虚线层不应再有螺纹圈（源图的 4虚线已按用户口径移走）"
            );
        }
    }

    /// 20 档（1096）× 3 视图、14 档（1097）× 2 视图全遍历：能出图、图层合法、无标注、bbox 有效。
    #[test]
    fn all_specs_and_views_generate_clean() {
        for (fam, ty) in [
            ("key_1096_a", KeyType::A),
            ("key_1096_b", KeyType::B),
            ("key_1096_c", KeyType::C),
        ] {
            let table = table_1096(ty);
            assert_eq!(table.rows.len(), 20, "{fam} 应 20 档");
            for r in &table.rows {
                for view in family_views(fam) {
                    let p = gen_all(fam, r.b, 0.0, view)
                        .unwrap_or_else(|e| panic!("{fam} b={} {view}: {e}", r.b));
                    assert!(!p.entities.is_empty(), "{fam} b={} {view} 空", r.b);
                    assert_eq!(p.meta.spec, format!("{}×{}×{}", trim(r.b), trim(r.h), trim(key_1096_legal_default(r))));
                    check_clean(&p, &format!("{fam} b={} {view}", r.b));
                }
            }
        }
        for (fam, ty) in [("key_1097_a", KeyType::A), ("key_1097_b", KeyType::B)] {
            let table = table_1097(ty);
            assert_eq!(table.rows.len(), 14, "{fam} 应 14 档");
            for r in &table.rows {
                for view in family_views(fam) {
                    let p = gen_all(fam, r.b, 0.0, view)
                        .unwrap_or_else(|e| panic!("{fam} b={} {view}: {e}", r.b));
                    assert!(!p.entities.is_empty(), "{fam} b={} {view} 空", r.b);
                    assert_eq!(p.meta.spec, format!("{}×{}×{}", trim(r.b), trim(r.h), trim(key_1097_legal_default(r))));
                    check_clean(&p, &format!("{fam} b={} {view}", r.b));
                }
            }
        }
    }

    fn check_clean(p: &GenPart, what: &str) {
        for e in &p.entities {
            let lay = e.common().layer.as_str();
            assert!(OCSM_LAYERS.contains(&lay), "{what} 图层越界: {lay}");
            assert!(matches!(e.common().color, Color::ByLayer), "{what} 颜色非 ByLayer");
            assert!(!matches!(e, EntityType::Dimension(_)), "{what} 出现尺寸标注");
        }
        assert!(p.bbox[2] > p.bbox[0] && p.bbox[3] > p.bbox[1], "{what} bbox 无效");
    }

    /// L 口径：1096 系列 34 值 + `L<10b`；1097 从易紧通取得系列 + `L<10b` + 默认就近合法化。
    #[test]
    fn length_series_and_constraints() {
        // 1096
        assert_eq!(KEY_1096_L_SERIES.len(), 34, "1096 L 系列 34 值");
        assert_eq!(KEY_1096_L_SERIES[0], 6.0);
        assert_eq!(KEY_1096_L_SERIES[33], 400.0);
        assert_eq!(key_1096_l_allowed(2.0), vec![6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0]);
        assert_eq!(key_1096_l_range(2.0), Some((6.0, 18.0)));
        assert!(check_length_1096(2.0, 6.0).is_ok());
        assert!(check_length_1096(2.0, 18.0).is_ok());
        assert!(check_length_1096(2.0, 20.0).is_err(), "b=2 时 L=20 违反 L<10b");
        assert!(check_length_1096(2.0, 7.0).is_err(), "7 不在 L 系列");
        assert!(check_length_1096(50.0, 400.0).is_ok());
        assert!(check_length_1096(50.0, 450.0).is_err());
        assert!(check_length_1096(9.0, 6.0).is_err(), "b=9 不在表内");
        assert!(key_1096_default_corrections().is_empty(), "1096 默认 L 应全部合法");
        for r in &table_1096(KeyType::A).rows {
            let def = key_1096_legal_default(r);
            assert!(check_length_1096(r.b, def).is_ok(), "b={} 默认 L={} 应合法", r.b, def);
        }
        // 1097：两表系列必须一致，且与易紧通 info_42177 长度行对齐。
        assert_eq!(key_1097_l_series(), table_1097(KeyType::B).l_series.as_slice());
        assert_eq!(key_1097_l_series().len(), 26, "含 R20 延伸 450 共 26 值");
        assert_eq!(key_1097_l_series()[0], 25.0);
        assert_eq!(key_1097_l_series()[24], 400.0);
        assert_eq!(key_1097_l_series()[25], 450.0);
        assert_eq!(
            key_1097_l_allowed(8.0),
            vec![25.0, 28.0, 32.0, 36.0, 40.0, 45.0, 50.0, 56.0, 63.0, 70.0]
        );
        assert_eq!(key_1097_l_range(8.0), Some((25.0, 70.0)));
        assert_eq!(key_1097_l_range(45.0), Some((25.0, 400.0)));
        assert!(check_length_1097(8.0, 70.0).is_ok());
        assert!(check_length_1097(8.0, 100.0).is_err(), "b=8 时 L=100 违反 L<10b");
        assert!(check_length_1097(8.0, 22.0).is_err(), "22 不在 1097 L 系列");
        assert!(check_length_1097(45.0, 400.0).is_ok());
        assert!(check_length_1097(45.0, 450.0).is_err(), "450 不满足 L<10b");
        let corrections = key_1097_default_corrections();
        assert_eq!(corrections, vec![(8.0, 100.0, 70.0), (10.0, 100.0, 90.0)], "b=8/10 默认就近化");
        // 传 L 要真正进几何：L=70 的 1097A 主视图左端外形到 x=-35。
        let p = gen_all("key_1097_a", 8.0, 70.0, "main").unwrap();
        assert_eq!(p.meta.spec, "8×7×70");
        assert!(
            p.entities.iter().any(|e| matches!(e, EntityType::Line(l)
                if near(l.start.x, -35.0) && near(l.start.y, 6.75) && near(l.end.x, -26.0))),
            "L=70 的倒角棱线应止于 x=-35"
        );
        let p = gen_all("key_1097_b", 8.0, 0.0, "main").unwrap();
        assert_eq!(p.meta.spec, "8×7×70", "省略 L 时 b=8 默认应为合法值 70");
    }

    /// 目录集成：五族登记、kind=key、型别下拉数据、视图/规格数、树上挂位。
    #[test]
    fn catalog_registration() {
        let cat: serde_json::Value = serde_json::from_str(&crate::partgen::catalog_json()).unwrap();
        let fams = &cat["families"];
        for (fam, n_sizes, n_views, n_types) in [
            ("key_1096_a", 20, 3, 3),
            ("key_1096_b", 20, 3, 3),
            ("key_1096_c", 20, 3, 3),
            ("key_1097_a", 14, 2, 2),
            ("key_1097_b", 14, 2, 2),
        ] {
            let f = &fams[fam];
            assert!(!f.is_null(), "目录缺 {fam}");
            assert_eq!(f["kind"], "key", "{fam} kind");
            assert_eq!(f["implemented"], true);
            assert_eq!(f["views"].as_array().unwrap().len(), n_views, "{fam} 视图数");
            assert_eq!(f["sizes"].as_array().unwrap().len(), n_sizes, "{fam} 规格数");
            assert_eq!(f["type_group"].as_array().unwrap().len(), n_types, "{fam} 型别下拉项");
            assert_eq!(crate::partgen::family_kind(fam), "key");
        }
        assert_eq!(fams["key_1096_a"]["views"][0]["id"], "main");
        assert_eq!(fams["key_1096_a"]["views"][2]["id"], "section");
        assert_eq!(fams["key_1097_a"]["views"][1]["id"], "top");
        assert_eq!(fams["key_1096_a"]["type_group"][1]["id"], "key_1096_b");
        // 1097 下拉：b=8 的首个 lengths 是合法默认 70；全部 < 10b。
        let s8 = &fams["key_1097_a"]["sizes"][0];
        assert_eq!(s8["d"], 8.0);
        assert_eq!(s8["lengths"][0], 70.0);
        assert_eq!(s8["l_min"], 25.0);
        assert_eq!(s8["l_max"], 70.0);
        for fam in ["key_1096_a", "key_1096_b", "key_1096_c", "key_1097_a", "key_1097_b"] {
            for s in fams[fam]["sizes"].as_array().unwrap() {
                let d = s["d"].as_f64().unwrap();
                for l in s["lengths"].as_array().unwrap() {
                    assert!(l.as_f64().unwrap() < 10.0 * d, "{fam} b={d} 含非法 L {l}");
                }
            }
        }
        // 树：五族都挂在「键/平键」下。
        fn walk(node: &serde_json::Value, out: &mut Vec<String>) {
            if let Some(arr) = node.as_array() {
                for n in arr {
                    walk(n, out);
                }
            }
            if let Some(f) = node.get("family").and_then(|x| x.as_str()) {
                out.push(f.to_string());
            }
            if let Some(k) = node.get("children") {
                walk(k, out);
            }
        }
        let mut on_tree = Vec::new();
        walk(&cat["tree"], &mut on_tree);
        for fam in ["key_1096_a", "key_1096_b", "key_1096_c", "key_1097_a", "key_1097_b"] {
            assert!(on_tree.contains(&fam.to_string()), "{fam} 没挂到零件树");
        }
        assert!(cat.to_string().contains("平键"), "树路径应含「平键」");
    }

    /// 视图/参数校验与错误路径。
    #[test]
    fn view_and_param_errors() {
        assert_eq!(family_views("key_1096_a"), vec!["main", "top", "section"]);
        assert_eq!(family_views("key_1097_a"), vec!["main", "top"]);
        assert!(family_views("key_9999").is_empty());
        assert!(gen_all("key_1096_a", 2.0, 6.0, "end").unwrap_err().contains("不提供视图"));
        assert!(gen_all("key_1097_a", 8.0, 100.0, "section").unwrap_err().contains("不提供视图"));
        assert!(gen_all("key_1096_a", 7.0, 6.0, "main").unwrap_err().contains("没有 b=7"));
        assert!(gen_all("key_1097_a", 9.0, 70.0, "main").unwrap_err().contains("没有 b=9"));
        assert!(gen_all("key_1097_a", 8.0, 100.0, "main").is_err());
        assert!(gen_all("key_1096_a", 4.0, 8.0, "main").is_ok());
        // 1096 A 型 h 误印：库内取标准值 20/22/25/28，印刷值仅留档。
        let a = table_1096(KeyType::A);
        let last4: Vec<_> = a.rows[a.rows.len() - 4..].iter().collect();
        assert_eq!(last4.iter().map(|r| r.h).collect::<Vec<_>>(), vec![20.0, 22.0, 25.0, 28.0]);
        assert_eq!(last4.iter().map(|r| r.h_printed).collect::<Vec<_>>(), vec![Some(32.0), Some(20.0), Some(22.0), Some(25.0)]);
        assert_eq!(table_1096(KeyType::B).rows[16].h_printed, None);
        assert!(KEY_1096_CHAMFER_RANGES.len() == 7 && type_label(KeyType::C).contains("单圆头"));
    }

    /// SVG 冒烟：样本规格可渲染；剖面线间距按用户口径 3.0mm。
    #[test]
    fn svg_smoke() {
        for (fam, d, l, views) in [
            ("key_1096_a", 4.0, 8.0, vec!["main", "top", "section"]),
            ("key_1096_c", 2.0, 6.0, vec!["main", "top", "section"]),
            ("key_1097_a", 8.0, 70.0, vec!["main", "top"]),
            ("key_1097_b", 45.0, 400.0, vec!["main", "top"]),
        ] {
            for view in views {
                let p = gen_all(fam, d, l, view).unwrap();
                let svg = part_svg(&p, 1200.0, 800.0);
                assert!(svg.contains("<svg"), "{fam}/{view} SVG 空");
                assert!(!svg.contains("error"), "{fam}/{view} SVG 报错");
            }
        }
        for h in gen_all("key_1096_a", 2.0, 6.0, "section").unwrap().entities.iter().filter_map(|e| match e {
            EntityType::Hatch(h) => Some(h),
            _ => None,
        }) {
            assert!(near(hatch_perpendicular_spacing(h.pattern_scale), 3.0), "1096 剖面线间距应为 3.0mm");
        }
        for h in gen_all("key_1097_a", 8.0, 70.0, "main").unwrap().entities.iter().filter_map(|e| match e {
            EntityType::Hatch(h) => Some(h),
            _ => None,
        }) {
            assert!(near(hatch_perpendicular_spacing(h.pattern_scale), 3.0), "1097 剖面线间距应为 3.0mm");
        }
    }
}
