//! 智能卡片「DIN 花键参数表」（`OCSMCARD`；`CardRenderer::DinTable`）：
//! **DIN 5480-1:2006-03 §9.1 Datenfeld / Bild 6 的 13 行 × 2 栏版面**（Nabe 左 / Welle 右），
//! 几何内建（不依赖外部 DXF；风格照既有 `nf_table.rs`：外框/分隔线/`OCSM_GB`/字高比例）。
//!
//! # 为什么是 13 行而不是 GB 的 21 项
//!
//! 调研（`~/桌面/OCSM/review/DIN花键参数表_调研.md`）已纠偏：DIN 的「参数表」= Datenfeld，
//! Bild 6 每栏 13 行：`标记 / z / m / α / 三个直径 / e-s 三极限 / D_M / 两个 M 极限`；
//! **d_B、d、d_b、x·m、W_k 不在 Bild 6 里**（那属 DIN 5480-2 名义表），本卡不塞。
//!
//! # 数据口径（逐格可追溯；表外不外推）
//!
//! | 字段组 | 来源 |
//! |---|---|
//! | 直径六项 + `e₂=s₁` | `assets/din5480_2_nominal.csv`（(m, d_B, z) 精确命中；查不到 → 「—」） |
//! | `Ae`/`As` | `assets/din5480_1_table7_dev.csv`（2026-09-26 OCR，双源 600dpi + 逐格复核；见该表头与 `din5480_1_table7_notes.md`） |
//! | `Tact`/`Teff` | `assets/din5480_1_table7_tol.csv`（只抽到 6–9 级、模数组 1,75–4 的实锚；其余 → 「—」，可用 `tactn/teffn/tactw/teffw` 覆盖） |
//! | `D_M`/`M2`/`M1` | `assets/din5480_2_inspection.csv`（(m,d_B,z) 命中时）；Bild 6 示例走 `assets/din5480_1_bild6_anchor.csv` |
//! | e/s 六值 | DIN 5480-1 §10.8 表 6 公式（`emax=e2+Ae+Tact+Teff` 等，见 `derive()`） |
//!
//! **Bild 6 示例**（N120×3×38×9H / W120×3×38×8f）：本卡逐行复刻标准原印值（锚点 CSV），
//! 同时用公式路重算并断言一致（测试 `bild6_example_reproduces_standard`）。
//!
//! # 明示缺口（不臆造）
//!
//! * Table 7 上段：**c1/c2（>400 侧）与 c9（≤12 细档）列映射无实锚** → 这两侧查表返回 `—`；
//!   c3=200–400 / c4=100–200 / c5=50–100 / c6=25–50 / c7=12–25 / c8=≤12 中 c4/c5 为实锚（Bild 6、KISSsoft 样张），
//!   其余按表头阶梯「推断」（来源注明）。
//! * Table 7 下段（Tact/Teff/Fp/fp/Fα/Fβ 全表体）未逐格抽取 → 只入 6–9 级实锚，10/11 级显示「—」。
//! * 直径偏差 `A_df1` 用名义表口径（切削）；Bild 6 示例的冷轧值 `−1,74` 只在锚点示例里出现。

use ocs_plugin_api::host::acadrust::entities::mtext::AttachmentPoint;
use ocs_plugin_api::host::acadrust::entities::{
    AttributeDefinition, AttributeEntity, Entity as _, EntityCommon, EntityType,
    HorizontalAlignment, Insert, MText, VerticalAlignment,
};
use ocs_plugin_api::host::acadrust::types::{Color, LineWeight, Vector3};

/// 表格块名（本卡只有一种版面：Nabe/Welle 两栏）。
pub const BLOCK: &str = "OCSM_DINTABLE_DIN";
/// 缺项标记（无表值/无锚/缺口一律显示它；预览与回执点明原因 —— 不臆造）。
pub const MISSING: &str = "—";
/// 名称/值宽度：标签列 195、值列 115（两侧共 620 宽）。
pub const W_LABEL: f64 = 195.0;
/// 单侧宽度。
pub const W_SIDE: f64 = 310.0;
/// 整表宽度。
pub const W_TOTAL: f64 = 620.0;
/// 标题行高。
pub const H_TITLE: f64 = 44.0;
/// 标记（代号）行高。
pub const H_DESIG: f64 = 60.0;
/// 数据行高（12 行）。
pub const H_ROW: f64 = 40.0;
/// 表高（= 44 + 60 + 12×40）。
pub const H_TOTAL: f64 = H_TITLE + H_DESIG + 12.0 * H_ROW;
/// 标签/值字高（照 NF 卡 25）。
pub const TEXT_H: f64 = 25.0;
/// 标题字高。
pub const TITLE_H: f64 = 30.0;
/// 标签**实体级**字宽（长标签不出标签列：195 可用宽，最长「槽宽 max. e_max」≈188）。
pub const LABEL_W_FACTOR: f64 = 0.75;
/// 值/标记**实体级**字宽（长值不出侧栏：最长标记 ≈265 < 302）。
pub const VALUE_W_FACTOR: f64 = 0.6;

/// 数据行 12 行的行中心 y（自上而下）。
pub fn data_row_y(i: usize) -> f64 {
    -(H_TITLE + H_DESIG) - (i as f64 + 0.5) * H_ROW
}
/// 标记行中心 y。
pub const DESIG_Y: f64 = -(H_TITLE + H_DESIG / 2.0);
/// 标题中心 y。
pub const TITLE_Y: f64 = -H_TITLE / 2.0;

/// 两侧的标签插入 x。
pub const LABEL_X_HUB: f64 = 6.0;
/// 外花键标签插入 x。
pub const LABEL_X_SHAFT: f64 = W_SIDE + 6.0;
/// 内花键值插入 x。
pub const VALUE_X_HUB: f64 = W_LABEL + 9.0;
/// 外花键值插入 x。
pub const VALUE_X_SHAFT: f64 = W_SIDE + W_LABEL + 9.0;

// ══════════════════════════════════════════════════════════════════════════
// Table 7 上段（Abmaße）资产
// ══════════════════════════════════════════════════════════════════════════

/// Table 7 上段（偏差表）一行：18 个 Abmaß 系列 × 9 个直径档列（µm）。
#[derive(Debug, Clone, PartialEq)]
pub struct DevRow {
    /// 外花键（Welle）偏差系列字母（v,u,t,s,r,p,n,m,k,js,h,g,f,e,d,c,b,a）。
    pub series: String,
    /// 对应的内花键（Nabe）系列字母（k↔F、js↔G、h↔H、g↔J、f↔K、e↔M；其余空）。
    pub hub_letter: String,
    /// 9 列数值（µm；`None` = 该格 OCR 留空、待核）。
    pub values: [Option<i32>; 9],
    /// 9 列范围标签与置信（`c1:>400_细档:待核|...`）。
    pub range_conf: String,
    /// 逐格 OCR/目视标记（可追溯，运行时不展示）。
    pub flags: String,
}

/// Table 7 上段资产（`include_str!`）。
const TABLE7_DEV_CSV: &str = include_str!("../assets/din5480_1_table7_dev.csv");

/// Table 7 下段（公差）锚点行。
#[derive(Debug, Clone, PartialEq)]
pub struct TolRow {
    /// 公差等级（6/7/8/9；10/11 为待抽空行）。
    pub grade: u32,
    /// 模数组（本资产只有 `1,75–4`）。
    pub module_group: String,
    /// 锚点直径说明。
    pub d_b_anchor: String,
    /// `Tact`（mm；待抽 → `None`）。
    pub tact: Option<f64>,
    /// `Teff`（mm）。
    pub teff: Option<f64>,
    /// `TG = Tact + Teff`。
    pub tg: Option<f64>,
    /// 来源。
    pub source: String,
    /// 备注。
    pub note: String,
}

const TABLE7_TOL_CSV: &str = include_str!("../assets/din5480_1_table7_tol.csv");
/// Bild 6 示例锚点（26 字段照录）。
const BILD6_ANCHOR_CSV: &str = include_str!("../assets/din5480_1_bild6_anchor.csv");

/// 逐行拆 CSV（本仓资产用引号包住含逗号字段；无转义引号）。
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

/// 十进制字段（CSV 用逗号作小数点；空格/减号统一）。
fn csv_decimal(s: &str) -> Option<f64> {
    let t = s.trim().replace(',', ".").replace('−', "-");
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

static TABLE7_DEV: std::sync::OnceLock<Vec<DevRow>> = std::sync::OnceLock::new();
static TABLE7_TOL: std::sync::OnceLock<Vec<TolRow>> = std::sync::OnceLock::new();
static BILD6_ANCHOR: std::sync::OnceLock<Vec<[String; 4]>> = std::sync::OnceLock::new();

/// Table 7 上段（懒加载；坏数据 panic = 构建错误）。
pub fn table7_rows() -> &'static [DevRow] {
    TABLE7_DEV.get_or_init(|| parse_dev_csv(TABLE7_DEV_CSV).expect("DIN 5480-1 Table 7 偏差表损坏"))
}

/// Table 7 下段锚点（懒加载）。
pub fn table7_tol_rows() -> &'static [TolRow] {
    TABLE7_TOL.get_or_init(|| parse_tol_csv(TABLE7_TOL_CSV).expect("DIN 5480-1 Table 7 公差锚损坏"))
}

/// Bild 6 锚点（`[side, field, value, source]`）。
pub fn bild6_anchor() -> &'static [[String; 4]] {
    BILD6_ANCHOR.get_or_init(|| {
        let mut out = Vec::new();
        for line in BILD6_ANCHOR_CSV.lines() {
            if line.trim().is_empty() || line.starts_with('#') || line.starts_with("side,") {
                continue;
            }
            let f = split_csv_line(line);
            if f.len() >= 4 {
                out.push([
                    f[0].trim().to_string(),
                    f[1].trim().to_string(),
                    f[2].trim().to_string(),
                    f[3].trim().to_string(),
                ]);
            }
        }
        out
    })
}

/// 锚点取值（side = `hub`/`shaft`/`meta`）。
pub fn anchor_value(side: &str, field: &str) -> Option<&'static str> {
    bild6_anchor()
        .iter()
        .find(|r| r[0] == side && r[1] == field)
        .map(|r| r[2].as_str())
}

/// 锚点数值（逗号小数/减号归一）。
pub fn anchor_f64(side: &str, field: &str) -> Option<f64> {
    anchor_value(side, field).and_then(csv_decimal)
}

/// 锚点显示串：把德文排版的 `−` 与小数逗号归一为图纸卡内一致的点号写法。
pub fn anchor_display(side: &str, field: &str) -> Option<String> {
    anchor_value(side, field).map(normalize_de)
}

/// `120 +0,76` → `120 +0.76`；`113,4 −1,74` → `113.4 -1.74`；其余原样。
fn normalize_de(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        match ch {
            '−' => out.push('-'),
            ',' => {
                let prev_digit = i > 0 && chars[i - 1].is_ascii_digit();
                let next_digit = chars.get(i + 1).is_some_and(|c| c.is_ascii_digit());
                out.push(if prev_digit && next_digit { '.' } else { ',' });
            }
            _ => out.push(*ch),
        }
    }
    out
}

fn parse_dev_csv(text: &str) -> Result<Vec<DevRow>, String> {
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        if line.trim().is_empty() || line.starts_with('#') || line.starts_with("series,") {
            continue;
        }
        let f = split_csv_line(line);
        if f.len() < 14 {
            return Err(format!("Table7 偏差表第 {n} 行只有 {} 列（应有 14 列)", f.len()));
        }
        let series = f[0].trim().to_string();
        if series.is_empty() {
            return Err(format!("Table7 偏差表第 {n} 行 series 为空"));
        }
        let mut values = [None; 9];
        for (k, slot) in values.iter_mut().enumerate() {
            let cell = f[2 + k].trim();
            if !cell.is_empty() {
                *slot = Some(
                    cell.parse::<i32>()
                        .map_err(|_| format!("Table7 第 {n} 行 c{}「{cell}」非法", k + 1))?,
                );
            }
        }
        rows.push(DevRow {
            series,
            hub_letter: f[1].trim().to_string(),
            values,
            range_conf: f[11].trim().to_string(),
            flags: f[12].trim().to_string(),
        });
    }
    if rows.is_empty() {
        return Err("Table7 偏差表为空".to_string());
    }
    Ok(rows)
}

fn parse_tol_csv(text: &str) -> Result<Vec<TolRow>, String> {
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        if line.trim().is_empty() || line.starts_with('#') || line.starts_with("grade,") {
            continue;
        }
        let f = split_csv_line(line);
        if f.len() < 8 {
            return Err(format!("Table7 公差锚第 {n} 行只有 {} 列（应有 8 列)", f.len()));
        }
        let grade = f[0]
            .trim()
            .parse::<u32>()
            .map_err(|_| format!("Table7 公差锚第 {n} 行 grade「{}」非法", f[0]))?;
        let opt = |k: usize| -> Result<Option<f64>, String> {
            let s = f[k].trim();
            if s.is_empty() || s == "—" {
                Ok(None)
            } else {
                csv_decimal(s).map(Some).ok_or_else(|| {
                    format!("Table7 公差锚第 {n} 行列 {}「{s}」非法", k + 1)
                })
            }
        };
        rows.push(TolRow {
            grade,
            module_group: f[1].trim().to_string(),
            d_b_anchor: f[2].trim().to_string(),
            tact: opt(3)?,
            teff: opt(4)?,
            tg: opt(5)?,
            source: f[6].trim().to_string(),
            note: f[7].trim().to_string(),
        });
    }
    if rows.is_empty() {
        return Err("Table7 公差锚为空".to_string());
    }
    Ok(rows)
}

// ── 直径档列映射（Table 7 上段列 c1..c9）────────────────────────────────────

/// 直径档：`(列序 0..9, 范围标签, 置信)`；`None` = 该侧无实锚，运行期不取。
pub fn range_for_d_b(d_b: f64) -> Option<(usize, &'static str, &'static str)> {
    if !(d_b.is_finite() && d_b > 0.0) {
        return None;
    }
    if d_b > 400.0 {
        None // c1/c2 待核：>400 侧列映射无实锚
    } else if d_b > 200.0 {
        Some((2, "200–400", "推断"))
    } else if d_b > 100.0 {
        Some((3, "100–200", "已核(锚)"))
    } else if d_b > 50.0 {
        Some((4, "50–100", "已核(锚)"))
    } else if d_b > 25.0 {
        Some((5, "25–50", "推断"))
    } else if d_b > 12.0 {
        Some((6, "12–25", "推断"))
    } else {
        Some((7, "≤12", "推断"))
    }
}

/// 按侧取 Table 7 偏差行：内花键按 hub_letter（F..M），外花键按 series（v..a）。
pub fn dev_lookup(letter: &str, hub: bool) -> Option<&'static DevRow> {
    let want = letter.trim();
    table7_rows().iter().find(|r| {
        if hub {
            r.hub_letter.eq_ignore_ascii_case(want)
        } else {
            r.series.eq_ignore_ascii_case(want)
        }
    })
}

/// `letter` 在直径 `d_b` 处的 Table 7 偏差（mm）。
///
/// 返回 `(值 mm, 说明)`；说明写清档位、置信、表值（µm）与缺口原因。
pub fn dev_lookup_mm(letter: &str, d_b: f64, hub: bool) -> Result<(f64, String), String> {
    let Some(row) = dev_lookup(letter, hub) else {
        return Err(format!(
            "Table 7 没有{}偏差系列「{letter}」（{}）",
            if hub { "孔 " } else { "轴 " },
            if hub {
                "孔 F/G/H/J/K/M"
            } else {
                "轴 v/u/t/s/r/p/n/m/k/js/h/g/f/e/d/c/b/a"
            }
        ));
    };
    let Some((ci, label, conf)) = range_for_d_b(d_b) else {
        return Err(format!(
            "Table 7 的 d_B={} 落在 >400 侧（列 c1/c2），该侧列映射无实锚 —— 显示「—」；\
             可用 ae=… / as=… 显式覆盖",
            crate::partgen_kit::trim(d_b)
        ));
    };
    let Some(um) = row.values[ci] else {
        return Err(format!(
            "Table 7 系列「{letter}」在档 {label}（列 c{}）OCR 留空待核 —— 显示「—」",
            ci + 1
        ));
    };
    let cell_flag = row
        .flags
        .contains(&format!("c{}:待核", ci + 1));
    Ok((
        um as f64 / 1000.0,
        format!(
            "Table 7 {letter} 行 × {label}（列 c{}，置信：{conf}）：{um} µm{}{}",
            ci + 1,
            if conf == "已核(锚)" {
                ""
            } else {
                "（范围按表头阶梯推断）"
            },
            if cell_flag { "（该格 OCR 标待核）" } else { "" }
        ),
    ))
}

/// 公差等级 `grade` 在模数 `m` 处的 `(Tact, Teff)`（mm）与说明。
pub fn tol_lookup_mm(grade: u32, m: f64) -> Result<(f64, f64, f64, String), String> {
    let in_group = (1.75..=4.0).contains(&m);
    let Some(row) = table7_tol_rows().iter().find(|r| r.grade == grade) else {
        return Err(format!("Table 7 公差锚没有等级 {grade}"));
    };
    if !in_group {
        return Err(format!(
            "Table 7 公差表只抽到模数组 1,75–4 的锚点（m={} 不在内）—— 显示「—」；\
             可用 tactn/teffn/tactw/teffw 显式覆盖",
            crate::partgen_kit::trim(m)
        ));
    }
    match (row.tact, row.teff, row.tg) {
        (Some(tact), Some(teff), Some(tg)) => Ok((
            tact,
            teff,
            tg,
            format!("Table 7 公差 {grade} 级：Tact={tact}，Teff={teff}，TG={tg}（{}）", row.source),
        )),
        _ => Err(format!(
            "Table 7 公差 {grade} 级未抽取（{}）—— 显示「—」；可用 tactn/teffn/tactw/teffw 显式覆盖",
            row.source
        )),
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 表结构：13 行 × 2 侧（照 Bild 6 字段清单与栏序）
// ══════════════════════════════════════════════════════════════════════════

/// 一行字段的静态定义：`(内花键标签, 内花键 tag, 外花键标签, 外花键 tag, 取值口径, 来源)`。
#[derive(Debug, Clone, Copy)]
pub struct RowSpec {
    /// 内花键（Nabe）标签。
    pub hub_label: &'static str,
    /// 内花键 ATTDEF tag。
    pub hub_tag: &'static str,
    /// 外花键（Welle）标签。
    pub shaft_label: &'static str,
    /// 外花键 ATTDEF tag。
    pub shaft_tag: &'static str,
    /// 公式/口径（预览 title）。
    pub formula: &'static str,
    /// 来源（预览 title）。
    pub source: &'static str,
}

/// 13 行（行序 = Bild 6；行 1 = 标记）。
pub const ROWS: &[RowSpec] = &[
    RowSpec { hub_label: "Nabe 标记", hub_tag: "N标记", shaft_label: "Welle 标记", shaft_tag: "W标记",
              formula: "§8 代号：DIN 5480 – N/W d_B×m×z×等级+偏差字母", source: "DIN 5480-1:2006 §8" },
    RowSpec { hub_label: "齿数 z", hub_tag: "N齿数", shaft_label: "齿数 z", shaft_tag: "W齿数",
              formula: "输入", source: "Bild 6 行 2" },
    RowSpec { hub_label: "模数 m", hub_tag: "N模数", shaft_label: "模数 m", shaft_tag: "W模数",
              formula: "输入", source: "Bild 6 行 3" },
    RowSpec { hub_label: "压力角 α", hub_tag: "N压力角", shaft_label: "压力角 α", shaft_tag: "W压力角",
              formula: "DIN 5480 固定 30°", source: "§5 / Bild 6 行 4" },
    RowSpec { hub_label: "齿根圆 d_f2", hub_tag: "N齿根圆", shaft_label: "齿顶圆 d_a1", shaft_tag: "W齿顶圆",
              formula: "名义表 d_f2 + A_df2；d_a1（齿侧定心 h11）", source: "DIN 5480-2 名义表 + Table 5" },
    RowSpec { hub_label: "齿根成形圆 d_Ff2", hub_tag: "N齿根成形圆", shaft_label: "齿根成形圆 d_Ff1", shaft_tag: "W齿根成形圆",
              formula: "名义表 d_Ff2min（min.）/ d_Ff1max（max.）", source: "DIN 5480-2 名义表" },
    RowSpec { hub_label: "齿顶圆 d_a2", hub_tag: "N齿顶圆", shaft_label: "齿根圆 d_f1", shaft_tag: "W齿根圆",
              formula: "名义表 d_a2（H11）/ d_f1 + A_df1", source: "DIN 5480-2 名义表 + Table 5" },
    RowSpec { hub_label: "槽宽 max. e_max", hub_tag: "N槽宽max", shaft_label: "齿厚 eff. s_vmax", shaft_tag: "W齿厚svmax",
              formula: "emax = e2 + Ae + Tact + Teff；svmax = s1 + As", source: "§10.8 表 6" },
    RowSpec { hub_label: "槽宽 min. e_min", hub_tag: "N槽宽min", shaft_label: "齿厚 max. s_max", shaft_tag: "W齿厚smax",
              formula: "emin = e2 + Ae + Teff（actual Ref.）；smax = s1 + As − Teff", source: "§10.8 表 6" },
    RowSpec { hub_label: "槽宽 eff. e_vmin", hub_tag: "N槽宽eff", shaft_label: "齿厚 min. s_min", shaft_tag: "W齿厚smin",
              formula: "evmin = e2 + Ae；smin = s1 + As − Tact − Teff", source: "§10.8 表 6" },
    RowSpec { hub_label: "量圆 D_M", hub_tag: "N量圆", shaft_label: "量圆 D_M", shaft_tag: "W量圆",
              formula: "DIN 3977 系列（5480-2 检验表）", source: "DIN 5480-2 检验表" },
    RowSpec { hub_label: "量圆距 M2_max", hub_tag: "N量距max", shaft_label: "量圆距 M1_max", shaft_tag: "W量距max",
              formula: "M2max（棒间距）/ M1max Ref.（跨棒距）", source: "DIN 5480-2 检验表 / Bild 6" },
    RowSpec { hub_label: "量圆距 M2_min", hub_tag: "N量距min", shaft_label: "量圆距 M1_min", shaft_tag: "W量距min",
              formula: "M2min Ref. / M1min", source: "DIN 5480-2 检验表 / Bild 6" },
];

/// 26 个 ATTDEF tag（先 13 个内花键、再 13 个外花键；与 [`ROWS`] 同序）。
pub fn value_tags() -> Vec<&'static str> {
    let mut v = Vec::with_capacity(26);
    for r in ROWS {
        v.push(r.hub_tag);
    }
    for r in ROWS {
        v.push(r.shaft_tag);
    }
    v
}

fn common_of(layer: &str) -> EntityCommon {
    let mut c = EntityCommon::default();
    c.layer = layer.to_string();
    c.color = Color::ByLayer;
    c.linetype = "ByLayer".into();
    c
}

/// 标签/标题 MTEXT（左中；标题用正中），字高 [`TEXT_H`] / 标题 [`TITLE_H`]，样式一律 `OCSM_GB`。
fn mtext_ent(value: &str, x: f64, y: f64, height: f64, attach: i16) -> EntityType {
    let mut m = MText::new();
    m.value = value.to_string();
    m.insertion_point = Vector3::new(x, y, 0.0);
    m.height = height;
    m.rectangle_width = W_SIDE - 2.0 * LABEL_X_HUB;
    m.style = "OCSM_GB".into();
    m.attachment_point = match attach {
        5 => AttachmentPoint::MiddleCenter,
        _ => AttachmentPoint::MiddleLeft,
    };
    m.common = common_of("6文字层");
    EntityType::MText(m)
}

/// 值/标记 ATTDEF：左中，实体级字宽 [`VALUE_W_FACTOR`]，样式一律 `OCSM_GB`。
fn value_attdef(tag: &str, x: f64, y: f64, width: f64) -> AttributeDefinition {
    let mut ad = AttributeDefinition::new(tag.to_string(), String::new(), " ".to_string());
    ad.insertion_point = Vector3::new(x, y, 0.0);
    ad.alignment_point = ad.insertion_point;
    ad.height = TEXT_H;
    ad.width_factor = VALUE_W_FACTOR;
    ad.text_style = "OCSM_GB".into();
    ad.horizontal_alignment = HorizontalAlignment::Left;
    ad.vertical_alignment = VerticalAlignment::Middle;
    ad.flags.preset = true;
    ad.common = common_of("6文字层");
    let _ = width;
    ad
}

/// 13 行 × 2 侧的标签 MTEXT（静态）+ 标题；线条另见 [`block_entities`]。
fn labels() -> Vec<EntityType> {
    let mut out = Vec::with_capacity(25);
    out.push(mtext_ent("DIN 5480 花键参数表", W_TOTAL / 2.0, TITLE_Y, TITLE_H, 5));
    for (i, r) in ROWS.iter().enumerate() {
        // 行 1（标记行）不另画标签：两侧标记本身就是该行内容（照 Bild 6）。
        if i == 0 {
            continue;
        }
        let y = data_row_y(i - 1);
        out.push(mtext_ent(r.hub_label, LABEL_X_HUB, y, TEXT_H, 4));
        out.push(mtext_ent(r.shaft_label, LABEL_X_SHAFT, y, TEXT_H, 4));
    }
    out
}

/// 26 个 ATTDEF（先 13 个内花键、再 13 个外花键；与 [`values`] 同序）。
pub fn attdefs() -> Vec<AttributeDefinition> {
    let mut out = Vec::with_capacity(26);
    // 标记行：每侧的标记横跨标签+值两列。
    out.push(value_attdef(ROWS[0].hub_tag, LABEL_X_HUB, DESIG_Y, W_SIDE - 2.0 * LABEL_X_HUB));
    for i in 1..ROWS.len() {
        let y = data_row_y(i - 1);
        out.push(value_attdef(
            ROWS[i].hub_tag,
            VALUE_X_HUB,
            y,
            W_SIDE - W_LABEL - 2.0 * 9.0,
        ));
    }
    out.push(value_attdef(ROWS[0].shaft_tag, LABEL_X_SHAFT, DESIG_Y, W_SIDE - 2.0 * LABEL_X_HUB));
    for i in 1..ROWS.len() {
        let y = data_row_y(i - 1);
        out.push(value_attdef(
            ROWS[i].shaft_tag,
            VALUE_X_SHAFT,
            y,
            W_SIDE - W_LABEL - 2.0 * 9.0,
        ));
    }
    out
}

/// 表格块成员：20 横/竖线 + 25 MTEXT（标题 + 24 标签）+ 26 ATTDEF。
///
/// 版式（y 向上为正，表从 y=0 向下）：
/// * 标题行 `[0, −44]`（全宽）；标记行 `[−44, −104]`（标记自身即行内容，不另画标签）；
///   12 数据行各 40，底 `y=−584`；
/// * 竖线：全高 `x=0, 310, 620`；标签/值分格 `x=195, 505`（自 −44 起）。
pub fn block_entities() -> Vec<EntityType> {
    let mut out = Vec::new();
    // 横线：0、-44、-104，随后 12 条行底（-144 … -584）。
    let mut ys = vec![0.0, -H_TITLE, -(H_TITLE + H_DESIG)];
    for i in 1..=12 {
        ys.push(-(H_TITLE + H_DESIG) - i as f64 * H_ROW);
    }
    for y in &ys {
        out.push(crate::partgen_kit::line([0.0, *y], [W_TOTAL, *y], "1轮廓实线层"));
    }
    // 全高竖线：外框 + 中分隔（Nabe/Welle）。
    for x in [0.0, W_SIDE, W_TOTAL] {
        out.push(crate::partgen_kit::line(
            [x, 0.0],
            [x, -(H_TITLE + H_DESIG + 12.0 * H_ROW)],
            "1轮廓实线层",
        ));
    }
    // 标签/值分格竖线（标题行与标记行不切）。
    for x in [W_LABEL, W_SIDE + W_LABEL] {
        out.push(crate::partgen_kit::line(
            [x, -H_TITLE],
            [x, -(H_TITLE + H_DESIG + 12.0 * H_ROW)],
            "2细线层",
        ));
    }
    for e in labels() {
        out.push(e);
    }
    for ad in attdefs() {
        out.push(EntityType::AttributeDefinition(ad));
    }
    out
}

// ══════════════════════════════════════════════════════════════════════════
// 输入模型 / 解析
// ══════════════════════════════════════════════════════════════════════════

/// 一个配合（等级 + 偏差系列字母）。
#[derive(Debug, Clone, PartialEq)]
pub struct DinFit {
    /// 公差等级（1–12；DIN 5480 常用 7/8/9/10/11）。
    pub grade: u32,
    /// 偏差系列字母（孔 F/G/H/J/K/M；轴 v…a）。
    pub letter: String,
}

impl DinFit {
    /// 代号里的 `9H` / `8f` 写法。
    pub fn token(&self) -> String {
        format!("{}{}", self.grade, self.letter)
    }
    /// 校验字母是否属该侧（`hub=true` 孔）。
    pub fn validate(&self, hub: bool) -> Result<(), String> {
        const HUB: &[&str] = &["F", "G", "H", "J", "K", "M"];
        const SHAFT: &[&str] = &[
            "v", "u", "t", "s", "r", "p", "n", "m", "k", "js", "h", "g", "f", "e", "d", "c", "b",
            "a",
        ];
        if !(1..=12).contains(&self.grade) {
            return Err(format!("公差等级 {} 超出范围（1–12）", self.grade));
        }
        let ok = if hub {
            HUB.iter().any(|x| x.eq_ignore_ascii_case(&self.letter))
        } else {
            SHAFT.iter().any(|x| *x == self.letter.to_ascii_lowercase())
        };
        if !ok {
            return Err(format!(
                "{}偏差系列「{}」非法（{}）",
                if hub { "孔 " } else { "轴 " },
                self.letter,
                if hub {
                    "F/G/H/J/K/M（DIN 5480 §10.3 六个）"
                } else {
                    "v/u/t/s/r/p/n/m/k/js/h/g/f/e/d/c/b/a（十八个）"
                }
            ));
        }
        Ok(())
    }
}

impl Default for DinFit {
    fn default() -> Self {
        DinFit { grade: 9, letter: "H".into() }
    }
}

/// DIN 花键参数表卡参数（CLI 与 GUI 同一份字段）。
#[derive(Debug, Clone, PartialEq)]
pub struct DinTableSpec {
    /// 模数 m。
    pub m: f64,
    /// 齿数 z。
    pub z: u32,
    /// 基准直径 d_B。
    pub d_b: f64,
    /// 内花键配合（缺省 9H）。
    pub hub: DinFit,
    /// 外花键配合（缺省 8f）。
    pub shaft: DinFit,
    /// `e₂=s₁` 覆盖（mm）。
    pub e2_s1: Option<f64>,
    /// `Ae` 覆盖（mm）。
    pub ae: Option<f64>,
    /// `As` 覆盖（mm）。
    pub as_: Option<f64>,
    /// `Tact`（孔）覆盖。
    pub tact_hub: Option<f64>,
    /// `Teff`（孔）覆盖。
    pub teff_hub: Option<f64>,
    /// `Tact`（轴）覆盖。
    pub tact_shaft: Option<f64>,
    /// `Teff`（轴）覆盖。
    pub teff_shaft: Option<f64>,
    pub at: Option<[f64; 2]>,
    pub rot: f64,
}

impl Default for DinTableSpec {
    fn default() -> Self {
        DinTableSpec {
            m: 3.0,
            z: 38,
            d_b: 120.0,
            hub: DinFit::default(),
            shaft: DinFit { grade: 8, letter: "f".into() },
            e2_s1: None,
            ae: None,
            as_: None,
            tact_hub: None,
            teff_hub: None,
            tact_shaft: None,
            teff_shaft: None,
            at: None,
            rot: 0.0,
        }
    }
}

fn validate_basic(m: f64, z: u32, d_b: f64) -> Result<(), String> {
    if !(m.is_finite() && m > 0.0) {
        return Err(format!("DIN 花键参数表：模数 m={m} 必须是正数"));
    }
    if !(3..=1000).contains(&z) {
        return Err(format!("DIN 花键参数表：齿数 z={z} 超出范围（3–1000）"));
    }
    if !(d_b.is_finite() && d_b > 0.0) {
        return Err(format!("DIN 花键参数表：基准直径 d_B={d_b} 必须是正数"));
    }
    Ok(())
}

/// 是否 Bild 6 示例（N120×3×38×9H / W120×3×38×8f） —— 是则整表照标准原印值。
pub fn is_bild6_example(spec: &DinTableSpec) -> bool {
    (spec.m - 3.0).abs() < 1e-9
        && spec.z == 38
        && (spec.d_b - 120.0).abs() < 1e-3
        && spec.hub.grade == 9
        && spec.hub.letter.eq_ignore_ascii_case("H")
        && spec.shaft.grade == 8
        && spec.shaft.letter == "f"
        && spec.e2_s1.is_none()
        && spec.ae.is_none()
        && spec.as_.is_none()
        && spec.tact_hub.is_none()
        && spec.teff_hub.is_none()
        && spec.tact_shaft.is_none()
        && spec.teff_shaft.is_none()
}

/// 名义表命中行（(m, d_B, z) 精确；只取本卡需要的字段）。
#[derive(Debug, Clone, PartialEq)]
pub struct DinNominal {
    /// 公称齿槽宽/齿厚 `e₂=s₁`。
    pub e2_s1: f64,
    /// 内花键齿根圆 d_f2 与偏差 A_df2。
    pub d_f2: f64,
    pub a_df2: f64,
    /// 内花键齿根成形圆 min。
    pub d_ff2_min: f64,
    /// 内花键齿顶圆。
    pub d_a2: f64,
    /// 外花键齿顶圆。
    pub d_a1: f64,
    /// 外花键齿根成形圆 max。
    pub d_ff1_max: f64,
    /// 外花键齿根圆与偏差 A_df1。
    pub d_f1: f64,
    pub a_df1: f64,
    /// 行来源说明。
    pub source: String,
}

/// 查名义表 `(m, d_B, z)`（同 d_B 多 z 是表事实，必须带 z）。
pub fn nominal_lookup(m: f64, z: u32, d_b: f64) -> Option<DinNominal> {
    crate::invol_spline::din5480_rows()
        .iter()
        .find(|r| {
            (r.m - m).abs() < 1e-9 && (r.d_b - d_b).abs() < 1e-3 && r.z == z
        })
        .map(|r| DinNominal {
            e2_s1: r.e2_s1,
            d_f2: r.d_f2,
            a_df2: r.a_df2,
            d_ff2_min: r.d_ff2_min,
            d_a2: r.d_a2,
            d_a1: r.d_a1,
            d_ff1_max: r.d_ff1_max,
            d_f1: r.d_f1,
            a_df1: r.a_df1,
            source: format!(
                "DIN 5480-2 名义表 m={} d_B={} z={}（page={} 表{}，source={}）",
                crate::partgen_kit::trim(r.m),
                crate::partgen_kit::trim(r.d_b),
                r.z,
                r.page,
                r.table_no,
                r.source,
            ),
        })
}

/// 检验表命中行（(m, d_B, z)）。
#[derive(Debug, Clone, PartialEq)]
pub struct DinInspection {
    /// 内花键量棒直径与棒间距。
    pub d_m_hub: f64,
    pub m2: f64,
    /// 外花键量棒直径与跨棒距。
    pub d_m_shaft: f64,
    pub m1: f64,
    /// 行来源说明。
    pub source: String,
}

/// 查检验表 `(m, d_B, z)`。
pub fn inspection_lookup(m: f64, z: u32, d_b: f64) -> Option<DinInspection> {
    crate::invol_spline::inspection_rows()
        .iter()
        .find(|r| (r.m - m).abs() < 1e-9 && (r.d_b - d_b).abs() < 1e-3 && r.z == z)
        .map(|r| DinInspection {
            d_m_hub: r.d_m_hub,
            m2: r.m2,
            d_m_shaft: r.d_m_shaft,
            m1: r.m1,
            source: r.source_note(),
        })
}

/// 求值结果（测试/预览/落图共用）。
#[derive(Debug, Clone, PartialEq)]
pub struct DinDerived {
    /// 名义表行（查不到 = None，直径与 e₂ 显示「—」）。
    pub nominal: Option<DinNominal>,
    /// `e₂=s₁`（mm）。
    pub e2_s1: Option<f64>,
    /// 内花键 Ae（mm）与说明。
    pub ae: Option<f64>,
    pub ae_note: String,
    /// 外花键 As（mm）与说明。
    pub as_: Option<f64>,
    pub as_note: String,
    /// 孔 9H 的 Tact/Teff（mm）与说明。
    pub tact_hub: Option<f64>,
    pub teff_hub: Option<f64>,
    pub tol_hub_note: String,
    /// 轴 8f 的 Tact/Teff。
    pub tact_shaft: Option<f64>,
    pub teff_shaft: Option<f64>,
    pub tol_shaft_note: String,
    /// e/s 六值（mm）。
    pub e_max: Option<f64>,
    pub e_min: Option<f64>,
    pub e_vmin: Option<f64>,
    pub s_vmax: Option<f64>,
    pub s_max: Option<f64>,
    pub s_min: Option<f64>,
    /// D_M / M2 / M1（mm）。
    pub d_m_hub: Option<f64>,
    pub m2_max: Option<f64>,
    pub m2_min: Option<f64>,
    pub d_m_shaft: Option<f64>,
    pub m1_max: Option<f64>,
    pub m1_min: Option<f64>,
    /// 是否 Bild 6 示例路径（整表照原印值）。
    pub anchor: bool,
    /// 数据缺口说明（进预览 missing_note/回执）。
    pub notes: Vec<String>,
}

/// 三数相加（任一 None → None）。
fn add3(a: Option<f64>, b: Option<f64>, c: Option<f64>) -> Option<f64> {
    Some(a? + b? + c?)
}

fn sub2(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    Some(a? - b?)
}

/// 求值：名义表 + Table 7（或显式覆盖）+ 表 6 公式；Bild 6 示例走锚点并保持公式一致。
pub fn derive(spec: &DinTableSpec) -> Result<DinDerived, String> {
    validate_basic(spec.m, spec.z, spec.d_b)?;
    spec.hub.validate(true).map_err(|e| format!("DIN 花键参数表：{e}"))?;
    spec.shaft.validate(false).map_err(|e| format!("DIN 花键参数表：{e}"))?;
    let nominal = nominal_lookup(spec.m, spec.z, spec.d_b);
    let e2 = spec.e2_s1.or(nominal.as_ref().map(|n| n.e2_s1));
    let mut notes = Vec::new();

    // Ae / As：显式覆盖 > Table 7 查表 > 「—」。
    let mut ae_note;
    let mut as_note;
    let ae = match spec.ae {
        Some(v) => {
            ae_note = format!("显式输入 Ae={}", trim3(v));
            Some(v)
        }
        None => match dev_lookup_mm(&spec.hub.letter, spec.d_b, true) {
            Ok((v, n)) => {
                ae_note = n;
                Some(v)
            }
            Err(e) => {
                ae_note = e.clone();
                notes.push(format!("Ae：{e}"));
                None
            }
        },
    };
    let as_ = match spec.as_ {
        Some(v) => {
            as_note = format!("显式输入 As={}", trim3(v));
            Some(v)
        }
        None => match dev_lookup_mm(&spec.shaft.letter, spec.d_b, false) {
            Ok((v, n)) => {
                as_note = n;
                Some(v)
            }
            Err(e) => {
                as_note = e.clone();
                notes.push(format!("As：{e}"));
                None
            }
        },
    };

    // Tact / Teff：显式覆盖 > Table 7 公差锚 > 「—」。
    let mut resolve_tol = |grade: u32,
                           tact_ov: Option<f64>,
                           teff_ov: Option<f64>,
                           who: &str|
     -> (Option<f64>, Option<f64>, String) {
        if let (Some(tact), Some(teff)) = (tact_ov, teff_ov) {
            return (
                Some(tact),
                Some(teff),
                format!("显式输入 {who} Tact={}、Teff={}", trim3(tact), trim3(teff)),
            );
        }
        match tol_lookup_mm(grade, spec.m) {
            Ok((tact, teff, _tg, n)) => (
                tact_ov.or(Some(tact)),
                teff_ov.or(Some(teff)),
                n,
            ),
            Err(e) => {
                if tact_ov.is_none() && teff_ov.is_none() {
                    notes.push(format!("{who} 公差：{e}"));
                }
                (tact_ov, teff_ov, e)
            }
        }
    };
    let (tact_hub, teff_hub, tol_hub_note) =
        resolve_tol(spec.hub.grade, spec.tact_hub, spec.teff_hub, "孔");
    let (tact_shaft, teff_shaft, tol_shaft_note) =
        resolve_tol(spec.shaft.grade, spec.tact_shaft, spec.teff_shaft, "轴");

    let anchor = is_bild6_example(spec);
    let (mut d_m_hub, mut m2_max, mut m2_min, mut d_m_shaft, mut m1_max, mut m1_min) =
        (None, None, None, None, None, None);
    if anchor {
        d_m_hub = anchor_f64("hub", "D_M");
        m2_max = anchor_f64("hub", "M2_max");
        m2_min = anchor_f64("hub", "M2_min");
        d_m_shaft = anchor_f64("shaft", "D_M");
        m1_max = anchor_f64("shaft", "M1_max");
        m1_min = anchor_f64("shaft", "M1_min");
    } else if let Some(insp) = inspection_lookup(spec.m, spec.z, spec.d_b) {
        d_m_hub = Some(insp.d_m_hub);
        m2_max = Some(insp.m2);
        m2_min = Some(insp.m2);
        d_m_shaft = Some(insp.d_m_shaft);
        m1_max = Some(insp.m1);
        m1_min = Some(insp.m1);
        notes.push(format!(
            "M2/M1 取检验表名义值（{}）：该表只给基本偏差行，max/min 需按 A*_M 系数换算，本卡如实给名义值",
            insp.source
        ));
    } else {
        notes.push(format!(
            "D_M/M2/M1：检验表没有 m={} d_B={} z={} 行（已入库档位见 din5480_2_notes.md）—— 显示「—」",
            crate::partgen_kit::trim(spec.m),
            crate::partgen_kit::trim(spec.d_b),
            spec.z
        ));
    }

    // 表 6 六值。
    let (e_max, e_min, e_vmin, s_vmax, s_max, s_min) = if anchor {
        (
            anchor_f64("hub", "e_max"),
            anchor_f64("hub", "e_min"),
            anchor_f64("hub", "e_vmin"),
            anchor_f64("shaft", "s_vmax"),
            anchor_f64("shaft", "s_max"),
            anchor_f64("shaft", "s_min"),
        )
    } else {
        let ev = e2.zip(ae).map(|(a, b)| a + b);
        let emin = add3(e2, ae, teff_hub);
        let emax = emin.zip(tact_hub).map(|(a, b)| a + b);
        let sv = e2.zip(as_).map(|(a, b)| a + b);
        let smax = sub2(sv, teff_shaft);
        let smin = sv
            .zip(tact_shaft)
            .zip(teff_shaft)
            .map(|((a, b), c)| a - b - c);
        (emax, emin, ev, sv, smax, smin)
    };

    Ok(DinDerived {
        nominal,
        e2_s1: e2,
        ae,
        ae_note,
        as_,
        as_note,
        tact_hub,
        teff_hub,
        tol_hub_note,
        tact_shaft,
        teff_shaft,
        tol_shaft_note,
        e_max,
        e_min,
        e_vmin,
        s_vmax,
        s_max,
        s_min,
        d_m_hub,
        m2_max,
        m2_min,
        d_m_shaft,
        m1_max,
        m1_min,
        anchor,
        notes,
    })
}

/// 3 位小数（显示与内部计算分开）；`None` → 「—」。
pub fn fmt_mm(v: Option<f64>) -> String {
    v.map(|v| trim3(v)).unwrap_or_else(|| MISSING.to_string())
}

/// 最多 3 位小数。
pub fn trim3(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// 带符号偏差显示：`+0.76` / `-1.15`。
pub fn fmt_dev(v: f64) -> String {
    if v >= 0.0 {
        format!("+{}", trim3(v))
    } else {
        trim3(v)
    }
}

/// 代号（§8）：`Nabe DIN 5480 – N d_B×m×z×等级+字母` / `Welle …`。
pub fn designation(spec: &DinTableSpec, hub: bool) -> String {
    format!(
        "{} DIN 5480 – {}{}×{}×{}×{}",
        if hub { "Nabe" } else { "Welle" },
        if hub { "N" } else { "W" },
        trim3(spec.d_b),
        trim3(spec.m),
        spec.z,
        if hub { spec.hub.token() } else { spec.shaft.token() }
    )
}

/// 26 项取值（顺序 = [`attdefs()`]：13 孔 + 13 轴）。
pub fn values(spec: &DinTableSpec) -> Result<Vec<(String, String)>, String> {
    let d = derive(spec)?;
    let n = d.nominal.as_ref();
    let mut out: Vec<(&'static str, String)> = Vec::with_capacity(26);
    // 孔
    out.push(("N标记", designation(spec, true)));
    out.push(("N齿数", spec.z.to_string()));
    out.push(("N模数", trim3(spec.m)));
    out.push(("N压力角", "30°".to_string()));
    out.push((
        "N齿根圆",
        if d.anchor {
            anchor_display("hub", "d_f2").unwrap_or_else(|| MISSING.to_string())
        } else {
            n.map(|r| format!("{} {}", trim3(r.d_f2), fmt_dev(r.a_df2)))
                .unwrap_or_else(|| MISSING.to_string())
        },
    ));
    out.push((
        "N齿根成形圆",
        if d.anchor {
            anchor_display("hub", "d_Ff2").unwrap_or_else(|| MISSING.to_string())
        } else {
            n.map(|r| format!("{} min.", trim3(r.d_ff2_min)))
                .unwrap_or_else(|| MISSING.to_string())
        },
    ));
    out.push((
        "N齿顶圆",
        if d.anchor {
            anchor_display("hub", "d_a2").unwrap_or_else(|| MISSING.to_string())
        } else {
            n.map(|r| format!("{} H11", trim3(r.d_a2)))
                .unwrap_or_else(|| MISSING.to_string())
        },
    ));
    out.push(("N槽宽max", fmt_mm(d.e_max)));
    out.push(("N槽宽min", fmt_mm(d.e_min)));
    out.push(("N槽宽eff", fmt_mm(d.e_vmin)));
    out.push(("N量圆", fmt_mm(d.d_m_hub)));
    out.push(("N量距max", fmt_mm(d.m2_max)));
    out.push(("N量距min", fmt_mm(d.m2_min)));
    // 轴
    out.push(("W标记", designation(spec, false)));
    out.push(("W齿数", spec.z.to_string()));
    out.push(("W模数", trim3(spec.m)));
    out.push(("W压力角", "30°".to_string()));
    out.push((
        "W齿顶圆",
        if d.anchor {
            anchor_display("shaft", "d_a1").unwrap_or_else(|| MISSING.to_string())
        } else {
            n.map(|r| format!("{} h11", trim3(r.d_a1)))
                .unwrap_or_else(|| MISSING.to_string())
        },
    ));
    out.push((
        "W齿根成形圆",
        if d.anchor {
            anchor_display("shaft", "d_Ff1").unwrap_or_else(|| MISSING.to_string())
        } else {
            n.map(|r| format!("{} max.", trim3(r.d_ff1_max)))
                .unwrap_or_else(|| MISSING.to_string())
        },
    ));
    out.push((
        "W齿根圆",
        if d.anchor {
            anchor_display("shaft", "d_f1").unwrap_or_else(|| MISSING.to_string())
        } else {
            n.map(|r| format!("{} {}", trim3(r.d_f1), fmt_dev(r.a_df1)))
                .unwrap_or_else(|| MISSING.to_string())
        },
    ));
    out.push(("W齿厚svmax", fmt_mm(d.s_vmax)));
    out.push(("W齿厚smax", fmt_mm(d.s_max)));
    out.push(("W齿厚smin", fmt_mm(d.s_min)));
    out.push(("W量圆", fmt_mm(d.d_m_shaft)));
    out.push(("W量距max", fmt_mm(d.m1_max)));
    out.push(("W量距min", fmt_mm(d.m1_min)));

    // 顺序护栏：取值顺序必须与 ATTDEF 表一致（加/改行时先在这里暴露）。
    let got: Vec<&str> = out.iter().map(|(t, _)| *t).collect();
    let want: Vec<String> = attdefs().iter().map(|ad| ad.tag.clone()).collect();
    let want: Vec<&str> = want.iter().map(|s| s.as_str()).collect();
    if got != want {
        return Err(format!(
            "DIN 花键参数表：取值映射顺序与模板属性不一致：{got:?} != {want:?}"
        ));
    }
    Ok(out.into_iter().map(|(t, v)| (t.to_string(), v)).collect())
}

/// 建 INSERT（基点在 `at`，旋转 `rot_deg` 度；26 个 ATTRIB 取自 `values()`）。
pub fn build_insert(spec: &DinTableSpec, at: [f64; 2], rot_deg: f64) -> Result<Insert, String> {
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
        tmpl.rotation = 0.0;
        let mut attr = AttributeEntity::from_definition(&tmpl, Some(val));
        attr.apply_transform(&ins.get_transform());
        ins.attributes.push(attr);
    }
    Ok(ins)
}

/// 26 项 Markdown（回执/计算书）。
pub fn markdown_table(spec: &DinTableSpec) -> Result<String, String> {
    let vals = values(spec)?;
    let mut md = String::new();
    md.push_str("| 属性 | 值 |\n|---|---|\n");
    for (tag, v) in vals {
        md.push_str(&format!("| {tag} | {v} |\n"));
    }
    Ok(md)
}

// ══════════════════════════════════════════════════════════════════════════
// GUI 选项表与 GUI/HTTP 模型
// ══════════════════════════════════════════════════════════════════════════

/// DIN 卡的表单字段（表驱动 GUI 骨架；Bild 6 示例作默认）。
/// DIN 卡的表达式策略（表驱动；体系要求 SPLINE、α=30、直齿；变位进 d_B 公式；
/// 本卡 Nabe/Welle 两栏都做，方向不收窄）。
pub const EXPR_POLICY: crate::card_expr::ExprPolicy = crate::card_expr::ExprPolicy {
    card: "DIN 花键参数表",
    mark: crate::card_expr::ExprMark::Spline,
    alphas: &[30.0],
    spur: true,
    allow_shift: true,
    map: &[
        crate::card_expr::ExprRule {
            target: "d_b",
            label: "基准直径 d_B",
            op: crate::card_expr::ExprOp::DinBaseDB,
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
            placeholder: "SPLINE IN M3 Z38 ALPHA30 X0.45 BETA0 H30",
            default: "",
            title: "九字段统一齿形表达式（MARK KIND M Z ALPHA X DA DF BETA H）；粘贴后自动反解 d_B=m(z+1.1+2x)、m、z；DIN 5480 压力角恒 30°",
            options: &[],
            options_from: "",
            min: 0.0,
            step: 0.0,
            required: false,
        },
        crate::card::CardFieldSpec { key: "m", label: "模数 m", kind: "number",
            placeholder: "如 3", default: "3",
            title: "DIN 5480-1 模数 m（Bild 6 主参数）", options: &[], options_from: "",
            min: 0.0, step: 0.001, required: true },
        crate::card::CardFieldSpec { key: "z", label: "齿数 z", kind: "number",
            placeholder: "如 38", default: "38",
            title: "齿数 z", options: &[], options_from: "",
            min: 3.0, step: 1.0, required: true },
        crate::card::CardFieldSpec { key: "d_b", label: "基准直径 d_B", kind: "number",
            placeholder: "如 120", default: "120",
            title: "基准直径 d_B = m·z（DIN 5480）", options: &[], options_from: "",
            min: 0.0, step: 0.001, required: true },
        crate::card::CardFieldSpec { key: "hub", label: "内花键配合", kind: "text",
            placeholder: "如 9H（F/G/H/J/K/M）", default: "9H",
            title: "Nabe 配合（字母 F/G/H/J/K/M + 等级数字；Bild 6 示例 9H）", options: &[], options_from: "",
            min: 0.0, step: 0.0, required: false },
        crate::card::CardFieldSpec { key: "shaft", label: "外花键配合", kind: "text",
            placeholder: "如 8f（v…a）", default: "8f",
            title: "Welle 配合（字母 v…a + 等级数字；Bild 6 示例 8f）", options: &[], options_from: "",
            min: 0.0, step: 0.0, required: false },
        crate::card::CardFieldSpec { key: "e2", label: "e₂=s₁ 覆盖", kind: "number",
            placeholder: "选填（名义表缺行时）", default: "",
            title: "e₂=s₁ 名义值显式覆盖（mm；缺行时用）", options: &[], options_from: "",
            min: 0.0, step: 0.001, required: false },
        crate::card::CardFieldSpec { key: "ae", label: "Ae 覆盖", kind: "number",
            placeholder: "选填，如 0", default: "",
            title: "Ae（齿槽宽上偏差）显式覆盖（mm）", options: &[], options_from: "",
            min: 0.0, step: 0.001, required: false },
        crate::card::CardFieldSpec { key: "as_", label: "As 覆盖", kind: "number",
            placeholder: "选填，如 -0.028", default: "",
            title: "As（齿厚上偏差）显式覆盖（mm）", options: &[], options_from: "",
            min: 0.0, step: 0.001, required: false },
        crate::card::CardFieldSpec { key: "tact_n", label: "Tact(N) 覆盖", kind: "number",
            placeholder: "选填", default: "",
            title: "Nabe 实际齿槽宽公差显式覆盖（mm）", options: &[], options_from: "",
            min: 0.0, step: 0.0001, required: false },
        crate::card::CardFieldSpec { key: "teff_n", label: "Teff(N) 覆盖", kind: "number",
            placeholder: "选填", default: "",
            title: "Nabe 作用齿槽宽公差显式覆盖（mm）", options: &[], options_from: "",
            min: 0.0, step: 0.0001, required: false },
        crate::card::CardFieldSpec { key: "tact_w", label: "Tact(W) 覆盖", kind: "number",
            placeholder: "选填", default: "",
            title: "Welle 实际齿厚公差显式覆盖（mm）", options: &[], options_from: "",
            min: 0.0, step: 0.0001, required: false },
        crate::card::CardFieldSpec { key: "teff_w", label: "Teff(W) 覆盖", kind: "number",
            placeholder: "选填", default: "",
            title: "Welle 作用齿厚公差显式覆盖（mm）", options: &[], options_from: "",
            min: 0.0, step: 0.0001, required: false },
    ],
    note: "粘九字段表达式（自动反解 d_B/m/z）或直接填 m/z/d_B + N/W 配合 → 缺行可用 e₂ / Ae / As / Tact / Teff 覆盖 → 点「出表」回到图纸放置。",
    missing_note: "Table 7 上段 c1/c2（>400 侧）与 c9（≤12 细档）列映射无实锚 → 「—」；\n                   下段公差表只抽到 6–9 级、模数组 1,75–4 的实锚 → 其余等级/模数组 Tact/Teff 显示「—」；\n                   D_M/M2/M1 无检验表行且非 Bild 6 示例时显示「—」。",
};

/// DIN 卡选项/口径 JSON（随 `/api/spline_options` 下发；页面只渲染）。
pub fn options_json() -> serde_json::Value {
    serde_json::json!({
        "columns": ROWS.iter().enumerate().flat_map(|(i, r)| {
            let (label, tag, unit, source) = if i == 0 {
                (r.hub_label, r.hub_tag, "", r.source)
            } else {
                (r.hub_label, r.hub_tag, "mm", r.source)
            };
            let _ = source;
            vec![
                serde_json::json!({"tag": tag, "label": label, "unit": unit,
                                   "formula": r.formula, "source": r.source}),
                serde_json::json!({"tag": r.shaft_tag, "label": r.shaft_label, "unit": unit,
                                   "formula": r.formula, "source": r.source}),
            ]
        }).collect::<Vec<_>>(),
        "hub_letters": ["F","G","H","J","K","M"],
        "shaft_letters": ["v","u","t","s","r","p","n","m","k","js","h","g","f","e","d","c","b","a"],
        "grades": [1,2,3,4,5,6,7,8,9,10,11,12],
        "default": {"m": 3.0, "z": 38, "d_b": 120.0, "hub": "9H", "shaft": "8f"},
        "example": {
            "m": 3.0, "z": 38, "d_b": 120.0, "hub": "9H", "shaft": "8f",
            "note": "DIN 5480-1:2006 Bild 6 原示例（N120×3×38×9H / W120×3×38×8f）—— 整表照标准原印值"
        },
        "missing_note": "Table 7 上段 c1/c2（>400 侧）与 c9（≤12 细档）列映射无实锚 → 「—」；\
                         下段公差表只抽到 6–9 级、模数组 1,75–4 的实锚 → 其余等级/模数组 Tact/Teff 显示「—」；\
                         D_M/M2/M1 无检验表行且非 Bild 6 示例时显示「—」；可用 ae/as/e2/tactn/teffn/tactw/teffw 显式覆盖。",
        "note": "版面照 DIN 5480-1:2006 Bild 6（13 行 × Nabe/Welle 两栏）；文字样式一律 OCSM_GB；\
                 Table 7 上段数值为 2026-09-26 双源 OCR + 逐格复核入库（assets/din5480_1_table7_dev.csv）。",
    })
}

/// CLI/HTTP 表单模型（字段与 GUI 控件一一对应）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DinTableModel {
    /// 卡类型（GUI 回传；后端按 renderer 分派，这里只记不看）。
    #[serde(default)]
    pub card: String,
    /// 九字段统一齿形表达式（可空；给了则覆盖 m/z/d_B）。
    #[serde(default)]
    pub expr: Option<String>,
    /// 模数 m。
    pub m: f64,
    /// 齿数 z。
    pub z: u32,
    /// 基准直径 d_B。
    pub d_b: f64,
    /// 内花键配合（`9H`）。
    #[serde(default)]
    pub hub: Option<String>,
    /// 外花键配合（`8f`）。
    #[serde(default)]
    pub shaft: Option<String>,
    /// 覆盖值。
    #[serde(default)]
    pub e2: Option<f64>,
    #[serde(default)]
    pub ae: Option<f64>,
    #[serde(default)]
    pub as_: Option<f64>,
    #[serde(default)]
    pub tact_n: Option<f64>,
    #[serde(default)]
    pub teff_n: Option<f64>,
    #[serde(default)]
    pub tact_w: Option<f64>,
    #[serde(default)]
    pub teff_w: Option<f64>,
    #[serde(default)]
    pub at: Option<[f64; 2]>,
    #[serde(default)]
    pub rot: f64,
}

impl DinTableModel {
    /// →（校验过的 `DinTableSpec`）。
    pub fn spec(&self) -> Result<DinTableSpec, String> {
        let hub = parse_fit_token(self.hub.as_deref().unwrap_or("9H"), true)
            .map_err(|e| format!("DIN 花键参数表：孔配合「{}」{e}", self.hub.as_deref().unwrap_or("9H")))?;
        let shaft = parse_fit_token(self.shaft.as_deref().unwrap_or("8f"), false)
            .map_err(|e| format!("DIN 花键参数表：轴配合「{}」{e}", self.shaft.as_deref().unwrap_or("8f")))?;
        let (m, z, d_b) = match self.expr.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(e) => {
                let r = crate::card_expr::resolve(&EXPR_POLICY, e, None)?;
                (
                    r.value("m")
                        .ok_or_else(|| "DIN 花键参数表：表达式映射表缺 m（内部错误）".to_string())?,
                    r.value("z")
                        .ok_or_else(|| "DIN 花键参数表：表达式映射表缺 z（内部错误）".to_string())?
                        as u32,
                    r.value("d_b")
                        .ok_or_else(|| {
                            "DIN 花键参数表：表达式映射表缺 d_B（内部错误）".to_string()
                        })?,
                )
            }
            None => (self.m, self.z, self.d_b),
        };
        let spec = DinTableSpec {
            m,
            z,
            d_b,
            hub,
            shaft,
            e2_s1: self.e2,
            ae: self.ae,
            as_: self.as_,
            tact_hub: self.tact_n,
            teff_hub: self.teff_n,
            tact_shaft: self.tact_w,
            teff_shaft: self.teff_w,
            at: self.at,
            rot: self.rot,
        };
        derive(&spec)?;
        Ok(spec)
    }

    /// 预览 JSON（不碰图纸）。
    pub fn preview_json(&self) -> Result<serde_json::Value, String> {
        let spec = self.spec()?;
        let d = derive(&spec)?;
        let vals = values(&spec)?;
        let mut items = Vec::with_capacity(ROWS.len() * 2);
        let mut missing = Vec::new();
        for (i, r) in ROWS.iter().enumerate() {
            for (tag, label) in [(r.hub_tag, r.hub_label), (r.shaft_tag, r.shaft_label)] {
                let value = vals
                    .iter()
                    .find(|(t, _)| t == tag)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                if value == MISSING {
                    missing.push(label.to_string());
                }
                items.push(serde_json::json!({
                    "tag": tag,
                    "label": label,
                    "unit": if i == 0 { "" } else { "mm" },
                    "value": value,
                    "formula": r.formula,
                    "source": r.source,
                    "missing": value == MISSING,
                }));
            }
        }
        let readout = serde_json::json!([
            {"k": "e₂ = s₁（名义）", "v": fmt_mm(d.e2_s1)},
            {"k": format!("Ae（孔 {}）", spec.hub.token()), "v": fmt_mm(d.ae)},
            {"k": format!("As（轴 {}）", spec.shaft.token()), "v": fmt_mm(d.as_)},
            {"k": format!("孔 {}：Tact / Teff", spec.hub.token()),
             "v": format!("{} / {}", fmt_mm(d.tact_hub), fmt_mm(d.teff_hub))},
            {"k": format!("轴 {}：Tact / Teff", spec.shaft.token()),
             "v": format!("{} / {}", fmt_mm(d.tact_shaft), fmt_mm(d.teff_shaft))},
            {"k": "Bild 6 示例路径", "v": if d.anchor { "是（整表照标准原印值）" } else { "否（按公式/表值计算）" }},
            {"k": "Ae 出处", "v": d.ae_note},
            {"k": "As 出处", "v": d.as_note},
            {"k": "孔公差出处", "v": d.tol_hub_note},
            {"k": "轴公差出处", "v": d.tol_shaft_note},
            {"k": "名义表", "v": d.nominal.as_ref().map(|n| n.source.clone()).unwrap_or_else(|| MISSING.to_string())},
        ]);
        let expr_echo = self
            .expr
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let fields = if expr_echo.is_some() {
            serde_json::json!({"m": spec.m, "z": spec.z, "d_b": spec.d_b})
        } else {
            serde_json::Value::Null
        };
        Ok(serde_json::json!({
            "ok": true,
            "card": "DIN花键参数表",
            "renderer": "din_table",
            "expr": expr_echo,
            "fields": fields,
            "title": format!(
                "DIN 5480 花键参数表（N{}×{}×{}×{} / W…×{}）",
                trim3(spec.d_b), trim3(spec.m), spec.z, spec.hub.token(), spec.shaft.token()
            ),
            "m": spec.m,
            "z": spec.z,
            "d_b": spec.d_b,
            "hub": spec.hub.token(),
            "shaft": spec.shaft.token(),
            "anchor": d.anchor,
            "readout": readout,
            "items": items,
            "missing": missing,
            "missing_note": options_json()["missing_note"],
        }))
    }

    /// 待放置件要带的 26 个 ATTRIB（tag → 值 + ATTDEF 模板）。
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

    /// 直接落点用的 `INSERT`。
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
            "DIN 5480 {} / {}（dB={} m={} z={}{}）",
            designation(&spec, true),
            designation(&spec, false),
            trim3(spec.d_b),
            trim3(spec.m),
            spec.z,
            if d.anchor { "，Bild 6 示例原印值" } else { "" }
        ))
    }

    /// 待放置件的 `OCSM_PART` 元数据（薄台账）。
    pub fn part_meta_json(&self) -> Result<String, String> {
        let spec = self.spec()?;
        Ok(serde_json::json!({
            "family": "din_table",
            "card": "DIN花键参数表",
            "d_b": spec.d_b,
            "m": spec.m,
            "z": spec.z,
            "hub": spec.hub.token(),
            "shaft": spec.shaft.token(),
        })
        .to_string())
    }
}

// ══════════════════════════════════════════════════════════════════════════
// CLI 解析
// ══════════════════════════════════════════════════════════════════════════

/// 命令用法（`OCSMCARD` 报错指路）。
/// 表达式截取用：DIN 卡选项关键字（`N9H`/`W8f`/`e2`/`ae`/`as`/`tactn`…/`at`/`rot`）。
/// `BETA0` 不是 `b` 选项（rest 不是数字）；表达式里的 `M3`/`Z38` 在第 7 token 前不判。
fn is_option_token(t: &str) -> bool {
    let l = t.to_ascii_lowercase();
    if t.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return true; // `9H` 裸配合 / 裸数字
    }
    if matches!(
        l.as_str(),
        "at" | "rot" | "旋转" | "内" | "外" | "内花键" | "外花键" | "孔" | "轴" | "m" | "z"
            | "b" | "db" | "模数" | "齿数" | "基准直径" | "直径" | "e2" | "ae" | "as" | "tactn"
            | "teffn" | "tactw" | "teffw"
    ) {
        return true;
    }
    if l.starts_with('n') || l.starts_with('w') {
        if t[1..].starts_with(|c: char| c.is_ascii_digit()) {
            return true;
        }
    }
    for p in ["m", "z", "b"] {
        if let Some(rest) = l.strip_prefix(p) {
            if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

pub fn usage() -> String {
    "智能卡片「DIN花键参数表」用法：\
     `OCSMCARD DIN花键参数表 <九字段齿形表达式> [N<等级><字母>] [W<等级><字母>] \
     [e2 …] [ae …] [as …] [tactn …] [teffn …] [tactw …] [teffw …] [at x,y] [rot 度]`\
     （如 `OCSMCARD DIN花键参数表 SPLINE IN M3 Z38 ALPHA30 X0.45 BETA0 H30 N9H W8f`；\
     也可沿用 `M3 Z38 B120 N9H W8f` 或代号 `N 120×3×38×9H W 120×3×38×8f`）。\
     表达式反解 d_B=m(z+1.1+2x)、m、z（DIN 5480 压力角恒 30°）。\
     版面 = DIN 5480-1:2006 Bild 6 的 13 行 × Nabe/Welle 两栏；孔缺省 9H、轴缺省 8f；\
     Ae/As 取 Table 7（本仓 OCR 入库：100–200 与 50–100 档有实锚，其余按表头阶梯推断），\
     Tact/Teff 取 Table 7 公差锚（6–9 级、模数组 1,75–4）—— 缺口显示「—」，可用 ae/as/e2/tactn/teffn/tactw/teffw 覆盖。"
        .to_string()
}

/// 解析 `9H` / `8f` / `js` 这类配合 token。
pub fn parse_fit_token(t: &str, hub: bool) -> Result<DinFit, String> {
    let s = t.trim();
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return Err(format!("缺等级数字（如 `9H`/`8f`）"));
    }
    let grade: u32 = digits.parse().map_err(|_| format!("等级「{digits}」非法"))?;
    let letter = s[digits.len()..].trim().to_string();
    if letter.is_empty() {
        return Err("缺偏差字母（孔 F/G/H/J/K/M；轴 v…a）".to_string());
    }
    let letter = if hub { letter.to_ascii_uppercase() } else { letter.to_ascii_lowercase() };
    let fit = DinFit { grade, letter };
    fit.validate(hub)?;
    Ok(fit)
}

/// 把 `×`/`X` 统一成 `x`，去掉全角空格。
fn normalize_input(t: &str) -> String {
    t.replace('×', "x").replace('X', "x").replace('　', " ")
}

/// 解析 4 段长度代号体：`120x3x38x9H` → (dB, m, z, fit)。
fn parse_designation_body(body: &str) -> Result<(f64, f64, u32, String), String> {
    let b = body.trim().trim_start_matches(['N', 'n', 'W', 'w', 'A', 'I', 'a', 'i']);
    let parts: Vec<&str> = b.split('x').collect();
    if parts.len() != 4 {
        return Err(format!(
            "长度代号「{body}」应有 4 段 `d_B×m×z×等级字母`（如 `120×3×38×9H`）"
        ));
    }
    let d_b = csv_decimal(parts[0]).ok_or_else(|| format!("d_B「{}」不是数字", parts[0]))?;
    let m = csv_decimal(parts[1]).ok_or_else(|| format!("m「{}」不是数字", parts[1]))?;
    let z: u32 = parts[2]
        .trim()
        .parse()
        .map_err(|_| format!("z「{}」不是整数", parts[2]))?;
    let fit = parts[3].trim().to_string();
    if fit.is_empty() {
        return Err("等级字母段为空".to_string());
    }
    Ok((d_b, m, z, fit))
}

impl DinTableSpec {
    /// 解析 CLI 文本。
    pub fn parse(text: &str) -> Result<Self, String> {
        let norm = normalize_input(text);
        let tokens: Vec<&str> = norm.split_whitespace().collect();
        if tokens.is_empty() {
            return Err(usage());
        }
        let mut spec = DinTableSpec {
            m: 0.0,
            z: 0,
            d_b: 0.0,
            hub: DinFit { grade: 9, letter: "H".into() },
            shaft: DinFit { grade: 8, letter: "f".into() },
            e2_s1: None,
            ae: None,
            as_: None,
            tact_hub: None,
            teff_hub: None,
            tact_shaft: None,
            teff_shaft: None,
            at: None,
            rot: 0.0,
        };
        let mut have = (false, false, false); // m, z, d_b
        let mut pending: Option<bool> = None; // Some(true)=待接孔代号体
        // 表达式形态：`<九字段表达式> [N9H] [W8f] [e2 …] … [at x,y] [rot 度]`。
        let mut expr: Option<String> = None;
        let tokens = if let Some((e, rest)) = crate::card_expr::split_expr(&tokens, is_option_token) {
            expr = Some(e);
            rest
        } else {
            tokens
        };
        let mut i = 0usize;
        let need = |i: &mut usize, tokens: &[&str], what: &str| -> Result<String, String> {
            *i += 1;
            tokens.get(*i).map(|s| s.to_string()).ok_or_else(|| format!("DIN 花键参数表：{what} 缺少数值"))
        };
        while i < tokens.len() {
            let t = tokens[i];
            let lower = t.to_ascii_lowercase();
            // 代号体（pending 或自带 N/W 前缀）
            if let Some(hub) = pending.take() {
                let Some((d_b, m, z, fit)) = parse_designation_body(t).ok() else {
                    return Err(format!("DIN 花键参数表：代号体「{t}」非法。\n{}", usage()));
                };
                spec.d_b = d_b;
                spec.m = m;
                spec.z = z;
                have = (true, true, true);
                let f = parse_fit_token(&fit, hub)?;
                if hub { spec.hub = f; } else { spec.shaft = f; }
                i += 1;
                continue;
            }
            if lower == "n" || lower == "w" || lower == "内" || lower == "外" {
                pending = Some(lower == "n" || lower == "内");
                i += 1;
                continue;
            }
            // 形如 n120x3x38x9H / w120x3x38x8f（含 A/I 直径对中前缀也收下后忽略）
            if (lower.starts_with('n') || lower.starts_with('w'))
                && t[1..].contains('x')
                && t[1..].chars().next().is_some_and(|c| c.is_ascii_digit() || c == 'a' || c == 'i')
            {
                let hub = lower.starts_with('n');
                let body = &t[1..];
                // 跳过 A/I 前缀
                let body = body
                    .trim_start_matches(['A', 'I', 'a', 'i'])
                    .trim();
                let Some((d_b, m, z, fit)) = parse_designation_body(body).ok() else {
                    return Err(format!(
                        "DIN 花键参数表：代号「{t}」非法（应如 `N120×3×38×9H`）。\n{}",
                        usage()
                    ));
                };
                spec.d_b = d_b;
                spec.m = m;
                spec.z = z;
                have = (true, true, true);
                let f = parse_fit_token(&fit, hub)?;
                if hub { spec.hub = f; } else { spec.shaft = f; }
                i += 1;
                continue;
            }
            let mut key = lower.clone();
            let mut inline: Option<String> = None;
            if let Some((k, v)) = t.split_once('=') {
                key = k.to_ascii_lowercase();
                inline = Some(v.to_string());
            }
            let value = |i: &mut usize, tokens: &[&str]| -> Result<String, String> {
                if let Some(v) = &inline {
                    if !v.is_empty() {
                        return Ok(v.clone());
                    }
                }
                *i += 1;
                tokens
                    .get(*i)
                    .map(|s| s.to_string())
                    .ok_or_else(|| format!("DIN 花键参数表：「{key}」缺少数值"))
            };
            match key.as_str() {
                "at" => {
                    let v = need(&mut i, &tokens, "at")?;
                    let (x, y) = v
                        .split_once(',')
                        .ok_or_else(|| format!("DIN 花键参数表：at「{v}」应为 `x,y`"))?;
                    let x: f64 = x.trim().parse().map_err(|e| format!("at x={x} 不是数字：{e}"))?;
                    let y: f64 = y.trim().parse().map_err(|e| format!("at y={y} 不是数字：{e}"))?;
                    spec.at = Some([x, y]);
                }
                "rot" | "旋转" => {
                    let v = need(&mut i, &tokens, "rot")?;
                    spec.rot = v.parse().map_err(|e| format!("rot={v} 不是数字：{e}"))?;
                }
                "m" | "模数" => {
                    let v = value(&mut i, &tokens)?;
                    let body = v.trim_start_matches(['m', 'M']);
                    spec.m = body.parse().map_err(|e| format!("m={v} 不是数字：{e}"))?;
                    have.0 = true;
                }
                "z" | "齿数" => {
                    let v = value(&mut i, &tokens)?;
                    let body = v.trim_start_matches(['z', 'Z']);
                    spec.z = body.parse().map_err(|e| format!("z={v} 不是整数：{e}"))?;
                    have.1 = true;
                }
                "b" | "db" | "基准直径" | "直径" => {
                    let v = value(&mut i, &tokens)?;
                    let body = v.trim_start_matches(['b', 'B']);
                    spec.d_b = body.parse().map_err(|e| format!("d_B={v} 不是数字：{e}"))?;
                    have.2 = true;
                }
                "n" | "内" | "内花键" | "孔" => {
                    let v = value(&mut i, &tokens)?;
                    spec.hub = parse_fit_token(&v, true)?;
                }
                "w" | "外" | "外花键" | "轴" => {
                    let v = value(&mut i, &tokens)?;
                    spec.shaft = parse_fit_token(&v, false)?;
                }
                "e2" => {
                    let v = value(&mut i, &tokens)?;
                    spec.e2_s1 = Some(v.parse().map_err(|e| format!("e2={v} 不是数字：{e}"))?);
                }
                "ae" => {
                    let v = value(&mut i, &tokens)?;
                    spec.ae = Some(v.parse().map_err(|e| format!("ae={v} 不是数字：{e}"))?);
                }
                "as" => {
                    let v = value(&mut i, &tokens)?;
                    spec.as_ = Some(v.parse().map_err(|e| format!("as={v} 不是数字：{e}"))?);
                }
                "tactn" => {
                    let v = value(&mut i, &tokens)?;
                    spec.tact_hub = Some(v.parse().map_err(|e| format!("tactn={v} 不是数字：{e}"))?);
                }
                "teffn" => {
                    let v = value(&mut i, &tokens)?;
                    spec.teff_hub = Some(v.parse().map_err(|e| format!("teffn={v} 不是数字：{e}"))?);
                }
                "tactw" => {
                    let v = value(&mut i, &tokens)?;
                    spec.tact_shaft = Some(v.parse().map_err(|e| format!("tactw={v} 不是数字：{e}"))?);
                }
                "teffw" => {
                    let v = value(&mut i, &tokens)?;
                    spec.teff_shaft = Some(v.parse().map_err(|e| format!("teffw={v} 不是数字：{e}"))?);
                }
                // 无 = 的短写法（M3 / Z38 / B120 / N9H / W8f）
                _ if key.starts_with('m') && key[1..].chars().all(|c| c.is_ascii_digit() || c == '.') => {
                    spec.m = key[1..].parse().map_err(|e| format!("m={t} 不是数字：{e}"))?;
                    have.0 = true;
                }
                _ if key.starts_with('z') && key[1..].chars().all(|c| c.is_ascii_digit()) => {
                    spec.z = key[1..].parse().map_err(|e| format!("z={t} 不是整数：{e}"))?;
                    have.1 = true;
                }
                _ if (key.starts_with('b') || key.starts_with("db"))
                    && key.trim_start_matches("db").trim_start_matches('b').chars().all(|c| c.is_ascii_digit() || c == '.') =>
                {
                    let body = key.trim_start_matches("db").trim_start_matches('b');
                    spec.d_b = body.parse().map_err(|e| format!("d_B={t} 不是数字：{e}"))?;
                    have.2 = true;
                }
                _ if key.starts_with('n') && key.len() > 1 => {
                    spec.hub = parse_fit_token(&t[1..], true)?;
                }
                _ if key.starts_with('w') && key.len() > 1 => {
                    spec.shaft = parse_fit_token(&t[1..], false)?;
                }
                _ => {
                    // 孔/轴配合也可用裸 token「9H」直接给（按字母大小写判侧：大写=孔、小写=轴）。
                    let digits: String =
                        t.chars().take_while(|c| c.is_ascii_digit()).collect();
                    let letter = t[digits.len()..].trim();
                    let parsed = if digits.is_empty() || letter.is_empty() {
                        Err(format!("不认识的参数「{t}」"))
                    } else if letter.chars().all(|c| c.is_ascii_uppercase()) {
                        parse_fit_token(t, true).map(|f| (f, true))
                    } else if letter.chars().all(|c| c.is_ascii_lowercase()) {
                        parse_fit_token(t, false).map(|f| (f, false))
                    } else {
                        Err(format!("不认识的参数「{t}」"))
                    };
                    match parsed {
                        Ok((f, true)) => spec.hub = f,
                        Ok((f, false)) => spec.shaft = f,
                        Err(e) => {
                            return Err(format!(
                                "DIN 花键参数表：{e}。\n{}",
                                usage()
                            ))
                        }
                    }
                }
            }
            i += 1;
        }
        if pending.is_some() {
            return Err(format!("DIN 花键参数表：`N`/`W` 后缺长度代号（如 `N 120×3×38×9H`）。\n{}", usage()));
        }
        if let Some(e) = expr {
            let r = crate::card_expr::resolve(&EXPR_POLICY, &e, None)?;
            let em = r.value("m").ok_or_else(|| {
                "DIN 花键参数表：表达式映射表缺 m（内部错误）".to_string()
            })?;
            let ez = r.value("z").ok_or_else(|| {
                "DIN 花键参数表：表达式映射表缺 z（内部错误）".to_string()
            })? as u32;
            let edb = r.value("d_b").ok_or_else(|| {
                "DIN 花键参数表：表达式映射表缺 d_B（内部错误）".to_string()
            })?;
            if have.0 && (spec.m - em).abs() > 1e-9 {
                return Err(format!(
                    "DIN 花键参数表：显式 m={} 与表达式反解的 m={} 不一致",
                    trim3(spec.m),
                    trim3(em)
                ));
            }
            if have.1 && spec.z != ez {
                return Err(format!(
                    "DIN 花键参数表：显式 z={} 与表达式反解的 z={ez} 不一致",
                    spec.z
                ));
            }
            if have.2 && (spec.d_b - edb).abs() > 1e-3 {
                return Err(format!(
                    "DIN 花键参数表：显式 d_B={} 与表达式反解的 d_B={} 不一致",
                    trim3(spec.d_b),
                    trim3(edb)
                ));
            }
            spec.m = em;
            spec.z = ez;
            spec.d_b = edb;
        } else if !have.0 || !have.1 || !have.2 {
            let what = match have {
                (false, false, false) => "模数 m、齿数 z、基准直径 d_B",
                (false, _, _) => "模数 m",
                (_, false, _) => "齿数 z",
                _ => "基准直径 d_B",
            };
            return Err(format!("DIN 花键参数表：缺 {what}（写法 `M3 Z38 B120` / 代号 `N 120×3×38×9H` / 九字段表达式）。\n{}", usage()));
        }
        derive(&spec)?;
        Ok(spec)
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 测试
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn near6(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.5e-6
    }

    fn example() -> DinTableSpec {
        DinTableSpec::default()
    }

    /// Table 7 上段资产：18 行 × 9 列、阶梯倍数（±1 µm 容差）、两个实锚（c4=100–200 / c5=50–100）。
    #[test]
    fn table7_dev_asset_and_range_anchors() {
        let rows = table7_rows();
        assert_eq!(rows.len(), 18, "18 个 Abmaß 系列");
        let ladder = [200, 180, 160, 140, 125, 110, 100, 90, 80];
        let factors = [
            ("v", 1.0), ("u", 0.9), ("t", 0.8), ("s", 0.7), ("r", 0.6), ("p", 0.5),
            ("n", 0.4), ("m", 0.3), ("k", 0.2), ("js", 0.1), ("h", 0.0), ("g", -0.1),
            ("f", -0.2), ("e", -0.3), ("d", -0.4), ("c", -0.6), ("b", -0.8), ("a", -1.0),
        ];
        for (row, (letter, f)) in rows.iter().zip(factors.iter()) {
            assert_eq!(row.series, *letter);
            for (ci, l) in ladder.iter().enumerate() {
                let expect = (f * *l as f64).round() as i32;
                let got = row.values[ci].expect("162 格均无留空");
                // 阶梯第 5 档 125 的 .5 倍在本表不按统一舍入（u=112、s=88、p=62、m=37、js=12），
                // 以表值为准；其余格严格等于系列系数×阶梯。
                assert!(
                    (got - expect).abs() <= 1,
                    "系列 {letter} c{} 应 ≈ {expect}（±1），实为 {got}",
                    ci + 1
                );
            }
        }
        // 表值逐格示例（.5 档按表值）
        let get = |s: &str, ci: usize| dev_lookup(s, false).unwrap().values[ci];
        assert_eq!(get("v", 0), Some(200));
        assert_eq!(get("u", 4), Some(112));
        assert_eq!(get("s", 4), Some(88));
        assert_eq!(get("p", 4), Some(62));
        assert_eq!(get("m", 4), Some(37));
        assert_eq!(get("js", 4), Some(12));
        assert_eq!(get("a", 0), Some(-200));
        // 实锚 1：Bild 6 示例（dB=120, 8f）As = −28 µm ⇒ c4 = 100–200
        let f = dev_lookup("f", false).unwrap();
        assert_eq!(f.series, "f");
        assert_eq!(f.values[3], Some(-28));
        // 实锚 2：KISSsoft 样张（dB=70, 6f）As = −25 µm ⇒ c5 = 50–100
        assert_eq!(f.values[4], Some(-25));
        assert_eq!(range_for_d_b(120.0), Some((3, "100–200", "已核(锚)")));
        assert_eq!(range_for_d_b(70.0), Some((4, "50–100", "已核(锚)")));
        assert_eq!(range_for_d_b(30.0), Some((5, "25–50", "推断")));
        assert_eq!(range_for_d_b(300.0), Some((2, "200–400", "推断")));
        assert_eq!(range_for_d_b(5.0), Some((7, "≤12", "推断")));
        // >400 侧与 c1/c2/c9 不取（无实锚）：
        assert_eq!(range_for_d_b(500.0), None);
        assert!(dev_lookup_mm("f", 500.0, false).is_err());
        // 孔系列：F↔k、M↔e；孔 H 基本偏差 0
        assert_eq!(dev_lookup("F", true).unwrap().series, "k");
        assert_eq!(dev_lookup("M", true).unwrap().series, "e");
        assert_eq!(dev_lookup("F", false).unwrap().series, "f", "大写 F 在轴侧仍是 f");
        assert_eq!(dev_lookup_mm("H", 120.0, true).unwrap().0, 0.0);
    }

    /// Table 7 公差锚：6–9 级有值，10/11 级为待抽空行；模数组 1,75–4 外不可用。
    #[test]
    fn table7_tol_asset() {
        let rows = table7_tol_rows();
        assert_eq!(rows.len(), 6);
        let tol = |g: u32| rows.iter().find(|r| r.grade == g).unwrap().clone();
        assert!(near6(tol(6).tact.unwrap(), 0.018) && near6(tol(6).teff.unwrap(), 0.010));
        assert!(near6(tol(7).tact.unwrap(), 0.025) && near6(tol(7).teff.unwrap(), 0.015));
        assert!(near6(tol(8).tact.unwrap(), 0.040) && near6(tol(8).teff.unwrap(), 0.023));
        assert!(near6(tol(9).tact.unwrap(), 0.056) && near6(tol(9).teff.unwrap(), 0.034));
        for g in [10, 11] {
            assert!(tol(g).tact.is_none() && tol(g).teff.is_none(), "等级 {g} 待抽");
        }
        let (t, e, tg, n) = tol_lookup_mm(9, 3.0).unwrap();
        assert!(near6(t, 0.056) && near6(e, 0.034) && near6(tg, 0.090));
        assert!(n.contains("Table 7"));
        assert!(tol_lookup_mm(9, 7.5).is_err(), "模数组 5–10 未抽");
        assert!(tol_lookup_mm(10, 3.0).is_err(), "10 级未抽");
    }

    /// Bild 6 示例：26 项逐项等于标准原印值；公式路与锚点一致。
    #[test]
    fn bild6_example_reproduces_standard() {
        let spec = example();
        assert!(is_bild6_example(&spec));
        let d = derive(&spec).unwrap();
        assert!(d.anchor);
        // 六值（原印）
        assert!(near(d.e_max.unwrap(), 6.361));
        assert!(near(d.e_min.unwrap(), 6.305));
        assert!(near(d.e_vmin.unwrap(), 6.271));
        assert!(near(d.s_vmax.unwrap(), 6.243));
        assert!(near(d.s_max.unwrap(), 6.220));
        assert!(near(d.s_min.unwrap(), 6.180));
        // 反推自洽：e2=6.271、Ae=0、As=−0.028、Tact 9H=0.056、Teff 9H=0.034、8f=0.040/0.023
        assert!(near(d.e2_s1.unwrap(), 6.271));
        assert!(near(anchor_f64("meta", "Ae_hub").unwrap(), 0.0));
        assert!(near(anchor_f64("meta", "As_shaft").unwrap(), -0.028));
        assert!(near(d.tact_hub.unwrap(), 0.056));
        assert!(near(d.teff_hub.unwrap(), 0.034));
        assert!(near(d.tact_shaft.unwrap(), 0.040));
        assert!(near(d.teff_shaft.unwrap(), 0.023));
        // 公式路复算（显式覆盖 = 锚点反推值；不启用 anchor 路径）
        let mut calc = spec.clone();
        calc.ae = Some(0.0);
        calc.as_ = Some(-0.028);
        calc.tact_hub = Some(0.056);
        calc.teff_hub = Some(0.034);
        calc.tact_shaft = Some(0.040);
        calc.teff_shaft = Some(0.023);
        assert!(!is_bild6_example(&calc));
        let c = derive(&calc).unwrap();
        for (a, b) in [
            (c.e_max.unwrap(), 6.361),
            (c.e_min.unwrap(), 6.305),
            (c.e_vmin.unwrap(), 6.271),
            (c.s_vmax.unwrap(), 6.243),
            (c.s_max.unwrap(), 6.220),
            (c.s_min.unwrap(), 6.180),
        ] {
            assert!(near6(a, b), "公式路 {a} != {b}");
        }
        // 26 项取值（示例逐项）
        let vals = values(&spec).unwrap();
        let get = |tag: &str| vals.iter().find(|(t, _)| t == tag).unwrap().1.clone();
        assert_eq!(get("N标记"), "Nabe DIN 5480 – N120×3×38×9H");
        assert_eq!(get("W标记"), "Welle DIN 5480 – W120×3×38×8f");
        assert_eq!(get("N齿数"), "38");
        assert_eq!(get("N压力角"), "30°");
        assert_eq!(get("N齿根圆"), "120 +0.76");
        assert_eq!(get("N齿根成形圆"), "119.49 min.");
        assert_eq!(get("N齿顶圆"), "114 H11");
        assert_eq!(get("N槽宽max"), "6.361");
        assert_eq!(get("N槽宽min"), "6.305");
        assert_eq!(get("N槽宽eff"), "6.271");
        assert_eq!(get("N量圆"), "5.25");
        assert_eq!(get("N量距max"), "109.266");
        assert_eq!(get("N量距min"), "109.169");
        assert_eq!(get("W齿顶圆"), "119.40 h11");
        assert_eq!(get("W齿根成形圆"), "113.91 max.");
        assert_eq!(get("W齿根圆"), "113.4 -1.74");
        assert_eq!(get("W齿厚svmax"), "6.243");
        assert_eq!(get("W齿厚smax"), "6.22");
        assert_eq!(get("W齿厚smin"), "6.18");
        assert_eq!(get("W量圆"), "6");
        assert_eq!(get("W量距max"), "126.017");
        assert_eq!(get("W量距min"), "125.956");
    }

    /// 一般路径：KISSsoft 示例件（m=3、dB=70、z=22、6f/7H）应命中 Table 7 实锚列并复算。
    #[test]
    fn general_path_kisssoft_row() {
        let spec = DinTableSpec {
            m: 3.0,
            z: 22,
            d_b: 70.0,
            hub: DinFit { grade: 7, letter: "H".into() },
            shaft: DinFit { grade: 6, letter: "f".into() },
            ..DinTableSpec::default()
        };
        let d = derive(&spec).unwrap();
        assert!(!d.anchor);
        assert!(near6(d.ae.unwrap(), 0.0));
        assert!(near6(d.as_.unwrap(), -0.025));
        assert!(near6(d.tact_hub.unwrap(), 0.025));
        assert!(near6(d.teff_hub.unwrap(), 0.015));
        assert!(near6(d.tact_shaft.unwrap(), 0.018));
        assert!(near6(d.teff_shaft.unwrap(), 0.010));
        // e2=s1 由名义表（m=3,dB=70,z=22）给出；名义表 3 位小数 = 5,117
        let n = d.nominal.as_ref().unwrap();
        assert!(near6(n.e2_s1, 5.117), "名义表 e2/s1 = {}", n.e2_s1);
        // 与 KISSsoft 表相差 ≤1e-3（名义表 3 位小数舍入所致）
        let near3 = |a: f64, b: f64| (a - b).abs() < 1.5e-3;
        assert!(near3(d.e_vmin.unwrap(), 5.1167));
        assert!(near3(d.e_min.unwrap(), 5.1317));
        assert!(near3(d.e_max.unwrap(), 5.1568));
        assert!(near3(d.s_vmax.unwrap(), 5.0920));
        assert!(near3(d.s_max.unwrap(), 5.0820));
        assert!(near3(d.s_min.unwrap(), 5.0640));
    }

    /// 标缺行为：>400 侧 Ae/As → 「—」；模数组 5–10 的 Tact/Teff → 「—」；未入检验表行 → M 值「—」。
    #[test]
    fn missing_behavior_is_explicit() {
        // dB=500（>400 侧）：H 查不到，derive 返回带缺口说明的 Ok（图仍能出，值显示「—」）
        let spec = DinTableSpec { d_b: 500.0, ..DinTableSpec::default() };
        let d = derive(&spec).unwrap();
        assert!(d.ae.is_none() && d.as_.is_none());
        assert!(d.notes.iter().any(|n| n.contains(">400")), "{:?}", d.notes);
        assert!(vals_has_missing(&spec, "N槽宽eff"));
        // 换用显式 Ae/As 后可出 e/s，但直径/M 值仍如实标缺
        let spec2 = DinTableSpec {
            d_b: 500.0,
            ae: Some(0.0),
            as_: Some(-0.05),
            ..DinTableSpec::default()
        };
        let d2 = derive(&spec2).unwrap();
        assert!(d2.nominal.is_none());
        assert!(d2.e2_s1.is_none() && d2.e_vmin.is_none(), "无名义表行 ⇒ e₂ 缺口，e/s 显示「—」");
        assert!(vals_has_missing(&spec2, "N齿根圆"));
        assert!(vals_has_missing(&spec2, "N量距max"));
        // 模数组 5–10 → 公差「—」
        let spec3 = DinTableSpec { m: 5.0, z: 16, d_b: 80.0, ..DinTableSpec::default() };
        let d3 = derive(&spec3).unwrap();
        assert!(d3.tact_hub.is_none() && d3.teff_hub.is_none());
        assert!(d3.tact_shaft.is_none() && d3.teff_shaft.is_none());
        assert!(d3.e_max.is_none() && d3.e_min.is_none());
        assert!(vals_has_missing(&spec3, "N槽宽max"));
        // 未知 (m,z,d_B) → 名义表缺口进 notes
        let spec4 = DinTableSpec { m: 3.0, z: 99, d_b: 120.0, ..DinTableSpec::default() };
        let d4 = derive(&spec4).unwrap();
        assert!(d4.nominal.is_none() && d4.e_vmin.is_none());
    }

    fn vals_has_missing(spec: &DinTableSpec, tag: &str) -> bool {
        values(spec)
            .unwrap()
            .iter()
            .any(|(t, v)| t == tag && v == MISSING)
    }

    /// 块结构：15 横线 + 5 竖线 + 27 MTEXT（标题 + 26 标签）+ 26 ATTDEF；全部 OCSM_GB；
    /// 文本框不出本格（几何估算，实体级字宽）。
    #[test]
    fn block_structure_and_no_text_overlap() {
        let ents = block_entities();
        let n_line = ents.iter().filter(|e| matches!(e, EntityType::Line(_))).count();
        let n_mtext = ents.iter().filter(|e| matches!(e, EntityType::MText(_))).count();
        let n_att = ents.iter().filter(|e| matches!(e, EntityType::AttributeDefinition(_))).count();
        assert_eq!(n_line, 20, "15 横线 + 5 竖线");
        assert_eq!(n_mtext, 25, "标题 + 12×2 标签（标记行不另画标签）");
        assert_eq!(n_att, 26, "13×2 值属性");
        // 所有文字样式一律 OCSM_GB（用户要求）
        for e in &ents {
            match e {
                EntityType::MText(m) => assert_eq!(m.style, "OCSM_GB"),
                EntityType::AttributeDefinition(a) => assert_eq!(a.text_style, "OCSM_GB"),
                _ => {}
            }
        }
        // 文本框几何检查：宽度按实体级字宽估算；不出标签列/侧栏；同行不重叠。
        let est = |s: &str, h: f64, f: f64| -> f64 {
            s.chars()
                .map(|c| {
                    if c == ' ' {
                        0.25
                    } else if c.is_ascii() {
                        if c.is_ascii_digit() || c == '.' || c == '+' || c == '-' {
                            0.55
                        } else {
                            0.75
                        }
                    } else {
                        1.0
                    }
                })
                .sum::<f64>()
                * h
                * f
        };
        let spec = example();
        let vals = values(&spec).unwrap();
        for (i, r) in ROWS.iter().enumerate() {
            let y = if i == 0 { DESIG_Y } else { data_row_y(i - 1) };
            let hub = vals.iter().find(|(t, _)| t == r.hub_tag).unwrap().1.clone();
            let shaft = vals.iter().find(|(t, _)| t == r.shaft_tag).unwrap().1.clone();
            // 标签不出标签列（标记行无标签，跳过）
            if i != 0 {
                assert!(
                    est(r.hub_label, TEXT_H, LABEL_W_FACTOR) < W_LABEL - LABEL_X_HUB - 2.0,
                    "标签「{}」出内花键标签列",
                    r.hub_label
                );
                assert!(
                    est(r.shaft_label, TEXT_H, LABEL_W_FACTOR) < W_LABEL - LABEL_X_HUB - 2.0,
                    "标签「{}」出外花键标签列",
                    r.shaft_label
                );
            }
            // 值不出侧栏（标记行从标签 x 起跨两列）
            let vx_hub = if i == 0 { LABEL_X_HUB } else { VALUE_X_HUB };
            let vx_shaft = if i == 0 { LABEL_X_SHAFT } else { VALUE_X_SHAFT };
            assert!(
                vx_hub + est(&hub, TEXT_H, VALUE_W_FACTOR) < W_SIDE - 2.0,
                "值「{hub}」出内花键侧栏"
            );
            assert!(
                vx_shaft + est(&shaft, TEXT_H, VALUE_W_FACTOR) < W_TOTAL - 2.0,
                "值「{shaft}」出外花键侧栏"
            );
            // 同行两框不相交（标签右缘 < 值左缘；两侧之间留缝；标记行只有值）
            if i != 0 {
                let lh = LABEL_X_HUB + est(r.hub_label, TEXT_H, LABEL_W_FACTOR);
                assert!(lh < VALUE_X_HUB, "行 {i} 孔标签与孔值相交");
                let ls = LABEL_X_SHAFT + est(r.shaft_label, TEXT_H, LABEL_W_FACTOR);
                assert!(ls < VALUE_X_SHAFT, "行 {i} 轴标签与轴值相交");
            }
            let _ = y;
        }
        // 标题居中不出框
        assert!(W_TOTAL / 2.0 - est("DIN 5480 花键参数表", TITLE_H, 1.0) / 2.0 > 0.0);
    }

    /// CLI：三种写法（短参数 / 代号 / 混合）与覆盖项；报错指路。
    #[test]
    fn cli_parse_forms() {
        let a = DinTableSpec::parse("M3 Z38 B120").unwrap();
        assert_eq!(a, DinTableSpec { m: 3.0, z: 38, d_b: 120.0, ..DinTableSpec::default() });
        let b = DinTableSpec::parse("N120×3×38×9H W120×3×38×8f").unwrap();
        assert_eq!(b.m, 3.0);
        assert_eq!(b.d_b, 120.0);
        assert_eq!(b.hub.token(), "9H");
        assert_eq!(b.shaft.token(), "8f");
        let c = DinTableSpec::parse("N 120x3x38x9H W 120x3x38x8f at 10,20 rot 30").unwrap();
        assert_eq!(c.at, Some([10.0, 20.0]));
        assert_eq!(c.rot, 30.0);
        let d = DinTableSpec::parse("模数 3 齿数 38 基准直径 120 内花键 9H 外花键 8f").unwrap();
        assert_eq!(d.hub.token(), "9H");
        let e = DinTableSpec::parse("M3 Z38 B120 ae=0 as=-0.028 tactn=0.056 teffn=0.034").unwrap();
        assert_eq!(e.ae, Some(0.0));
        assert!(near(e.as_.unwrap(), -0.028));
        // 报错
        assert!(DinTableSpec::parse("M3 Z38").unwrap_err().contains("基准直径"));
        assert!(DinTableSpec::parse("M3 Z38 B120 N9v").unwrap_err().contains("偏差系列"));
        assert!(DinTableSpec::parse("M3 Z38 B120 W9L").unwrap_err().contains("偏差系列"));
        assert!(DinTableSpec::parse("M3 Z38 B120 foo").unwrap_err().contains("不认识的参数"));
        assert!(DinTableSpec::parse("").unwrap_err().contains("用法"));
    }

    /// GUI 模型：字段解析 / 预览 JSON 形状 / 标缺清单。
    #[test]
    fn model_preview_json() {
        let m: DinTableModel = serde_json::from_value(serde_json::json!({
            "card": "DIN花键参数表", "m": 3, "z": 38, "d_b": 120,
            "hub": "9H", "shaft": "8f", "at": null, "rot": 0
        }))
        .unwrap();
        let j = m.preview_json().unwrap();
        assert_eq!(j["ok"], true);
        assert_eq!(j["renderer"], "din_table");
        assert_eq!(j["anchor"], true);
        assert_eq!(j["items"].as_array().unwrap().len(), 26);
        assert_eq!(j["missing"].as_array().unwrap().len(), 0);
        assert!(j["readout"].as_array().unwrap().len() >= 10);
        // 标缺路径
        let m2: DinTableModel = serde_json::from_value(serde_json::json!({
            "m": 5, "z": 16, "d_b": 80, "hub": "9H", "shaft": "8f"
        }))
        .unwrap();
        let j2 = m2.preview_json().unwrap();
        assert!(!j2["missing"].as_array().unwrap().is_empty());
        assert!(j2["missing_note"].as_str().unwrap().contains("Table 7"));
    }

    /// 代号与用法串。
    #[test]
    fn designation_and_usage() {
        let spec = example();
        assert_eq!(designation(&spec, true), "Nabe DIN 5480 – N120×3×38×9H");
        assert_eq!(designation(&spec, false), "Welle DIN 5480 – W120×3×38×8f");
        let u = usage();
        assert!(u.contains("DIN花键参数表"));
        assert!(u.contains("Bild 6"));
        assert!(table7_rows().iter().all(|r| r.values.len() == 9));
    }

    /// INSERT：26 个 ATTRIB、层与旋转；与取值一一对应。
    #[test]
    fn insert_attributes() {
        let spec = example();
        let ins = build_insert(&spec, [5.0, 7.0], 15.0).unwrap();
        assert_eq!(ins.attributes.len(), 26);
        assert!(near(ins.rotation.to_degrees(), 15.0));
        assert_eq!(ins.attributes[0].tag, "N标记");
        assert!(ins.attributes[0].value.contains("N120×3×38×9H"));
        assert_eq!(ins.attributes[25].tag, "W量距min");
        assert_eq!(ins.attributes[25].value, "125.956");
        let model = DinTableModel {
            card: "DIN花键参数表".into(),
            expr: None,
            m: 3.0,
            z: 38,
            d_b: 120.0,
            hub: Some("9H".into()),
            shaft: Some("8f".into()),
            e2: None,
            ae: None,
            as_: None,
            tact_n: None,
            teff_n: None,
            tact_w: None,
            teff_w: None,
            at: Some([0.0, 0.0]),
            rot: 0.0,
        };
        let echo = model.echo_note().unwrap();
        assert!(echo.contains("Bild 6 示例原印值"), "{echo}");
        assert!(model.part_meta_json().unwrap().contains("din_table"));
        assert!(model.pending_attrs().unwrap().len() == 26);
        assert!(markdown_table(&spec).unwrap().contains("N槽宽max"));
    }

    fn expr_model(expr: &str) -> DinTableModel {
        DinTableModel {
            card: "DIN花键参数表".into(),
            expr: Some(expr.into()),
            m: 0.0,
            z: 0,
            d_b: 0.0,
            hub: None,
            shaft: None,
            e2: None,
            ae: None,
            as_: None,
            tact_n: None,
            teff_n: None,
            tact_w: None,
            teff_w: None,
            at: None,
            rot: 0.0,
        }
    }

    /// ★ 表达式 → 字段 → 卡内值：d_B=m(z+1.1+2x)、m、z；Bild 6 锚点逐项。
    #[test]
    fn din_expr_maps_to_base_diameter_and_anchor() {
        let spec = expr_model("SPLINE IN M3 Z38 ALPHA30 X0.45 DA120 DF114 BETA0 H30")
            .spec()
            .unwrap();
        assert!(near(spec.d_b, 120.0), "d_B={}", spec.d_b);
        assert!(near(spec.m, 3.0));
        assert_eq!(spec.z, 38);
        assert!(is_bild6_example(&spec), "表达式反解 d_B=120 → 走 Bild 6 锚点");
        let vals = values(&spec).unwrap();
        let get = |tag: &str| vals.iter().find(|(t, _)| t == tag).unwrap().1.clone();
        assert!(get("N标记").contains("N120×3×38×9H"), "{}", get("N标记"));
        assert!(get("W标记").contains("W120×3×38×8f"), "{}", get("W标记"));
        assert_eq!(get("N槽宽max"), "6.361");
        assert_eq!(get("W齿厚svmax"), "6.243");
        // 与旧 M/Z/B 输入同值
        let old = DinTableSpec::parse("M3 Z38 B120 N9H W8f").unwrap();
        assert_eq!(values(&old).unwrap(), values(&spec).unwrap());
        // 预览回填 fields + expr 回显
        let pj = expr_model("SPLINE IN M3 Z38 ALPHA30 X0.45 DA120 DF114 BETA0 H30")
            .preview_json()
            .unwrap();
        assert!(near(pj["fields"]["d_b"].as_f64().unwrap(), 120.0));
        assert!(near(pj["fields"]["m"].as_f64().unwrap(), 3.0));
        assert_eq!(pj["fields"]["z"], 38);
        assert!(pj["expr"].as_str().unwrap().starts_with("SPLINE IN"));
    }

    /// CLI：完整行/短表达式（缺 DA/DF/BETA/H）两种都能截；旧写法保留；冲突/体系错误。
    #[test]
    fn din_cli_expr_form_and_errors() {
        let cli = DinTableSpec::parse(
            "SPLINE IN M3 Z38 ALPHA30 X0.45 DA120 DF114 BETA0 H30 N9H W8f",
        )
        .unwrap();
        assert!(near(cli.d_b, 120.0) && near(cli.m, 3.0) && cli.z == 38);
        assert_eq!(cli.hub.token(), "9H");
        assert_eq!(cli.shaft.token(), "8f");
        // 短表达式：到 X 就结束，后面直接跟 W8f（无 DA/DF/BETA/H）
        let short = DinTableSpec::parse("SPLINE IN M3 Z38 ALPHA30 X0.45 W8f").unwrap();
        assert!(near(short.d_b, 120.0));
        assert_eq!(short.shaft.token(), "8f");
        // KIND 两向都收（DIN 卡 Nabe/Welle 两栏都做）；外花键 DA/DF 用表值 119.4/113.4
        let ex = DinTableSpec::parse(
            "SPLINE EX M3 Z38 ALPHA30 X0.45 DA119.4 DF113.4 BETA0 H30",
        )
        .unwrap();
        assert!(near(ex.d_b, 120.0));
        // 显式 d_B 与表达式反解不一致 → 报错
        let e = DinTableSpec::parse(
            "SPLINE IN M3 Z38 ALPHA30 X0.45 DA120 DF114 BETA0 H30 B100",
        )
        .unwrap_err();
        assert!(e.contains("显式 d_B=100") && e.contains("d_B=120"), "{e}");
        // α 必须 30
        let e = DinTableSpec::parse("SPLINE IN M3 Z38 ALPHA20 X0 BETA0 H30").unwrap_err();
        assert!(e.contains("压力角 20°") && e.contains("α=30"), "{e}");
        // MARK 体系不符
        let e = expr_model("GEAR IN M3 Z38 ALPHA30 X0.45 BETA0 H30")
            .spec()
            .unwrap_err();
        assert!(e.contains("MARK") && e.contains("GEAR（齿轮）"), "{e}");
    }
}
