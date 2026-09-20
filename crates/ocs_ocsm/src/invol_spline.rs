//! **渐开线花键**（GB/T 3478.1-2008 / DIN 5480-1:2015）—— 核心几何 + 预设表 + 视图图元。
//!
//! 本轮范围：**外花键**（端视图真实渐开线齿廓 / 侧视轮廓 / 收尾弧）+ 预设表 + 数值接口。
//! 内花键只留 [`InvolParams::internal_tip_dia`] 数值接口（不出几何）；ANSI 预设未做。
//! 注册点（`lib.rs` / `guide_server.rs` / 两个 GUI / 手册）由后续接线方另行接入，本文件只交付模块本体。
//!
//! # 一、口径（权威公式）
//!
//! ## GB/T 3478.1-2008（默认预设）
//!
//! 基本齿廓见图 2（p07/p08），外花键大径基本尺寸系列见表 4~表 6，尺寸计算公式见表 3：
//!
//! | 基本齿廓 | α | ha* | hf* | ρf* | cF* | 外花键大径 Dee | 外花键小径 Dfe | 内花键大径 Dii |
//! |---|---|---|---|---|---|---|---|---|
//! | 30°平齿根 | 30° | 0.5 | 0.75 | 0.2 | 0.1 | m(z+1) | m(z−1.5) | m(z+1.5) |
//! | 30°圆齿根 | 30° | 0.5 | 0.9 | 0.4 | 0.1 | m(z+1) | m(z−1.8) | m(z+1.8) |
//! | 37.5°圆齿根 | 37.5° | 0.45 | 0.7 | 0.3 | 0.1 | m(z+0.9) | m(z−1.4) | m(z+1.4) |
//! | 45°圆齿根 | 45° | 0.4 | 0.6 | 0.25 | 0.1 | m(z+0.8) | m(z−1.2) | m(z+1.2) |
//!
//! 任务原文只给了 GB 的统一 `ha*=0.5`；为让 `da` 与表 4~表 6 的权威公式
//! `m(z+1) / m(z+0.9) / m(z+0.8)` 逐项一致，37.5°/45° 预设的 ha* 取等价值 **0.45/0.4**
//! （即 `da = d + 2·ha*·m`）。**直径一律按标准公式算**，不用图上标注的齿顶高反推。
//!
//! ## DIN 5480-1:2015
//!
//! `α=30°`、`ha*=0.45`、`hf*=0.55`（DIN 5480-1:2015 条 5.1：**齿侧对中（Flank centering）
//! 时 h_fP = 0.55·m 是基准**；滚刀加工 −0.1m、插齿刀 ±0.2m 属刀具相关修正，需要时用
//! [`InvolParams::with_coeffs`] 覆盖）、
//! `ρf*=0.16`（DIN 5480-2 表 1：m=0.5~10 全为 0.16m）、`c*=hf*−ha*=0.10`；
//! `d_a1 = m·z + 2x·m + 0.9m`、`d_f1 = m·z + 2x·m − 2·hf*·m`、
//! `s1 = m·π/2 + 2x·m·tanα`；变位范围 `x·m ∈ [−0.05m, +0.45m]`（即 `x ∈ [−0.05, 0.45]`，在
//! [`InvolParams::validate`] 里校验）。
//!
//! `hf*=0.55` 与 DIN 5480-2 名义表逐行吻合：m=0.5 的表里 x·m=0.225（x=0.45）时，
//! z=34 → `d_f1=17.00+0.45−0.55=16.90`、z=10 → `5.00+0.45−0.55=4.90`（`d_a1` 亦为 17.90 / 5.90，
//! 均见表 `DIN5480-2_名义表.csv` p.11）；旧值 hf*=0.60 会差 0.1m（16.85），故按任务改用基准 0.55。
//!
//! ## DIN 5480-2 名义表（`d_B` 查表，本轮主路径）
//!
//! 数据入库为 `assets/din5480_2_nominal.csv`（618 行；源 665 行中剔除 p35/m=5 整页 47 行，
//! 原因与 m=1.5 缺失见 `assets/din5480_2_notes.md`）。按基准直径 [`lookup_by_d_b`]：
//! 同一 `d_B` 可能有多个 `z/x₁` 变体（表格事实，返回多行）；找不到时列附近候选；缺失档位
//! （m=1.5 空表框 / m=5 已剔除）明确报「该档位数据缺失」。
//!
//! `x_from_d_b(d_B, m, z) = (d_B − m(z + 1.1)) / (2m)` 是**从 OCR 名义表反推并经全表校验**
//! 的关系（等价 `d_B = d + 1.1m + 2x·m`、`z_B = z + 1.1 + 2x`；618 行逐行残差 0），
//! **不是标准原文公式**；[`resolve_din_by_d_b`] 用它把 `DB`+（M 或 Z）补成全参数，
//! 并在 `DB+M+Z` 三参齐给时与表值互相印证。
//!
//! ⚠ `d_B` 只参与**名义值选择/显示与一致性校验**，不改变几何公式（几何仍由 m/z/x 决定）；
//! [`InvolParams::d_b`] 存查表/公式得到的基准直径，[`InvolParams::d_b_estimate`] 是旧的
//! `m(z+2x)` 名义估算（保留兼容，非 `d_B` 定义式）。
//!
//! # 二、几何
//!
//! * 分度圆 `d=mz`、基圆 `db=d·cosα`、齿厚 `s=mπ/2+2xm·tanα`、齿厚半角
//!   `ψ(R)=s/d+invα−invα_R`（`α_R=acos(db/R)`，`R<db` 时夹在 db）—— 与 `gear.rs` 同口径（直齿）。
//! * 端视图：每齿一条**真实渐开线**（按 [`INVOLUTE_SEGMENTS`] 折线采样），**齿槽心线 = 0°+k·360°/z**
//!   （用户定案，与 `spline.rs` 矩形花键同相）；齿顶弧按大径、齿根弧按小径；基圆高于小径时
//!   用径向直线从齿根圆接到基圆。齿根过渡圆角 `ρf` 本轮只导出数值、不画圆角弧。
//! * 侧视：矩形 L×大径 + 小径线；小径线图层由调用方定（常规 `2细线层` / 剖视 `1轮廓实线层`），
//!   模块只产出带图层标记的 `EntityType` 列表。
//! * 收尾（口径同 `spline.rs`）：`R=de/2`，圆心 `(L, ±(小径/2+R))`，与小径相切、终点交大径，
//!   轴向长 `l=√(h(2R−h))`、`h=(大径−小径)/2`；`de` 由调用方传入，须大于大径。
//!
//! # 三、相位/中心线
//!
//! * 齿槽心线 `0°+k·360°/z`，齿心线 `180°/z+k·360°/z`；
//! * 十字中心线长度 = `大径+6`（与 `spline.rs` 正视图一致）。

#![allow(dead_code)] // 注册/接线由后续改动负责，本轮只交付模块

use std::sync::OnceLock;

use ocs_plugin_api::host::acadrust::entities::EntityType;

use crate::partgen_kit::{arc, line, trim, LAYER_CENTER, LAYER_MAIN, LAYER_THIN};

/// GB/T 3478.1-2008 标准号（默认预设）。
pub const GB_CODE: &str = "GB/T 3478.1-2008";
/// DIN 5480-1:2015 标准号。
pub const DIN_CODE: &str = "DIN 5480-1:2015";

/// 端视图每条渐开线齿廓的折线段数（真实渐开线采样；折线保形、实体数可控）。
pub const INVOLUTE_SEGMENTS: usize = 12;

// ─────────────────────────── 标准 / 预设 ───────────────────────────

/// 花键标准（ANSI 预留，后续加 `Ansi` 变体与预设表即可）。
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplineStd {
    /// GB/T 3478.1-2008（默认预设）。
    GB,
    /// DIN 5480-1:2015。
    DIN,
}

impl SplineStd {
    /// 标准号。
    pub fn code(self) -> &'static str {
        match self {
            Self::GB => GB_CODE,
            Self::DIN => DIN_CODE,
        }
    }

    /// 短标签（图纸标注/文件名用）。
    pub fn label(self) -> &'static str {
        match self {
            Self::GB => "GB",
            Self::DIN => "DIN",
        }
    }

    /// 该标准下的预设列表。
    pub fn presets(self) -> &'static [InvolPreset] {
        match self {
            Self::GB => GB_PRESETS,
            Self::DIN => DIN_PRESETS,
        }
    }
}

/// 一条基本齿廓预设（可被 `x`/系数逐个覆盖）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InvolPreset {
    /// 所属标准。
    pub std: SplineStd,
    /// 齿廓名（GB：`30平齿根`/`30圆齿根`/`37.5圆齿根`/`45圆齿根`；DIN：`DIN30`）。
    pub profile: &'static str,
    /// 压力角 α（度）。
    pub alpha_deg: f64,
    /// 齿顶高系数 ha*。
    pub ha_star: f64,
    /// 齿根高系数 hf*。
    pub hf_star: f64,
    /// 齿根圆角系数 ρf*。
    pub rho_star: f64,
    /// 齿形裕度系数 cF*（GB 全为 0.1；DIN 取基础齿廓顶隙 c = h_fP*−h_aP* = 0.10）。
    pub c_f_star: f64,
}

/// **GB/T 3478.1-2008 预设表**（图 2 基本齿廓 + 表 4~表 6 大径公式）。
pub const GB_PRESETS: &[InvolPreset] = &[
    InvolPreset {
        std: SplineStd::GB,
        profile: "30平齿根",
        alpha_deg: 30.0,
        ha_star: 0.5,
        hf_star: 0.75,
        rho_star: 0.2,
        c_f_star: 0.1,
    },
    InvolPreset {
        std: SplineStd::GB,
        profile: "30圆齿根",
        alpha_deg: 30.0,
        ha_star: 0.5,
        hf_star: 0.9,
        rho_star: 0.4,
        c_f_star: 0.1,
    },
    InvolPreset {
        std: SplineStd::GB,
        profile: "37.5圆齿根",
        alpha_deg: 37.5,
        ha_star: 0.45,
        hf_star: 0.7,
        rho_star: 0.3,
        c_f_star: 0.1,
    },
    InvolPreset {
        std: SplineStd::GB,
        profile: "45圆齿根",
        alpha_deg: 45.0,
        ha_star: 0.4,
        hf_star: 0.6,
        rho_star: 0.25,
        c_f_star: 0.1,
    },
];

/// **DIN 5480-1:2015 预设表**（齿侧对中基准 `h_fP*=0.55`；`c_f_star` 见类型注释）。
pub const DIN_PRESETS: &[InvolPreset] = &[InvolPreset {
    std: SplineStd::DIN,
    profile: "DIN30",
    alpha_deg: 30.0,
    ha_star: 0.45,
    hf_star: 0.55,
    rho_star: 0.16,
    c_f_star: 0.10,
}];

/// 齿廓名规范化（去空白/度符号、转小写）：`"30° 圆齿根"` 与 `"30圆齿根"` 等价。
fn norm_profile(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '°')
        .collect::<String>()
        .to_lowercase()
}

/// 按标准 + 齿廓名查预设（找不到时把可用预设列出来）。
pub fn preset(std: SplineStd, profile: &str) -> Result<&'static InvolPreset, String> {
    let key = norm_profile(profile);
    std.presets()
        .iter()
        .find(|p| norm_profile(p.profile) == key)
        .ok_or_else(|| {
            let list = std
                .presets()
                .iter()
                .map(|p| p.profile)
                .collect::<Vec<_>>()
                .join("、");
            format!("{} 没有齿廓预设「{profile}」；可用：{list}", std.code())
        })
}

// ─────────────────────────── 预设代号（CLI / DSL / GUI） ───────────────────────────

/// 预设 → CLI/DSL 代号：**GB 默认 `GB30R`**，另 `GB30P`/`GB375R`/`GB45R`/`DIN30`。
pub fn preset_code(std: SplineStd, profile: &str) -> Option<&'static str> {
    let key = norm_profile(profile);
    std.presets().iter().find(|p| norm_profile(p.profile) == key).map(|p| {
        match (std, p.profile) {
            (SplineStd::GB, "30平齿根") => "GB30P",
            (SplineStd::GB, "30圆齿根") => "GB30R",
            (SplineStd::GB, "37.5圆齿根") => "GB375R",
            (SplineStd::GB, "45圆齿根") => "GB45R",
            _ => "DIN30",
        }
    })
}

/// 预设代号 → `(标准, 齿廓名)`；收 `GB30P`/`GB30R`/`GB375R`/`GB45R`/`DIN30`，
/// 也收 GB + 中文齿廓名（`GB30圆齿根`）；`GB` / `DIN` 裸写取各自默认齿廓
/// （GB → `30圆齿根`，DIN → `DIN30`）。
pub fn parse_preset_token(token: &str) -> Option<(SplineStd, &'static str)> {
    let t = token.trim().replace([' ', '\u{3000}', '°'], "");
    if t.is_empty() {
        return None;
    }
    let upper = t.to_ascii_uppercase();
    if let Some(rest) = upper.strip_prefix("DIN") {
        return match rest {
            "" | "30" | "30R" | "DIN30" => Some((SplineStd::DIN, "DIN30")),
            _ => None,
        };
    }
    let rest = upper.strip_prefix("GB").unwrap_or(upper.as_str());
    let profile = match rest {
        "" => "30圆齿根", // GB 默认
        "30P" | "30平" | "30平齿根" | "平齿根" => "30平齿根",
        "30R" | "30圆" | "30圆齿根" | "圆齿根" => "30圆齿根",
        "37.5R" | "375R" | "37.5圆" | "37.5圆齿根" | "375圆齿根" => "37.5圆齿根",
        "45R" | "45圆" | "45圆齿根" => "45圆齿根",
        _ => return None,
    };
    Some((SplineStd::GB, profile))
}

// ──────────────── DIN 5480-2 名义表（`d_B` 查表；数据 = assets/din5480_2_nominal.csv） ────────────────

/// 入库名义表（`crates/ocs_ocsm/assets/din5480_2_nominal.csv`，**618 行**）。
///
/// 来源 `DIN5480-2_名义表_续2_merged.csv`（665 行）剔除 p35（m=5）整页 47 行
/// （源图数据区渲染缺陷、双源 OCR 互不一致）；`flags/source` 列原样保留便于追溯，
/// 运行时不展示。剔除原因与 m=1.5（p23/p24 空表框）缺失说明见 `assets/din5480_2_notes.md`。
const DIN5480_2_CSV: &str = include_str!("../assets/din5480_2_nominal.csv");

/// 名义表一行（只保留主路径需要的列；`flags/source` 留作追溯）。
#[derive(Debug, Clone, PartialEq)]
pub struct Din5480Row {
    /// 源 PDF 页码（DIN 5480-2:2015-03）。
    pub page: u16,
    /// 模数 m。
    pub m: f64,
    /// 页内表号（`Table_N`）。
    pub table_no: u32,
    /// **基准直径 `d_B`**（本表主键维度）。
    pub d_b: f64,
    /// 齿数 z。
    pub z: u32,
    /// 分度圆直径 `d = m·z`。
    pub d: f64,
    /// **基圆直径**（CSV 的 `d_b` 列 = `d·cos30°`；与基准直径 `d_B` 同名不同义）。
    pub base_dia: f64,
    /// 变位量 `x₁·m`（CSV 的 `x1_m` 列）。
    pub x_m: f64,
    /// 变位系数 `x₁ = (x₁·m)/m`（由表列派生）。
    pub x: f64,
    /// OCR 质量标记（原样保留，运行时不展示）。
    pub flags: String,
    /// OCR 来源（`A+B` / `A续2+B续2` 等；原样保留，运行时不展示）。
    pub source: String,
}

/// 查表数值容差：`d_B` 表值是整数，允许输入 1e-3 误差。
const DIN_D_B_TOL: f64 = 1e-3;
/// 查表模数容差。
const DIN_M_TOL: f64 = 1e-9;
/// 表值 `x₁·m` 最多 4 位小数（x 往返误差 ≤ 5e-5/m）；1e-4 足以判同。
const DIN_X_TOL: f64 = 1e-4;

/// 已知**缺失/剔除**的模数档及原因（有据可查的两档）。
pub const DIN_MISSING_MODULES: &[(f64, &str)] = &[
    (
        1.5,
        "该档位数据缺失（源图空表框）：p23/p24 的 m=1.5 名义表在 doc88 为空表框，待另找来源",
    ),
    (
        5.0,
        "该档位数据缺失（已整页剔除）：p35 的 m=5 源图数据区渲染缺陷、双源 OCR 互不一致",
    ),
];

static DIN5480_TABLE: OnceLock<Vec<Din5480Row>> = OnceLock::new();

/// 入库名义表（懒加载；解析不了会 panic —— 数据随二进制编译，属构建错误）。
pub fn din5480_rows() -> &'static [Din5480Row] {
    DIN5480_TABLE.get_or_init(|| {
        parse_din5480_csv(DIN5480_2_CSV)
            .unwrap_or_else(|e| panic!("DIN 5480-2 名义表入库数据损坏：{e}"))
    })
}

/// 已入库模数档（升序；缺档见 [`DIN_MISSING_MODULES`]）。
pub fn din5480_modules() -> Vec<f64> {
    let mut v: Vec<f64> = din5480_rows().iter().map(|r| r.m).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v.dedup_by(|a, b| (*a - *b).abs() < DIN_M_TOL);
    v
}

/// 拆分一行 CSV（本表十进制逗号字段被双引号包住；无转义引号）。
fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in line.chars() {
        match ch {
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    out.push(cur);
    out
}

/// 十进制字段（CSV 用逗号作小数点）。
fn csv_decimal(s: &str) -> Option<f64> {
    let t = s.trim().replace(',', ".");
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// 解析入库 CSV（跳过 `#` 注释与表头；缺列/坏值 → 带行号报错）。
fn parse_din5480_csv(text: &str) -> Result<Vec<Din5480Row>, String> {
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let f = split_csv_line(line);
        if f.first().map(|s| s.trim()) == Some("page") {
            continue; // 表头
        }
        if f.len() < 19 {
            return Err(format!("第 {n} 行只有 {} 列（应有 19 列）", f.len()));
        }
        let page = f[0]
            .trim()
            .parse::<u16>()
            .map_err(|_| format!("第 {n} 行 page「{}」非法", f[0]))?;
        let m = csv_decimal(&f[1]).ok_or_else(|| format!("第 {n} 行 m「{}」非法", f[1]))?;
        let table_no = f[2]
            .trim()
            .parse::<u32>()
            .map_err(|_| format!("第 {n} 行 table_no「{}」非法", f[2]))?;
        let d_b = csv_decimal(&f[3]).ok_or_else(|| format!("第 {n} 行 d_B「{}」非法", f[3]))?;
        let z = f[4]
            .trim()
            .parse::<u32>()
            .map_err(|_| format!("第 {n} 行 z「{}」非法", f[4]))?;
        let d = csv_decimal(&f[5]).ok_or_else(|| format!("第 {n} 行 d「{}」非法", f[5]))?;
        let base_dia =
            csv_decimal(&f[6]).ok_or_else(|| format!("第 {n} 行 d_b「{}」非法", f[6]))?;
        let x_m = csv_decimal(&f[7]).ok_or_else(|| format!("第 {n} 行 x1_m「{}」非法", f[7]))?;
        if m <= 0.0 {
            return Err(format!("第 {n} 行 m={m} 非正"));
        }
        rows.push(Din5480Row {
            page,
            m,
            table_no,
            d_b,
            z,
            d,
            base_dia,
            x_m,
            x: x_m / m,
            flags: f[17].trim().to_string(),
            source: f[18].trim().to_string(),
        });
    }
    Ok(rows)
}

/// 命中 `(d_B, m?)` 的原始行（不产生错误文案；[`lookup_by_d_b`] 的错误路径也用它）。
fn din5480_match(d_b: f64, m: Option<f64>) -> Vec<Din5480Row> {
    din5480_rows()
        .iter()
        .filter(|r| (r.d_b - d_b).abs() < DIN_D_B_TOL)
        .filter(|r| m.is_none_or(|m| (r.m - m).abs() < DIN_M_TOL))
        .cloned()
        .collect()
}

/// **DIN 5480-2 名义表查表**：按基准直径 `d_B`（可再限定模数 `m`）返回名义行。
///
/// * 命中可能**多行** —— 同一 `d_B` 有多个 `z/x₁` 变体是表格事实（如 p21 m=1.25、p25 m=1.75）；
/// * 找不到时列出该 `d_B` 附近候选（同 m 优先；缺失档退全表）；
/// * `m=1.5`（p23/p24 源图空表框）与 `m=5`（p35 已剔除）明确报「该档位数据缺失」。
pub fn lookup_by_d_b(d_b: f64, m: Option<f64>) -> Result<Vec<Din5480Row>, String> {
    if !(d_b.is_finite() && d_b > 0.0) {
        return Err(format!("DIN 5480-2 查表：d_B={} 必须是正数。", trim(d_b)));
    }
    if let Some(m) = m {
        if !(m.is_finite() && m > 0.0) {
            return Err(format!("DIN 5480-2 查表：m={} 必须是正数。", trim(m)));
        }
    }
    let mut hits = din5480_match(d_b, m);
    hits.sort_by(|a, b| a.z.cmp(&b.z).then(a.m.partial_cmp(&b.m).unwrap()));
    if hits.is_empty() {
        Err(din5480_miss_message(d_b, m))
    } else {
        Ok(hits)
    }
}

/// 未命中的错误文案：缺失档位说明 + 附近候选。
fn din5480_miss_message(d_b: f64, m: Option<f64>) -> String {
    let mut msg = format!("DIN 5480-2 查表：d_B={}", trim(d_b));
    if let Some(m) = m {
        msg.push_str(&format!("（m={}）", trim(m)));
    }
    msg.push_str(" 无命中");
    if let Some(m) = m {
        if let Some((_, why)) = DIN_MISSING_MODULES
            .iter()
            .find(|(mm, _)| (*mm - m).abs() < DIN_M_TOL)
        {
            msg.push_str(&format!("：{why}。"));
        } else if !din5480_modules()
            .iter()
            .any(|mm| (mm - m).abs() < DIN_M_TOL)
        {
            let list = din5480_modules()
                .iter()
                .map(|v| trim(*v))
                .collect::<Vec<_>>()
                .join("、");
            msg.push_str(&format!(
                "；表中没有 m={} 档（已入库档位：{}）。",
                trim(m),
                list
            ));
        } else {
            msg.push('。');
        }
    } else {
        msg.push('。');
    }
    // 附近候选：同 m（若给了 m）按 |Δd_B| 取最近的 5 个不同 d_B；缺失档退全表。
    let mut pool: Vec<&Din5480Row> = din5480_rows()
        .iter()
        .filter(|r| m.is_none_or(|m| (r.m - m).abs() < DIN_M_TOL))
        .collect();
    if pool.is_empty() {
        pool = din5480_rows().iter().collect();
    }
    pool.sort_by(|a, b| {
        (a.d_b - d_b)
            .abs()
            .partial_cmp(&(b.d_b - d_b).abs())
            .unwrap()
    });
    let mut seen: Vec<f64> = Vec::new();
    let mut cands: Vec<String> = Vec::new();
    for r in pool {
        if seen.iter().any(|v| (*v - r.d_b).abs() < DIN_D_B_TOL) {
            continue;
        }
        seen.push(r.d_b);
        cands.push(format!("d_B={}（m={}）", trim(r.d_b), trim(r.m)));
        if cands.len() >= 5 {
            break;
        }
    }
    if !cands.is_empty() {
        msg.push_str(&format!("附近候选：{}。", cands.join("、")));
    }
    msg
}

/// DIN 5480-2 基准直径反解变位系数：
/// `x = (d_B − m(z + 1.1)) / (2m)`，等价 `d_B = d + 1.1m + 2x·m`（`z_B = z + 1.1 + 2x`）。
///
/// **口径**：这是从 OCR 名义表反推并经**全表校验**的关系 —— 已解析 618 行逐行
/// `|x₁·m − (d_B − m(z+1.1))/2| = 0`（被剔除的 p35 除外）；**不是标准原文公式**，
/// 引用时以 DIN 5480-2 表值与检验表为准。
pub fn x_from_d_b(d_b: f64, m: f64, z: u32) -> f64 {
    (d_b - m * (z as f64 + 1.1)) / (2.0 * m)
}

/// 由 `m/z/x` 正算基准直径 `d_B = m(z + 1.1 + 2x)`（[`x_from_d_b`] 的逆）。
pub fn d_b_from_x(m: f64, z: u32, x: f64) -> f64 {
    m * (z as f64 + 1.1 + 2.0 * x)
}

/// `d_B` 补全的来源（派生值显示用）。
#[derive(Debug, Clone, PartialEq)]
pub enum D_bOrigin {
    /// 查表命中该行。
    Table(Din5480Row),
    /// 公式解出（未命中表）。
    Formula,
}

impl D_bOrigin {
    /// 给用户看的来源说明（GUI/命令行派生值文本用）。
    pub fn note(&self) -> String {
        match self {
            Self::Table(r) => format!("查表命中 p{} m={}", r.page, trim(r.m)),
            Self::Formula => "由公式解出，未命中表".to_string(),
        }
    }
}

/// 用 `d_B` 补全/校验 DIN 参数（`m/z/x` 缺哪项补哪项；主路径入口）。
///
/// 组合（`GB` 无 `d_B`，调用方先自行拒绝）：
/// * `d_B + m + z`：公式解 `x`；并核对表中同 `(m, z)` 的 `d_B` 是否含给定值（不一致报错）；
/// * `d_B + m`：查表补 `z`（同 `d_B` 有多 `z` 变体时报候选要 `Z`）；
/// * `d_B + z`：查表补 `m`（多 `m` 变体时报候选）；
/// * 只给 `d_B`：查表补 `m/z`（多命中报行列表）。
///
/// 命中表行时 `x` 取表值（精度优先），来源 [`D_bOrigin::Table`]；否则来源
/// [`D_bOrigin::Formula`]（`d_B` 取输入值）。
pub fn resolve_din_by_d_b(
    d_b: f64,
    m: Option<f64>,
    z: Option<u32>,
    x: Option<f64>,
) -> Result<(InvolParams, D_bOrigin), String> {
    if !(d_b.is_finite() && d_b > 0.0) {
        return Err(format!("DIN 5480：d_B={} 必须是正数。", trim(d_b)));
    }
    if let Some(m) = m {
        if !(m.is_finite() && m > 0.0) {
            return Err(format!("DIN 5480：m={} 必须是正数。", trim(m)));
        }
    }
    match (m, z) {
        (Some(m), Some(z)) => resolve_with_m_z(d_b, m, z, x),
        (Some(m), None) => {
            let mut hits = din5480_match(d_b, Some(m));
            if hits.is_empty() {
                return Err(lookup_by_d_b(d_b, Some(m)).unwrap_err());
            }
            let mut zs: Vec<u32> = hits.iter().map(|r| r.z).collect();
            zs.sort_unstable();
            zs.dedup();
            if zs.len() > 1 {
                return Err(format!(
                    "DIN 5480：d_B={}、m={} 有多个 z 变体：{}；请再给 Z（同一 d_B 多个 z/x₁ 是表格事实）。",
                    trim(d_b),
                    trim(m),
                    join_rows(&hits)
                ));
            }
            hits.sort_by_key(|r| r.z);
            row_to_params(hits.remove(0))
        }
        (None, Some(z)) => {
            let hits: Vec<Din5480Row> = din5480_match(d_b, None)
                .into_iter()
                .filter(|r| r.z == z)
                .collect();
            if hits.is_empty() {
                return Err(lookup_by_d_b(d_b, None).unwrap_err());
            }
            let mut ms: Vec<f64> = hits.iter().map(|r| r.m).collect();
            ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
            ms.dedup_by(|a, b| (*a - *b).abs() < DIN_M_TOL);
            if ms.len() > 1 {
                return Err(format!(
                    "DIN 5480：d_B={}、z={} 有多个 m 变体：{}；请再给 M。",
                    trim(d_b),
                    z,
                    join_rows(&hits)
                ));
            }
            row_to_params(hits[0].clone())
        }
        (None, None) => {
            let hits = din5480_match(d_b, None);
            if hits.is_empty() {
                return Err(lookup_by_d_b(d_b, None).unwrap_err());
            }
            if hits.len() > 1 {
                return Err(format!(
                    "DIN 5480：d_B={} 命中 {} 行（同一 d_B 多个 m/z/x₁ 变体）：{}；请再给 M 和/或 Z。",
                    trim(d_b),
                    hits.len(),
                    join_rows(&hits)
                ));
            }
            row_to_params(hits[0].clone())
        }
    }
}

/// 表行 → 参数（用表值 `x`，来源 [`D_bOrigin::Table`]）。
fn row_to_params(row: Din5480Row) -> Result<(InvolParams, D_bOrigin), String> {
    let p = InvolParams::din(row.m, row.z, row.x)
        .map_err(|e| {
            format!(
                "DIN 5480：查表行 p{} m={} z={} x={} 不自洽：{e}",
                row.page,
                trim(row.m),
                row.z,
                trim(row.x)
            )
        })?
        .with_d_b(row.d_b);
    Ok((p, D_bOrigin::Table(row)))
}

/// `d_B + m + z`：`x` 由公式解出；若表中 `(m, z)` 存在，则核对 `d_B` 是否一致。
fn resolve_with_m_z(
    d_b: f64,
    m: f64,
    z: u32,
    x: Option<f64>,
) -> Result<(InvolParams, D_bOrigin), String> {
    let x = match x {
        Some(x) => {
            let expected = d_b_from_x(m, z, x);
            if (expected - d_b).abs() > DIN_D_B_TOL {
                return Err(format!(
                    "DIN 5480：d_B={}、m={}、z={}、x={} 不自洽（由 x 正算 d_B={}）。",
                    trim(d_b),
                    trim(m),
                    z,
                    trim(x),
                    trim(expected)
                ));
            }
            x
        }
        None => x_from_d_b(d_b, m, z),
    };
    // 表内同 (m, z) 的 d_B 必须包含给定量 —— 否则是 d_B/m/z 组合不一致。
    let same_z: Vec<&Din5480Row> = din5480_rows()
        .iter()
        .filter(|r| (r.m - m).abs() < DIN_M_TOL && r.z == z)
        .collect();
    if !same_z.is_empty() && !same_z.iter().any(|r| (r.d_b - d_b).abs() < DIN_D_B_TOL) {
        let list = same_z
            .iter()
            .map(|r| format!("d_B={}（x={}）", trim(r.d_b), trim(r.x)))
            .collect::<Vec<_>>()
            .join("、");
        return Err(format!(
            "DIN 5480：d_B={}、m={}、z={} 组合不一致 —— 表中 m={}、z={} 为 {}；请核对 DB/M/Z，或去掉 DB 改走 M/Z/X。",
            trim(d_b),
            trim(m),
            z,
            trim(m),
            z,
            list
        ));
    }
    if !(-0.05 - 1e-9..=0.45 + 1e-9).contains(&x) {
        return Err(format!(
            "DIN 5480：由 d_B={}、m={}、z={} 解出 x={}，超出 x∈[−0.05, 0.45]（DIN 5480-1）。",
            trim(d_b),
            trim(m),
            z,
            trim(x)
        ));
    }
    let p = InvolParams::din(m, z, x)
        .map_err(|e| format!("DIN 5480：{e}"))?
        .with_d_b(d_b);
    // 命中同 (m, z, d_B) 的表行 → 用表值 x（精度优先）并标注来源。
    if let Some(row) = same_z
        .iter()
        .find(|r| (r.d_b - d_b).abs() < DIN_D_B_TOL && (r.x - x).abs() < DIN_X_TOL)
    {
        let p = p.with_x(row.x);
        p.validate().map_err(|e| format!("DIN 5480：{e}"))?;
        return Ok((p, D_bOrigin::Table((*row).clone())));
    }
    Ok((p, D_bOrigin::Formula))
}

/// 行列表（错误信息里的候选展示）。
fn join_rows(rows: &[Din5480Row]) -> String {
    rows.iter()
        .map(|r| format!("m={} z={} x={}", trim(r.m), r.z, trim(r.x)))
        .collect::<Vec<_>>()
        .join("、")
}

// ─────────────────────────── 参数 ───────────────────────────

/// 渐开线花键几何参数（预设 + 逐个覆盖）。
#[derive(Debug, Clone, PartialEq)]
pub struct InvolParams {
    /// 模数 m。
    pub m: f64,
    /// 齿数 z。
    pub z: u32,
    /// 变位系数 x（GB 通常 0；DIN 见 `x·m` 范围）。
    pub x: f64,
    /// 压力角 α（度）。
    pub alpha_deg: f64,
    /// 齿顶高系数 ha*。
    pub ha_star: f64,
    /// 齿根高系数 hf*。
    pub hf_star: f64,
    /// 齿根圆角系数 ρf*（`ρf = rho_star·m`）。
    pub rho_star: f64,
    /// 齿形裕度系数 cF*（`cF = c_f_star·m`）。
    pub c_f_star: f64,
    /// 所属标准。
    pub std: SplineStd,
    /// 齿廓名（预设名，如 `30圆齿根` / `DIN30`）。
    pub profile: &'static str,
    /// DIN 5480 基准直径 d_B（身份标识/显示；`None` = 未给，外部传入/回填）。
    pub d_b: Option<f64>,
}

impl InvolParams {
    /// 由预设构造（`x=0`、`d_b=None`），立即校验。
    pub fn from_preset(std: SplineStd, profile: &str, m: f64, z: u32) -> Result<Self, String> {
        let preset = preset(std, profile)?;
        let p = Self {
            m,
            z,
            x: 0.0,
            alpha_deg: preset.alpha_deg,
            ha_star: preset.ha_star,
            hf_star: preset.hf_star,
            rho_star: preset.rho_star,
            c_f_star: preset.c_f_star,
            std,
            profile: preset.profile,
            d_b: None,
        };
        p.validate()?;
        Ok(p)
    }

    /// GB/T 3478.1-2008（默认预设）构造。
    pub fn gb(profile: &str, m: f64, z: u32) -> Result<Self, String> {
        Self::from_preset(SplineStd::GB, profile, m, z)
    }

    /// DIN 5480-1:2015 构造（`x` 必须在 `[−0.05, 0.45]`）。
    pub fn din(m: f64, z: u32, x: f64) -> Result<Self, String> {
        let mut p = Self::from_preset(SplineStd::DIN, "DIN30", m, z)?;
        p.x = x;
        p.validate()?;
        Ok(p)
    }

    // ── 覆盖（builder；调用方负责再 `validate()`）──

    /// 覆盖变位系数 x。
    pub fn with_x(mut self, x: f64) -> Self {
        self.x = x;
        self
    }

    /// 覆盖压力角（度）。
    pub fn with_alpha_deg(mut self, alpha_deg: f64) -> Self {
        self.alpha_deg = alpha_deg;
        self
    }

    /// 逐个覆盖四个系数。
    pub fn with_coeffs(mut self, ha_star: f64, hf_star: f64, rho_star: f64, c_f_star: f64) -> Self {
        self.ha_star = ha_star;
        self.hf_star = hf_star;
        self.rho_star = rho_star;
        self.c_f_star = c_f_star;
        self
    }

    /// 外部传入/回填 DIN 基准直径 d_B（只影响显示，不参与几何）。
    pub fn with_d_b(mut self, d_b: f64) -> Self {
        self.d_b = Some(d_b);
        self
    }

    // ── 导出量 ──

    /// 压力角 α（弧度）。
    pub fn alpha(&self) -> f64 {
        self.alpha_deg.to_radians()
    }

    /// 分度圆直径 `d = m·z`（DIN 表 2 的 “Pitch diameter d” 口径）。
    pub fn d(&self) -> f64 {
        self.m * self.z as f64
    }

    /// 计入变位的计算直径 `d′ = m·z + 2x·m`（GB x=0 时 = d；
    /// = `d_a1 − 0.9m` = `d_f1 + 2·hf*·m`）。
    pub fn d_eff(&self) -> f64 {
        self.d() + 2.0 * self.x * self.m
    }

    /// 基圆直径 `db = d·cosα`。
    pub fn db(&self) -> f64 {
        self.d() * self.alpha().cos()
    }

    /// 齿顶圆直径：`d′ + 2·ha*·m`（GB 即 m(z+1) / m(z+0.9) / m(z+0.8)；
    /// DIN 即 `d_a1 = mz+2xm+0.9m`）。
    pub fn da(&self) -> f64 {
        self.d_eff() + 2.0 * self.ha_star * self.m
    }

    /// 齿根圆直径：`d′ − 2·hf*·m`（GB 即 m(z−1.5)/m(z−1.8)/m(z−1.4)/m(z−1.2)；
    /// DIN 即 `d_f1 = mz+2xm−2·hf*·m`）。
    pub fn df(&self) -> f64 {
        self.d_eff() - 2.0 * self.hf_star * self.m
    }

    /// 分度圆齿厚 `s = mπ/2 + 2x·m·tanα`。
    pub fn s(&self) -> f64 {
        std::f64::consts::PI * self.m / 2.0 + 2.0 * self.x * self.m * self.alpha().tan()
    }

    /// 变位量 `x·m`。
    pub fn x_m(&self) -> f64 {
        self.x * self.m
    }

    /// 齿根圆角半径 `ρf = rho_star·m`。
    pub fn rho_f(&self) -> f64 {
        self.rho_star * self.m
    }

    /// 齿形裕度 `cF = c_f_star·m`。
    pub fn c_f(&self) -> f64 {
        self.c_f_star * self.m
    }

    /// 顶隙 `c = (hf*−ha*)·m`（DIN 为 0.10m；GB 为几何差值，不另查表）。
    pub fn clearance(&self) -> f64 {
        (self.hf_star - self.ha_star) * self.m
    }

    /// 齿距角 `2π/z`（弧度）。
    pub fn pitch_angle(&self) -> f64 {
        std::f64::consts::TAU / self.z as f64
    }

    /// 齿高 `(da−df)/2`（收尾公式里的 h）。
    pub fn tooth_depth(&self) -> f64 {
        (self.da() - self.df()) / 2.0
    }

    /// 齿厚半角 `ψ(R) = s/d + invα − invα_R`（弧度；`R<db` 时按 db 夹住，与 `gear.rs` 同口径）。
    pub fn half_tooth_angle(&self, radius: f64) -> f64 {
        let r = self.d() / 2.0;
        let rb = self.db() / 2.0;
        let a_r = (rb / radius.max(rb)).clamp(-1.0, 1.0).acos();
        self.s() / (2.0 * r) + (inv(self.alpha()) - inv(a_r))
    }

    /// 渐开线起始半径（外花键）：`max(db, df)/2`（基圆以内没有渐开线）。
    pub fn r_involute_start(&self) -> f64 {
        (self.db() / 2.0).max(self.df() / 2.0)
    }

    /// 渐开线终止半径（外花键）：大径/2。
    pub fn r_involute_end(&self) -> f64 {
        self.da() / 2.0
    }

    /// 渐开线起始圆直径（显示用）。
    pub fn d_involute_start(&self) -> f64 {
        2.0 * self.r_involute_start()
    }

    /// 渐开线终止圆直径（= 大径，显示用）。
    pub fn d_involute_end(&self) -> f64 {
        self.da()
    }

    /// 内花键齿顶圆（**接口，本轮不出几何**）：GB = `m(z+1.5)/(z+1.8)/(z+1.4)/(z+1.2)`，
    /// 即 `d′+2·hf*·m`；DIN 按侧配合名义 `d_a2 = d_a1 = d′+0.9m`。
    pub fn internal_tip_dia(&self) -> f64 {
        match self.std {
            SplineStd::GB => self.d_eff() + 2.0 * self.hf_star * self.m,
            SplineStd::DIN => self.d_eff() + 0.9 * self.m,
        }
    }

    /// 旧的 DIN 名义估算 `m(z+2x)`（= `d + 2x·m`，相当于 `d_B − 1.1m`）；
    /// **不是** `d_B` 定义式。新代码用 [`x_from_d_b`] / [`d_b_from_x`] / [`lookup_by_d_b`]。
    pub fn d_b_estimate(&self) -> f64 {
        self.m * (self.z as f64 + 2.0 * self.x)
    }

    /// DIN 变位是否在 `x·m ∈ [−0.05m, +0.45m]`（即 x∈[−0.05, 0.45]）内。
    pub fn x_in_range(&self) -> bool {
        self.x >= -0.05 - 1e-9 && self.x <= 0.45 + 1e-9
    }

    /// 齿顶变尖/渐开线交叉：`ψ(da/2) ≤ 0`（端视图画不出真实齿廓）。
    pub fn tooth_tip_crossed(&self) -> bool {
        self.half_tooth_angle(self.da() / 2.0) <= 1e-9
    }

    /// 齿槽过宽/相邻齿廓在齿根处交叉：`ψ(r_start) ≥ 半齿距`。
    pub fn root_space_crossed(&self) -> bool {
        self.half_tooth_angle(self.r_involute_start()) >= self.pitch_angle() / 2.0 - 1e-9
    }

    /// 参数自检（错误信息直接给用户看）。
    pub fn validate(&self) -> Result<(), String> {
        if !(self.m.is_finite() && self.m > 0.0) {
            return Err(format!("渐开线花键：模数 m={} 必须是正数。", self.m));
        }
        if !(3..=1000).contains(&self.z) {
            return Err(format!(
                "渐开线花键：齿数 z={} 超出范围（3..=1000）。",
                self.z
            ));
        }
        if !(self.alpha_deg.is_finite() && self.alpha_deg > 10.0 && self.alpha_deg < 50.0) {
            return Err(format!(
                "渐开线花键：压力角 α 超出范围（10°<α<50°）：{}°。",
                self.alpha_deg
            ));
        }
        if !(self.ha_star.is_finite() && self.ha_star > 0.0) {
            return Err(format!(
                "渐开线花键：齿顶高系数 ha*={} 必须是正数。",
                self.ha_star
            ));
        }
        if !(self.hf_star.is_finite() && self.hf_star > 0.0) {
            return Err(format!(
                "渐开线花键：齿根高系数 hf*={} 必须是正数。",
                self.hf_star
            ));
        }
        if !(self.rho_star.is_finite() && self.rho_star >= 0.0) {
            return Err(format!(
                "渐开线花键：齿根圆角系数 ρf*={} 不能为负。",
                self.rho_star
            ));
        }
        if !(self.c_f_star.is_finite() && self.c_f_star >= 0.0) {
            return Err(format!(
                "渐开线花键：齿形裕度系数 cF*={} 不能为负。",
                self.c_f_star
            ));
        }
        if !self.x.is_finite() {
            return Err(format!("渐开线花键：变位系数 x={} 必须是有限数。", self.x));
        }
        if self.std == SplineStd::DIN && !self.x_in_range() {
            return Err(format!(
                "DIN 5480：变位系数 x={} 超出 x·m∈[−0.05m, +0.45m]（x∈[−0.05, 0.45]）。",
                trim(self.x)
            ));
        }
        if self.df() <= 1e-9 {
            return Err(format!(
                "渐开线花键：齿根圆直径 df={} 非正（检查 m/z/x/hf* 组合）。",
                trim(self.df())
            ));
        }
        if self.da() <= self.df() + 1e-9 {
            return Err(format!(
                "渐开线花键：齿顶圆 da={} 不大于齿根圆 df={}。",
                trim(self.da()),
                trim(self.df())
            ));
        }
        Ok(())
    }

    /// 规格文本（块名/明细表用）。
    pub fn spec(&self) -> String {
        let mut s = format!(
            "{} {} m{} z{}",
            self.std.label(),
            self.profile,
            trim(self.m),
            self.z
        );
        if self.x.abs() > 1e-12 {
            s.push_str(&format!(" x{}", trim(self.x)));
        }
        if let Some(d_b) = self.d_b {
            s.push_str(&format!(" d_B{}", trim(d_b)));
        }
        s
    }

    // ── 端视图（真实渐开线齿廓）──

    /// 渐开线上一点：齿中心线角 `center_rad`，`sign=±1` 取两侧，`radius` 处。
    fn flank_point(&self, radius: f64, center_rad: f64, sign: f64) -> [f64; 2] {
        let th = center_rad + sign * self.half_tooth_angle(radius);
        [radius * th.cos(), radius * th.sin()]
    }

    /// 一条齿廓（含基圆高于小径时的径向直线），从齿根端到齿顶端（`sign=+1` 侧）。
    fn push_flank(&self, out: &mut Vec<EntityType>, center_rad: f64, sign: f64) {
        let r0 = self.r_involute_start();
        let r1 = self.r_involute_end();
        let rf = self.df() / 2.0;
        let rb = self.db() / 2.0;
        // 基圆高于小径：先沿半径从齿根圆接到基圆（渐开线从基圆才开始）。
        if rb > rf + 1e-9 {
            let th = center_rad + sign * self.half_tooth_angle(rb);
            out.push(line(
                [rf * th.cos(), rf * th.sin()],
                [rb * th.cos(), rb * th.sin()],
                LAYER_MAIN,
            ));
        }
        let n = INVOLUTE_SEGMENTS;
        for i in 0..n {
            let ra = r0 + (r1 - r0) * i as f64 / n as f64;
            let rb2 = r0 + (r1 - r0) * (i + 1) as f64 / n as f64;
            out.push(line(
                self.flank_point(ra, center_rad, sign),
                self.flank_point(rb2, center_rad, sign),
                LAYER_MAIN,
            ));
        }
    }

    /// 端视图：真实渐开线齿廓 + 齿顶/齿根弧 + 十字中心线。
    ///
    /// 相位：齿槽心线 `0°+k·360°/z`，齿心线 `180°/z+k·360°/z`。
    pub fn front_view(&self) -> Result<Vec<EntityType>, String> {
        self.validate()?;
        if self.tooth_tip_crossed() {
            return Err(format!(
                "{} {}：齿顶变尖（ψ(da/2)={:.4}° ≤ 0），两条渐开线在齿顶圆之前相交，\
                 端视图画不出真实齿廓；请减小 ha* / 增大齿数。",
                self.std.label(),
                self.profile,
                self.half_tooth_angle(self.da() / 2.0).to_degrees()
            ));
        }
        if self.root_space_crossed() {
            return Err(format!(
                "{} {}：齿槽过宽（ψ(r_start)={:.4}° ≥ 半齿距 {:.4}°），相邻齿廓在齿根之前相交，\
                 端视图画不出真实齿廓；请减小 hf*/α 或增大齿数。",
                self.std.label(),
                self.profile,
                self.half_tooth_angle(self.r_involute_start()).to_degrees(),
                (self.pitch_angle() / 2.0).to_degrees()
            ));
        }
        let pitch = self.pitch_angle();
        let ra = self.da() / 2.0;
        let rf = self.df() / 2.0;
        let r0 = self.r_involute_start();
        let psi_root = self.half_tooth_angle(r0);
        let psi_tip = self.half_tooth_angle(ra);
        let mut out = Vec::with_capacity(self.z as usize * (2 * INVOLUTE_SEGMENTS + 2) + 2);
        for k in 0..self.z {
            // 齿心线 180°/z + k·360°/z
            let c = pitch * (k as f64 + 0.5);
            self.push_flank(&mut out, c, 1.0);
            out.push(arc(
                [0.0, 0.0],
                ra,
                (c - psi_tip).to_degrees(),
                (c + psi_tip).to_degrees(),
                LAYER_MAIN,
            ));
            self.push_flank(&mut out, c, -1.0);
            // 齿槽弧：从本齿 +1 侧齿根端到下一齿 −1 侧齿根端（中心 = 齿槽心线 0°+k·360°/z）
            out.push(arc(
                [0.0, 0.0],
                rf,
                (c + psi_root).to_degrees(),
                (c + pitch - psi_root).to_degrees(),
                LAYER_MAIN,
            ));
        }
        // 十字中心线：长度 = 大径 + 6（与 `spline.rs` 一致）。
        let half = ra + 3.0;
        out.push(line([-half, 0.0], [half, 0.0], LAYER_CENTER));
        out.push(line([0.0, -half], [0.0, half], LAYER_CENTER));
        Ok(out)
    }

    // ── 侧视图（矩形 + 小径线）──

    /// 侧视轮廓：矩形 `L × 大径` + 小径线（小径线落 `minor_layer`，由调用方定线型）。
    pub fn side_view_on(&self, len: f64, minor_layer: &str) -> Vec<EntityType> {
        let ra = self.da() / 2.0;
        let rf = self.df() / 2.0;
        vec![
            line([0.0, -ra], [0.0, ra], LAYER_MAIN),
            line([len, -ra], [len, ra], LAYER_MAIN),
            line([0.0, ra], [len, ra], LAYER_MAIN),
            line([0.0, -ra], [len, -ra], LAYER_MAIN),
            line([0.0, -rf], [len, -rf], minor_layer),
            line([0.0, rf], [len, rf], minor_layer),
            line([-3.0, 0.0], [len + 3.0, 0.0], LAYER_CENTER),
        ]
    }

    /// 常规侧视图（小径线 `2细线层`）。
    pub fn side_view(&self, len: f64) -> Vec<EntityType> {
        self.side_view_on(len, LAYER_THIN)
    }

    /// 侧剖视图（小径线 `1轮廓实线层`；不含剖面线，由调用方按需补）。
    pub fn section_view(&self, len: f64) -> Vec<EntityType> {
        self.side_view_on(len, LAYER_MAIN)
    }

    // ── 收尾 ──

    /// 滚刀半径 `R = de/2`。
    pub fn hob_radius(&self, de: f64) -> f64 {
        de / 2.0
    }

    /// 收尾长度 `l = √(h(2R−h))`（`h=(da−df)/2`、`R=de/2`；`de` 须大于大径）。
    pub fn runout_length(&self, de: f64) -> Result<f64, String> {
        if !(de.is_finite() && de > 0.0) {
            return Err(format!("渐开线花键：滚刀外径 de={de} 必须是正数。"));
        }
        if de <= self.da() + 1e-9 {
            return Err(format!(
                "渐开线花键：滚刀外径 de={} 必须大于外花键大径 da={}（否则收尾弧切不出去）。",
                trim(de),
                trim(self.da())
            ));
        }
        let h = self.tooth_depth();
        let r = self.hob_radius(de);
        Ok((h * (2.0 * r - h)).max(0.0).sqrt())
    }

    /// 收尾弧与大径交点在弧圆心中的极角（度）；上弧从 270° 转到 `360°−a`，下弧从 `a` 转到 90°。
    pub fn runout_end_angle(&self, de: f64) -> Result<f64, String> {
        let h = self.tooth_depth();
        let r = self.hob_radius(de);
        let l = self.runout_length(de)?;
        Ok((r - h).atan2(l).to_degrees())
    }

    /// 收尾弧圆心（`x0` = 满齿段右端，`sign=±1` 取上/下；与小径相切于 `(x0, ±df/2)`）。
    pub fn runout_center(&self, x0: f64, de: f64, sign: f64) -> Result<[f64; 2], String> {
        let r = self.hob_radius(de);
        self.runout_length(de)?;
        Ok([x0, sign * (self.df() / 2.0 + r)])
    }

    /// 收尾图元（2 条弧 + 终点竖线，落 `layer`；`x0` = 满齿段右端）。
    pub fn runout_arcs(&self, x0: f64, de: f64, layer: &str) -> Result<Vec<EntityType>, String> {
        let r = self.hob_radius(de);
        let l = self.runout_length(de)?;
        let a = self.runout_end_angle(de)?;
        let rf = self.df() / 2.0;
        let ra = self.da() / 2.0;
        Ok(vec![
            arc([x0, rf + r], r, 270.0, 360.0 - a, layer),
            arc([x0, -(rf + r)], r, a, 90.0, layer),
            line([x0 + l, -ra], [x0 + l, ra], layer),
        ])
    }
}

/// 渐开线展角函数 `inv α = tanα − α`。
fn inv(a: f64) -> f64 {
    a.tan() - a
}

// ─────────────────────────── 测试 ───────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn near6(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn has_line(entities: &[EntityType], a: [f64; 2], b: [f64; 2]) -> bool {
        entities.iter().any(|e| match e {
            EntityType::Line(l) => {
                let (p, q) = ([l.start.x, l.start.y], [l.end.x, l.end.y]);
                (near(p[0], a[0]) && near(p[1], a[1]) && near(q[0], b[0]) && near(q[1], b[1]))
                    || (near(p[0], b[0])
                        && near(p[1], b[1])
                        && near(q[0], a[0])
                        && near(q[1], a[1]))
            }
            _ => false,
        })
    }

    fn line_layer_at(entities: &[EntityType], y: f64) -> Option<&str> {
        entities.iter().find_map(|e| match e {
            EntityType::Line(l) if near(l.start.y, y) && near(l.end.y, y) => {
                Some(l.common.layer.as_str())
            }
            _ => None,
        })
    }

    fn arc_centers_at(entities: &[EntityType], radius: f64) -> Vec<(f64, f64)> {
        entities
            .iter()
            .filter_map(|e| match e {
                EntityType::Arc(a) if near(a.radius, radius) => {
                    let c = (a.start_angle.to_degrees() + a.end_angle.to_degrees()) / 2.0;
                    let span = a.end_angle.to_degrees() - a.start_angle.to_degrees();
                    Some((c.rem_euclid(360.0), span))
                }
                _ => None,
            })
            .collect()
    }

    /// GB 四种齿廓：da/df/ρf/cF/d/db/s 与标准公式逐项吻合；内花键接口同步。
    #[test]
    fn gb_four_profiles_match_standard_formulas() {
        let m = 3.0;
        let z = 20u32;
        // (profile, α, ha*, hf*, ρf*, 大径系数, 小径系数)
        let cases: [(&str, f64, f64, f64, f64, f64, f64); 4] = [
            ("30平齿根", 30.0, 0.5, 0.75, 0.2, 1.0, 1.5),
            ("30圆齿根", 30.0, 0.5, 0.9, 0.4, 1.0, 1.8),
            ("37.5圆齿根", 37.5, 0.45, 0.7, 0.3, 0.9, 1.4),
            ("45圆齿根", 45.0, 0.4, 0.6, 0.25, 0.8, 1.2),
        ];
        for (profile, alpha, ha, hf, rho, da_c, df_c) in cases {
            let p = InvolParams::gb(profile, m, z).unwrap();
            assert_eq!(p.std, SplineStd::GB);
            assert!(near(p.alpha_deg, alpha), "{profile} α");
            assert!(near(p.ha_star, ha), "{profile} ha*");
            assert!(near(p.hf_star, hf), "{profile} hf*");
            assert!(near(p.rho_star, rho), "{profile} ρf*");
            assert!(near(p.c_f_star, 0.1), "{profile} cF*");
            // 直径一律标准公式
            assert!(near(p.d(), m * z as f64), "{profile} d");
            assert!(
                near(p.db(), p.d() * alpha.to_radians().cos()),
                "{profile} db"
            );
            assert!(near(p.da(), m * (z as f64 + da_c)), "{profile} da");
            assert!(near(p.df(), m * (z as f64 - df_c)), "{profile} df");
            // 派生量
            assert!(near(p.rho_f(), rho * m), "{profile} ρf");
            assert!(near(p.c_f(), 0.1 * m), "{profile} cF");
            assert!(near(p.s(), std::f64::consts::PI * m / 2.0), "{profile} s");
            assert!(near(p.tooth_depth(), (p.da() - p.df()) / 2.0));
            assert!(
                near(p.internal_tip_dia(), m * (z as f64 + df_c)),
                "{profile} Dii"
            );
            // 齿顶/渐开线终止、起始圆
            assert!(near(p.d_involute_end(), p.da()), "{profile} 终止圆 = da");
            assert!(
                near(p.d_involute_start(), p.db().max(p.df())),
                "{profile} 起始圆"
            );
            p.validate().unwrap();
        }
    }

    /// DIN：hf*=0.55（齿侧对中基准）、公式与 DIN 5480-2 名义表逐行吻合；x 边界 [−0.05, 0.45]。
    #[test]
    fn din_formulas_match_5480_2_and_x_boundaries() {
        let (m, z, x) = (2.0, 18u32, 0.2);
        let p = InvolParams::din(m, z, x).unwrap();
        assert_eq!(p.std, SplineStd::DIN);
        assert!(near(p.alpha_deg, 30.0));
        assert!(
            near(p.ha_star, 0.45) && near(p.hf_star, 0.55) && near(p.rho_star, 0.16),
            "DIN 基础齿廓：h_fP*=0.55（DIN 5480-1 条 5.1）"
        );
        assert!(near(p.c_f_star, 0.10), "cF* = c* = h_fP*−h_aP* = 0.10");
        let alpha = 30f64.to_radians();
        let d = m * z as f64;
        assert!(near(p.d(), d));
        assert!(near(p.d_eff(), d + 2.0 * x * m));
        assert!(near(p.da(), d + 2.0 * x * m + 0.9 * m), "d_a1={}", p.da());
        assert!(
            near(p.df(), d + 2.0 * x * m - 2.0 * 0.55 * m),
            "d_f1={}",
            p.df()
        );
        assert!(near(
            p.s(),
            std::f64::consts::PI * m / 2.0 + 2.0 * x * m * alpha.tan()
        ));
        assert!(near(p.db(), d * alpha.cos()));
        assert!(near(p.rho_f(), 0.16 * m));
        assert!(near(p.clearance(), 0.10 * m), "c* = hf*−ha* = 0.10");
        assert!(near(p.c_f(), 0.10 * m), "cF = 0.10m");
        assert!(near(p.x_m(), x * m));
        assert!(p.x_in_range());
        p.validate().unwrap();

        // ── DIN 5480-2 名义表核对（`DIN5480-2_名义表.csv` p.11，m=0.5、x·m=0.225 即 x=0.45）：──
        //   z=34（d_B=18）→ d=17.00、d_a1=17.90、d_f1=16.90；z=10（d_B=6）→ d=5.00、d_a1=5.90、d_f1=4.90。
        //   hf*=0.55 时 d_f1 = mz + 2xm − 1.1m 与表逐行一致（旧 hf*=0.60 会得 16.85/4.85）。
        for (z2, da_want, df_want) in [(34u32, 17.90, 16.90), (10u32, 5.90, 4.90)] {
            let q = InvolParams::din(0.5, z2, 0.45).unwrap();
            assert!(near6(q.d(), 0.5 * z2 as f64), "z={z2} d");
            assert!(near6(q.da(), da_want), "z={z2} d_a1={}", q.da());
            assert!(near6(q.df(), df_want), "z={z2} d_f1={}", q.df());
        }

        // 边界内/外
        for good in [-0.05, 0.0, 0.45] {
            assert!(InvolParams::din(m, z, good).is_ok(), "x={good} 应合法");
        }
        for bad in [-0.050_1, 0.450_1] {
            let e = InvolParams::din(m, z, bad).unwrap_err();
            assert!(e.contains("x"), "错误信息要点名 x：{e}");
        }
    }

    /// 预设代号（CLI/DSL/GUI）：四 GB + DIN 往返；常见别名；非法代号拒绝。
    #[test]
    fn preset_token_round_trip_and_aliases() {
        for std in [SplineStd::GB, SplineStd::DIN] {
            for p in std.presets() {
                let code = preset_code(std, p.profile).expect("有代号");
                let (std2, profile2) = parse_preset_token(code).expect("能解析");
                assert_eq!(std2, std, "{code}");
                assert_eq!(profile2, p.profile, "{code}");
            }
        }
        assert_eq!(parse_preset_token("GB").unwrap().1, "30圆齿根", "GB 裸写默认");
        assert_eq!(parse_preset_token("DIN").unwrap().0, SplineStd::DIN);
        assert_eq!(parse_preset_token("gb 30p").unwrap().1, "30平齿根");
        assert_eq!(parse_preset_token("GB30圆齿根").unwrap().1, "30圆齿根");
        assert_eq!(parse_preset_token("GB37.5圆齿根").unwrap().1, "37.5圆齿根");
        assert_eq!(parse_preset_token("GB375R").unwrap().1, "37.5圆齿根");
        assert_eq!(parse_preset_token("din30r").unwrap().0, SplineStd::DIN);
        for bad in ["", "GB99", "DIN99", "ANSI", "GB30x"] {
            assert!(parse_preset_token(bad).is_none(), "{bad} 应拒绝");
        }
    }

    /// db 随 α 变化；ψ(R) 与 `gear.rs` 同口径（R=rb 时 ψ=s/d+invα）。
    #[test]
    fn db_and_involute_track_alpha() {
        let (m, z) = (3.0, 20u32);
        let gb30 = InvolParams::gb("30圆齿根", m, z).unwrap();
        let gb375 = InvolParams::gb("37.5圆齿根", m, z).unwrap();
        let gb45 = InvolParams::gb("45圆齿根", m, z).unwrap();
        let d = m * z as f64;
        for p in [&gb30, &gb375, &gb45] {
            assert!(
                near(p.db(), d * p.alpha_deg.to_radians().cos()),
                "{} db",
                p.profile
            );
            let want = p.s() / d + inv(p.alpha_deg.to_radians());
            assert!(
                near6(p.half_tooth_angle(p.db() / 2.0), want),
                "{} ψ(rb)",
                p.profile
            );
        }
        assert!(gb30.db() > gb375.db() && gb375.db() > gb45.db());
        // α 越大齿顶半角越小（同 ha* 下）
        assert!(gb30.half_tooth_angle(gb30.da() / 2.0) > gb45.half_tooth_angle(gb45.da() / 2.0));
    }

    /// 端视图：齿数 / 相位（0° 处是齿槽）/ 渐开线端点解析式 / 实体数。
    #[test]
    fn front_view_phase_tooth_count_and_involute() {
        let p = InvolParams::gb("30圆齿根", 3.0, 20).unwrap();
        let e = p.front_view().unwrap();
        let (ra, rf) = (p.da() / 2.0, p.df() / 2.0);
        let tips = arc_centers_at(&e, ra);
        let roots = arc_centers_at(&e, rf);
        assert_eq!(tips.len(), 20, "20 条齿顶弧");
        assert_eq!(roots.len(), 20, "20 条齿根弧");
        // 齿顶弧中心 180/z + k·360/z = 9° + 18k
        let mut want_tip: Vec<String> = (0..20)
            .map(|k| format!("{:.4}", (9.0 + 18.0 * k as f64).rem_euclid(360.0)))
            .collect();
        let mut got_tip: Vec<String> = tips.iter().map(|(c, _)| format!("{c:.4}")).collect();
        want_tip.sort();
        got_tip.sort();
        assert_eq!(got_tip, want_tip, "齿心线相位");
        // 齿槽弧中心 0° + k·360/z（0° 处是齿槽）
        let mut want_root: Vec<String> =
            (0..20).map(|k| format!("{:.4}", 18.0 * k as f64)).collect();
        let mut got_root: Vec<String> = roots.iter().map(|(c, _)| format!("{c:.4}")).collect();
        want_root.sort();
        got_root.sort();
        assert_eq!(got_root, want_root, "齿槽心线相位");
        // 0° 齿槽：有一条齿根弧中心归一后为 0°、且几何上跨 0°（start 在 360° 前、end 在 360° 后）
        assert!(
            e.iter().any(|x| matches!(x, EntityType::Arc(a)
                if near(a.radius, rf)
                    && (((a.start_angle + a.end_angle) / 2.0).to_degrees()).rem_euclid(360.0) < 1e-6
                    && a.start_angle.to_degrees().rem_euclid(360.0) > 180.0
                    && a.end_angle.to_degrees().rem_euclid(360.0) < 180.0)),
            "0° 处应是齿槽（齿根弧跨 0°）"
        );
        assert!(!got_tip.iter().any(|s| s == "0.0000"));
        // 渐开线端点解析式（第一齿 +1 侧，c = pitch/2 = 9°）
        let c = p.pitch_angle() / 2.0;
        let q0 = p.flank_point(rf, c, 1.0);
        let q1 = p.flank_point(ra, c, 1.0);
        let q0b = p.flank_point(
            p.r_involute_start() + (ra - p.r_involute_start()) / INVOLUTE_SEGMENTS as f64,
            c,
            1.0,
        );
        assert!(has_line(&e, q0, q0b), "渐开线首段端点");
        let last = e.iter().find_map(|x| match x {
            EntityType::Line(l)
                if near((l.end.x.powi(2) + l.end.y.powi(2)).sqrt(), ra)
                    && (l.end.y.atan2(l.end.x) - (c + p.half_tooth_angle(ra))).abs() < 1e-6 =>
            {
                Some(())
            }
            _ => None,
        });
        assert!(last.is_some(), "渐开线末点应落在齿顶圆上");
        assert!(near(q1[0].hypot(q1[1]), ra));
        // 实体数：每齿 2×12 折线 + 2 弧，加 2 中心线（本件 db<df 无径向直线）
        assert!(p.db() / 2.0 < rf);
        assert_eq!(e.len(), 20 * (2 * INVOLUTE_SEGMENTS + 2) + 2);
    }

    /// 侧视图/剖视图：矩形 + 小径线图层不同；坐标口径。
    #[test]
    fn side_view_rectangle_and_minor_layers() {
        let p = InvolParams::gb("30圆齿根", 3.0, 20).unwrap();
        let len = 30.0;
        let (ra, rf) = (p.da() / 2.0, p.df() / 2.0);
        let side = p.side_view(len);
        let section = p.section_view(len);
        assert_eq!(side.len(), 7);
        assert_eq!(section.len(), 7);
        for es in [&side, &section] {
            assert!(has_line(es, [0.0, -ra], [0.0, ra]));
            assert!(has_line(es, [len, -ra], [len, ra]));
            assert!(has_line(es, [0.0, ra], [len, ra]));
            assert!(has_line(es, [0.0, -rf], [len, -rf]));
            assert!(has_line(es, [-3.0, 0.0], [len + 3.0, 0.0]));
        }
        assert_eq!(
            line_layer_at(&side, rf),
            Some(LAYER_THIN),
            "常规侧视小径细线"
        );
        assert_eq!(
            line_layer_at(&section, rf),
            Some(LAYER_MAIN),
            "剖视小径轮廓线"
        );
        assert_eq!(line_layer_at(&side, ra), Some(LAYER_MAIN), "大径始终轮廓线");
    }

    /// 收尾弧：圆心/相切/终点/l 数值/图元；de 太小报错。
    #[test]
    fn runout_arc_center_tangency_and_length() {
        let p = InvolParams::gb("30圆齿根", 3.0, 20).unwrap();
        let de = 80.0;
        let (h, r) = (p.tooth_depth(), p.hob_radius(de));
        assert!(near(h, 4.2));
        let l = p.runout_length(de).unwrap();
        assert!(near(l, (h * (2.0 * r - h)).sqrt()));
        assert!((l - 17.842_641_2).abs() < 1e-4, "l={l}");
        // 圆心 (x0, rf+R)，切点 (x0, rf)
        let c = p.runout_center(30.0, de, 1.0).unwrap();
        assert!(near(c[0], 30.0) && near(c[1], p.df() / 2.0 + r));
        let t = [30.0, p.df() / 2.0];
        let dist = ((c[0] - t[0]).powi(2) + (c[1] - t[1]).powi(2)).sqrt();
        assert!(near(dist, r), "与小径相切");
        // 终点交大径 (x0+l, ra)
        let a = p.runout_end_angle(de).unwrap().to_radians();
        let endp = [c[0] + r * a.cos(), c[1] - r * a.sin()];
        assert!(near6(endp[0], 30.0 + l) && near6(endp[1], p.da() / 2.0));
        // 图元：2 弧 + 1 终点竖线；竖线长 ±ra
        let ents = p.runout_arcs(30.0, de, LAYER_THIN).unwrap();
        assert_eq!(ents.len(), 3);
        assert!(has_line(
            &ents,
            [30.0 + l, -p.da() / 2.0],
            [30.0 + l, p.da() / 2.0]
        ));
        // de ≤ da 报错
        let e = p.runout_length(p.da()).unwrap_err();
        assert!(e.contains("de"), "{e}");
    }

    // ── DIN 5480-2 名义表（d_B 查表）──

    /// 入库形状 + 全表反推公式 `x_from_d_b` 与表值逐行一致（含 d=m·z、基圆列）。
    #[test]
    fn din5480_table_shape_and_formula_holds_for_every_row() {
        let rows = din5480_rows();
        assert_eq!(rows.len(), 618, "入库 618 行（源 665 行剔除 p35/m=5 的 47 行）");
        assert!(rows.iter().all(|r| r.page != 35), "p35（m=5）整页已剔除");
        assert!(rows.iter().all(|r| (r.m - 5.0).abs() > 1e-9), "不该再有 m=5 残行");
        for r in rows {
            assert!(
                (r.d - r.m * r.z as f64).abs() < 1e-9,
                "p{} m{} z{}：d≠m·z",
                r.page,
                trim(r.m),
                r.z
            );
            // 基圆列（CSV 的 d_b = d·cos30°）是排版值，精度 3 位 → 容差 0.05。
            assert!(
                (r.base_dia - r.d * 30f64.to_radians().cos()).abs() < 0.05,
                "p{} 基圆列 d_b 与 d·cos30 偏离过大：{} vs {}",
                r.page,
                r.base_dia,
                r.d * 30f64.to_radians().cos()
            );
            // 任务指定公式：x = (d_B − m(z+1.1)) / (2m)，与表列 x1_m/m 逐行一致。
            let x = x_from_d_b(r.d_b, r.m, r.z);
            assert!(
                (x - r.x).abs() < 1e-9,
                "p{} m{} z{} d_B{}：x_from_d_b={} 表 x={}",
                r.page,
                trim(r.m),
                r.z,
                trim(r.d_b),
                x,
                r.x
            );
            assert!((d_b_from_x(r.m, r.z, x) - r.d_b).abs() < 1e-9, "正反变换不闭合");
        }
        let mods = din5480_modules();
        assert_eq!(mods.len(), 14, "已入库 14 个模数档：{mods:?}");
        assert!(!mods.iter().any(|m| (*m - 1.5).abs() < 1e-9), "m=1.5 未入库");
        assert!(!mods.iter().any(|m| (*m - 5.0).abs() < 1e-9), "m=5 已剔除");
    }

    /// 查表命中：抽各档（含多行 z/x₁ 变体）；按 d_B+m / d_B+z 补全。
    #[test]
    fn din5480_lookup_hits_samples_and_multi_row_variants() {
        // 各档抽一行：m0.5→p11 z10 x0.45；m1→p19 z16 x0.45；m1.25→p21 z10 x0.05；
        // m1.75→p25 z6；m2→p27 z18 x0.45；m10→p41 z44 x−0.05。
        let cases: [(f64, f64, u32, u16); 6] = [
            (0.5, 6.0, 10, 11),
            (1.0, 18.0, 16, 19),
            (1.25, 14.0, 10, 21),
            (1.75, 13.0, 6, 25),
            (2.0, 40.0, 18, 27),
            (10.0, 450.0, 44, 41),
        ];
        for (m, d_b, z, page) in cases {
            let rows = lookup_by_d_b(d_b, Some(m)).expect("应命中");
            assert!(
                rows.iter().any(|r| r.z == z && r.page == page),
                "m={} d_B={} 应命中 p{} z={}：{:?}",
                m,
                d_b,
                page,
                z,
                rows.iter().map(|r| (r.page, r.z)).collect::<Vec<_>>()
            );
            // 每条命中行都满足 d_B 反解式（表值精度）。
            for r in &rows {
                assert!((x_from_d_b(r.d_b, r.m, r.z) - r.x).abs() < 1e-9);
            }
            // 只给 d_B + m → 补 z；只给 d_B + z → 补 m。
            let (p1, o1) = resolve_din_by_d_b(d_b, Some(m), None, None).expect("d_B+m 应补 z");
            assert_eq!(p1.z, z, "d_B+m 补出的 z");
            assert!(matches!(o1, D_bOrigin::Table(_)));
            let (p2, o2) = resolve_din_by_d_b(d_b, None, Some(z), None).expect("d_B+z 应补 m");
            assert!((p2.m - m).abs() < 1e-9, "d_B+z 补出的 m");
            assert!(matches!(o2, D_bOrigin::Table(_)));
        }
        // 多行变体：p31 m=3 d_B=45 → z=13(x0.45) 与 z=14(x−0.05)，返回两行。
        let rows = lookup_by_d_b(45.0, Some(3.0)).unwrap();
        assert_eq!(rows.len(), 2, "同一 d_B 多个 z/x₁ 是表格事实");
        assert_eq!(rows.iter().map(|r| r.z).collect::<Vec<_>>(), vec![13, 14]);
        assert!((rows[0].x - 0.45).abs() < 1e-9 && (rows[1].x + 0.05).abs() < 1e-9);
        // 只给 d_B（不给 m）也会命中的是**跨模数**的多行（45 在多个 m 档都有）；
        // 补全时多行 → 报候选要 M/Z。
        assert!(lookup_by_d_b(45.0, None).unwrap().len() >= 2);
        let e = resolve_din_by_d_b(45.0, None, None, None).unwrap_err();
        assert!(e.contains("多个") && e.contains("m=3 z=13") && e.contains("m=3 z=14"), "{e}");
        let e = resolve_din_by_d_b(45.0, Some(3.0), None, None).unwrap_err();
        assert!(e.contains("多个 z 变体") && e.contains("请再给 Z"), "{e}");
        let e = resolve_din_by_d_b(6.0, None, Some(6), None).unwrap_err();
        assert!(e.contains("多个 m 变体") && e.contains("0.75") && e.contains("0.8"), "{e}");
    }

    /// 未命中：附近候选；缺失档位（m=1.5 空表框、m=5 已剔除）明确报数据缺失。
    #[test]
    fn din5480_lookup_miss_lists_candidates_and_missing_modules() {
        let e = lookup_by_d_b(41.0, Some(2.0)).unwrap_err();
        assert!(e.contains("附近候选"), "{e}");
        assert!(e.contains("d_B=40") && e.contains("d_B=42"), "应列出附近 40/42：{e}");
        assert!(e.contains("m=2"), "{e}");
        let e = lookup_by_d_b(1_000.0, None).unwrap_err();
        assert!(e.contains("附近候选"), "不给 m 也要给候选：{e}");
        // m=1.5：源图空表框
        let e = lookup_by_d_b(20.0, Some(1.5)).unwrap_err();
        assert!(
            e.contains("该档位数据缺失") && e.contains("源图空表框"),
            "m=1.5 应报空表框：{e}"
        );
        // m=5：p35 已整页剔除
        let e = lookup_by_d_b(50.0, Some(5.0)).unwrap_err();
        assert!(
            e.contains("该档位数据缺失") && e.contains("剔除"),
            "m=5 应报已剔除：{e}"
        );
        // 未收录档位（如 m=0.9）列出已入库档位
        let e = lookup_by_d_b(10.0, Some(0.9)).unwrap_err();
        assert!(e.contains("表中没有 m=0.9") && e.contains("已入库档位"), "{e}");
        // 非法输入
        assert!(lookup_by_d_b(-1.0, None).unwrap_err().contains("正数"));
        assert!(lookup_by_d_b(10.0, Some(0.0)).unwrap_err().contains("正数"));
    }

    /// `d_B + m + z` 互相印证：命中表行 → Table；表无此 (m,z) → Formula；不一致/越界报错。
    #[test]
    fn din5480_resolve_cross_checks_d_b_m_z() {
        // p27 m=2 d_B=40 z=18 x=0.45：查表命中。
        let (p, o) = resolve_din_by_d_b(40.0, Some(2.0), Some(18), None).unwrap();
        assert!((p.x - 0.45).abs() < 1e-9 && p.z == 18 && (p.m - 2.0).abs() < 1e-9);
        assert!((p.d() - 36.0).abs() < 1e-9 && p.d_b == Some(40.0));
        assert!(matches!(&o, D_bOrigin::Table(r) if r.page == 27), "{o:?}");
        assert_eq!(o.note(), "查表命中 p27 m=2");
        // 表里没有 (m=2, z=19)：公式解出，未命中表。
        let (p, o) = resolve_din_by_d_b(41.0, Some(2.0), Some(19), None).unwrap();
        assert!((p.x - 0.2).abs() < 1e-9, "x={}", p.x);
        assert_eq!(o.note(), "由公式解出，未命中表");
        // 组合不一致：表中 m=2 z=14 → d_B=30（x−0.05）/32（x0.45），给 40 报错。
        let e = resolve_din_by_d_b(40.0, Some(2.0), Some(14), None).unwrap_err();
        assert!(
            e.contains("组合不一致") && e.contains("d_B=30") && e.contains("d_B=32"),
            "{e}"
        );
        // x 与 d_B 不自洽：m2 z18 x0.2 → d_B=38.6，却给 40。
        let e = resolve_din_by_d_b(40.0, Some(2.0), Some(18), Some(0.2)).unwrap_err();
        assert!(e.contains("不自洽"), "{e}");
        // d_B 反解越界（x>0.45）。
        let e = resolve_din_by_d_b(100.0, Some(2.0), Some(10), None).unwrap_err();
        assert!(e.contains("超出") && e.contains("0.45"), "{e}");
    }

    // ── CSV dump（示例件，供人工审图）──

    fn csv_row(case: &str, view: &str, e: &EntityType) -> String {
        match e {
            EntityType::Line(l) => format!(
                "{case}-{view}-Line,{:.6},{:.6},{:.6},{:.6},{},,,,,\n",
                l.start.x, l.start.y, l.end.x, l.end.y, l.common.layer
            ),
            EntityType::Arc(a) => format!(
                "{case}-{view}-Arc,,,,,{},{:.6},{:.6},{:.6},{:.6},{:.6}\n",
                a.common.layer,
                a.center.x,
                a.center.y,
                a.radius,
                a.start_angle.to_degrees(),
                a.end_angle.to_degrees()
            ),
            _ => format!("{case}-{view}-other,,,,,,,,,,\n"),
        }
    }

    /// 把示例（GB 30°圆齿根 m=3 z=20；DIN m=2 z=18 x=0.2）dump 到
    /// `~/桌面/OCSM/review/invol_spline_demo.csv`（列：entity,x1,y1,x2,y2,layer,cx,cy,r,a0,a1；角度为度）。
    #[test]
    fn dump_demo_csv() {
        let gb = InvolParams::gb("30圆齿根", 3.0, 20).unwrap();
        let din = InvolParams::din(2.0, 18, 0.2).unwrap();
        let mut out = String::from("entity,x1,y1,x2,y2,layer,cx,cy,r,a0,a1\n");
        for (case, p, de) in [("GB", &gb, 80.0), ("DIN", &din, 50.0)] {
            for e in p.front_view().unwrap() {
                out.push_str(&csv_row(case, "front", &e));
            }
            for e in p.side_view(30.0) {
                out.push_str(&csv_row(case, "side", &e));
            }
            for e in p.runout_arcs(30.0, de, LAYER_THIN).unwrap() {
                out.push_str(&csv_row(case, "runout", &e));
            }
        }
        let home = std::env::var("HOME").unwrap_or_default();
        if home.is_empty() {
            eprintln!("跳过 invol_spline_demo.csv：HOME 未设置");
            return;
        }
        let dir = std::path::Path::new(&home).join("桌面/OCSM/review");
        std::fs::create_dir_all(&dir).expect("创建 review 目录");
        let path = dir.join("invol_spline_demo.csv");
        std::fs::write(&path, out).expect("写 invol_spline_demo.csv");
        assert!(path.exists());
    }
}
