//! `OCSMJOINT`：按**件链**装配螺栓副（确定性执行器）。
//!
//! 只做三件事，全部可复现、可回归测试：
//! 1. 把件链的高度加起来（板 / 垫圈 / 螺母 …）；
//! 2. 按"露出 N 扣"算出所需长度，并在该螺栓族的**表内供货长度系列**里取 ≥ 的最小值；
//!    表内系列是厂家实际可购档（GB/T 是推荐标准，厂家常做"接近标准"的增减档，例如 M8
//!    C 级除 40 起还有 35），所以**以表为准**、不按推标公称系列卡；取不到就报该族范围
//!    并提示换族（例如 C 级 M8 装不下 → 建议 5783 全螺纹）；
//! 3. 沿轴线把每个零件放到它该在的位置（基点约定来自零件族：螺栓 = 头部支承面 × 轴线、
//!    螺母/垫圈 = 端面 × 轴线）。
//!
//! **刻意不判断"该不该加平垫/弹垫/防松件"** —— 那取决于安装面材料、振动环境、企业规范，
//! 由 skill/工况层决定并把件链传进来；命令侧只负责算得对、放得准、能撤销。
//!
//! 命令形式（给 MCP/AI 一行驱动）：
//! ```text
//! OCSMJOINT at 50,10 rot -90 [protrude 2.5] bolt=<族>:<d>[:<l>] [plate=<t>|gap=<t>|<垫圈/螺母族>=<d>]
//! ```
//! 例：`OCSMJOINT at 50,10 rot -90 bolt=hex_bolt_b_full:8 plate=10 plate=10 washer=washer_971:8 washer=washer_93:8 nut=nut_c41:8`

use crate::partgen::{self, family_kind};

/// 件链里的一件。
#[derive(Debug, Clone, PartialEq)]
pub enum JointItem {
    /// 螺栓/螺钉：决定轴线与长度基准（必须位于件链首位）。
    Bolt { family: String, d: f64, l: Option<f64> },
    Nut { family: String, d: f64 },
    Washer { family: String, d: f64 },
    /// 被连接件（按厚度记）：命令不去量它，厚度由调用方给（skill 从选区读）。
    Plate { t: f64 },
    /// 通用间隔件（如垫板、间隙）。
    Gap { t: f64 },
}

impl JointItem {
    /// 该件沿轴线占的高度；螺栓返回 None（它是长度基准，不计入 Σ）。
    fn height(&self) -> Option<f64> {
        match self {
            JointItem::Bolt { .. } => None,
            JointItem::Plate { t } | JointItem::Gap { t } => Some(*t),
            JointItem::Nut { family, d } | JointItem::Washer { family, d } => {
                partgen::size_row(family, *d).map(|row| row.l_min)
            }
        }
    }

    fn describe(&self) -> String {
        match self {
            JointItem::Bolt { family, d, l } => {
                let (name, code) = partgen::family_meta(family);
                match l {
                    Some(l) => format!("{name} {code} {}×{}", label_d(*d), trim(*l)),
                    None => format!("{name} {code} {}", label_d(*d)),
                }
            }
            JointItem::Nut { family, d } => {
                let (name, code) = partgen::family_meta(family);
                match partgen::size_row(family, *d) {
                    Some(row) => format!("{name} {code} {}(m{})", label_d(*d), trim(row.l_min)),
                    None => format!("{name} {code} {}", label_d(*d)),
                }
            }
            JointItem::Washer { family, d } => {
                let (name, code) = partgen::family_meta(family);
                match partgen::size_row(family, *d) {
                    Some(row) => format!("{name} {code} {}(厚{})", label_d(*d), trim(row.l_min)),
                    None => format!("{name} {code} {}", label_d(*d)),
                }
            }
            JointItem::Plate { t } => format!("板厚 {}", trim(*t)),
            JointItem::Gap { t } => format!("间隔 {}", trim(*t)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JointSpec {
    /// 螺栓头部支承面所在的点（世界坐标 XY）。
    pub at: [f64; 2],
    /// 轴线方向（度，逆时针）：螺栓杆指向该方向，其余件沿它依次排开。
    pub rot_deg: f64,
    /// 期望露出螺母端的扣数（1 扣 = 1 个螺距），默认 2.5。
    pub protrude_turns: f64,
    /// 视图覆盖（可选）：默认全族用 `main`——螺栓/螺母是“轴线水平”的侧视，
    /// 垫圈的主视图也已按用户要求改为侧视轮廓（平垫=矩形 h×d2、弹垫=月牙）。
    pub view: Option<String>,
    pub items: Vec<JointItem>,
}

impl JointSpec {
    pub const DEFAULT_PROTRUDE_TURNS: f64 = 2.5;

    /// 解析命令行参数（`at/rot/protrude` + 件链）；失败给出用法。
    pub fn parse(args: &str) -> Result<Self, String> {
        let usage = "用法：OCSMJOINT at x,y rot 度 [protrude 扣数] \
                     bolt=<族>:<d>[:<l>] [plate=<厚>|gap=<厚>|<螺母或垫圈族>=<d>] …";
        let mut tokens = args.split_whitespace();
        let mut at: Option<[f64; 2]> = None;
        let mut rot_deg = 0.0f64;
        let mut protrude_turns = Self::DEFAULT_PROTRUDE_TURNS;
        let mut view: Option<String> = None;
        let mut items: Vec<JointItem> = Vec::new();
        while let Some(token) = tokens.next() {
            let lower = token.to_ascii_lowercase();
            match lower.as_str() {
                "at" | "@" => {
                    let first = tokens.next().ok_or_else(|| format!("at 缺坐标。{usage}"))?;
                    let (x, y) = match first.split_once(',') {
                        Some((x, y)) => (number(x)?, number(y)?),
                        None => (
                            number(&first)?,
                            number(&tokens.next().ok_or_else(|| format!("at 缺 y。{usage}"))?)?,
                        ),
                    };
                    at = Some([x, y]);
                }
                "rot" | "rotation" => {
                    let raw = tokens.next().ok_or_else(|| format!("rot 缺角度。{usage}"))?;
                    rot_deg = number(&raw)?;
                }
                "protrude" => {
                    let raw = tokens.next().ok_or_else(|| format!("protrude 缺数值。{usage}"))?;
                    protrude_turns = number(&raw)?;
                    if protrude_turns < 0.0 {
                        return Err("protrude 不能为负（扣数）".into());
                    }
                }
                "view" => {
                    let raw = tokens.next().ok_or_else(|| format!("view 缺视图名。{usage}"))?;
                    view = Some(raw.to_ascii_lowercase());
                }
                _ => items.push(parse_item(token)?),
            }
        }
        let at = at.ok_or_else(|| format!("缺 at x,y。{usage}"))?;
        if items.is_empty() {
            return Err(format!("件链为空。{usage}"));
        }
        if !matches!(items.first(), Some(JointItem::Bolt { .. })) {
            return Err("件链必须以 bolt=<族>:<d> 开头（它决定轴线与长度基准）。".into());
        }
        Ok(JointSpec { at, rot_deg, protrude_turns, view, items })
    }

    /// HTTP/JSON 形式（`POST /api/joint` 用）。
    pub fn from_json(body: &[u8]) -> Result<Self, String> {
        #[derive(serde::Deserialize)]
        struct Raw {
            at: [f64; 2],
            #[serde(default)]
            rot: f64,
            #[serde(default = "default_protrude")]
            protrude: f64,
            #[serde(default)]
            view: Option<String>,
            items: Vec<RawItem>,
        }
        #[derive(serde::Deserialize)]
        struct RawItem {
            kind: String,
            #[serde(default)]
            family: String,
            #[serde(default)]
            d: f64,
            #[serde(default)]
            l: Option<f64>,
            #[serde(default)]
            t: f64,
        }
        fn default_protrude() -> f64 {
            JointSpec::DEFAULT_PROTRUDE_TURNS
        }
        let raw: Raw = serde_json::from_slice(body).map_err(|e| format!("请求 JSON 无效: {e}"))?;
        let items = raw
            .items
            .into_iter()
            .map(|item| {
                Ok(match item.kind.to_ascii_lowercase().as_str() {
                    "bolt" => JointItem::Bolt { family: item.family, d: item.d, l: item.l },
                    "nut" => JointItem::Nut { family: item.family, d: item.d },
                    "washer" => JointItem::Washer { family: item.family, d: item.d },
                    "plate" => JointItem::Plate { t: item.t },
                    "gap" => JointItem::Gap { t: item.t },
                    other => return Err(format!("未知件类型: {other}")),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        if !matches!(items.first(), Some(JointItem::Bolt { .. })) {
            return Err("件链必须以 kind=bolt 开头".into());
        }
        Ok(JointSpec { at: raw.at, rot_deg: raw.rot, protrude_turns: raw.protrude, view: raw.view, items })
    }

    pub fn to_json(&self) -> String {
        let items: Vec<serde_json::Value> = self
            .items
            .iter()
            .map(|item| match item {
                JointItem::Bolt { family, d, l } => serde_json::json!({"kind":"bolt","family":family,"d":d,"l":l}),
                JointItem::Nut { family, d } => serde_json::json!({"kind":"nut","family":family,"d":d}),
                JointItem::Washer { family, d } => serde_json::json!({"kind":"washer","family":family,"d":d}),
                JointItem::Plate { t } => serde_json::json!({"kind":"plate","t":t}),
                JointItem::Gap { t } => serde_json::json!({"kind":"gap","t":t}),
            })
            .collect();
        serde_json::json!({
            "at": self.at,
            "rot": self.rot_deg,
            "protrude": self.protrude_turns,
            "view": self.view,
            "items": items,
        })
        .to_string()
    }
}

/// 单件放置：基点坐标 + 视角（弧度）+ 该件的规格（插入用）。
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub family: String,
    pub d: f64,
    pub l: f64,
    pub at: [f64; 3],
    pub rot_rad: f64,
    /// 人类可读的规格文本（如 "M8x35"）。
    pub spec: String,
    /// 该件采用的视图 id（默认 `main`，见 `JointSpec::view`）。
    pub view: String,
}

#[derive(Debug, Clone)]
pub struct JointPlan {
    pub stack: f64,
    /// 长度需求（Σ + 露出）。
    pub need: f64,
    /// 实际外露长度（mm）与扣数。
    pub protrude_mm: f64,
    pub bolt_family: String,
    pub bolt_d: f64,
    pub bolt_l: f64,
    pub placements: Vec<Placement>,
    /// 一行推理（写进命令行日志，便于审计）。
    pub report: String,
}

/// 计算整个件链：长度落点（以表内供货系列为准）+ 各件基点。
pub fn plan(spec: &JointSpec) -> Result<JointPlan, String> {
    let Some(JointItem::Bolt { family, d, l: explicit_l }) = spec.items.first().cloned() else {
        return Err("件链必须以 bolt=<族>:<d> 开头".into());
    };
    let row = partgen::size_row(&family, d).ok_or_else(|| {
        format!("零件库没有 {family} 的 {}，可用直径：{}", label_d(d), diameters_hint(&family))
    })?;
    if row.pitch <= 0.0 {
        return Err(format!("{family} {} 没有螺距数据，无法按扣数算露出", label_d(d)));
    }

    // ① Σ 件高（不含螺栓本身）
    let mut stack = 0.0f64;
    let mut chain_text: Vec<String> = Vec::new();
    for item in spec.items.iter().skip(1) {
        let h = item.height().ok_or_else(|| format!("{} 的规格在零件库里查不到", item.describe()))?;
        stack += h;
        chain_text.push(item.describe());
    }

    // ② 长度：Σ + 露出扣数×螺距，落标准长度系列
    let protrude_want = spec.protrude_turns * row.pitch;
    let need = stack + protrude_want;
    let bolt_l = match explicit_l {
        Some(l) => l,
        None => {
            let candidates: Vec<f64> = if row.lengths.is_empty() {
                // 表里没列 lengths 就按 5 递增在 [l_min, l_max] 内生成（少数族如此）。
                let mut v = Vec::new();
                let mut l = row.l_min;
                while l <= row.l_max + 1e-9 {
                    v.push(l);
                    l += 5.0;
                }
                v
            } else {
                row.lengths.clone()
            };
            candidates.iter().copied().find(|l| *l >= need - 1e-9).ok_or_else(|| {
                format!(
                    "{family} {} 的供货长度最大 {}，装不下（需 ≥ {}）。换更长的族或改件链。",
                    label_d(d),
                    trim(row.l_max),
                    trim(need)
                )
            })?
        }
    };
    let protrude_mm = bolt_l - stack;
    let protrude_turns = protrude_mm / row.pitch;

    // ③ 沿轴线排布：螺栓基点 = 头部支承面；其余件基点 = 前一序列的累积高度处
    let rot_rad = spec.rot_deg.to_radians();
    let axis = [rot_rad.cos(), rot_rad.sin()];
    let mut placements = Vec::new();
    let mut offset = 0.0f64;
    for (index, item) in spec.items.iter().enumerate() {
        let (family, d, l, spec_text) = match item {
            JointItem::Bolt { family, d, .. } => (family.clone(), *d, bolt_l, format!("{}{}", label_d(*d), format!("x{}", trim(bolt_l)))),
            JointItem::Nut { family, d } => (family.clone(), *d, 0.0, label_d(*d)),
            JointItem::Washer { family, d } => (family.clone(), *d, 0.0, label_d(*d)),
            JointItem::Plate { t } | JointItem::Gap { t } => {
                offset += *t;
                continue;
            }
        };
        let at = [
            spec.at[0] + axis[0] * offset,
            spec.at[1] + axis[1] * offset,
            0.0,
        ];
        let view = spec.view.clone().unwrap_or_else(|| "main".to_string());
        placements.push(Placement { family, d, l, at, rot_rad, spec: spec_text, view });
        if index > 0 {
            // 该件自身占用 offset 之后的空间
            offset += item.height().unwrap_or(0.0);
        }
    }

    let (bolt_name, bolt_code) = partgen::family_meta(&family);
    let chain_note = if chain_text.is_empty() {
        "无被连接件".to_string()
    } else {
        chain_text.join(" + ")
    };
    let report = format!(
        "OCSMJOINT：{bolt_name} {bolt_code} {}×{}｜件链 {chain_note} → Σ{}｜需 l ≥ {}(= Σ{} + 露出{}扣×{}) → 取供货长度 {}（实际外露 {}mm≈{}扣）｜基点 ({}, {}) rot {}°｜共 {} 件",
        label_d(d),
        trim(bolt_l),
        trim(stack),
        trim(need),
        trim(stack),
        trim(spec.protrude_turns),
        trim(row.pitch),
        trim(bolt_l),
        trim(protrude_mm),
        trim(protrude_turns.round_at(1)),
        trim(spec.at[0]),
        trim(spec.at[1]),
        trim(spec.rot_deg),
        placements.len(),
    );

    Ok(JointPlan {
        stack,
        need,
        protrude_mm,
        bolt_family: family,
        bolt_d: d,
        bolt_l,
        placements,
        report,
    })
}

trait RoundAt {
    fn round_at(self, digits: u32) -> f64;
}
impl RoundAt for f64 {
    fn round_at(self, digits: u32) -> f64 {
        let factor = 10f64.powi(digits as i32);
        (self * factor).round() / factor
    }
}

fn diameters_hint(family: &str) -> String {
    let list: Vec<String> = partgen::family_sizes(family).iter().map(|r| label_d(r.d)).collect();
    if list.is_empty() {
        "（该族未实现）".to_string()
    } else {
        list.join(", ")
    }
}

fn parse_item(token: &str) -> Result<JointItem, String> {
    let (key, value) = token
        .split_once('=')
        .ok_or_else(|| format!("件格式应为 `<类型>=<族>:<规格>` 或 `plate=<厚>`（收到 {token}）"))?;
    let key = key.to_ascii_lowercase();
    match key.as_str() {
        "plate" | "p" | "板" => return Ok(JointItem::Plate { t: number(value)? }),
        "gap" | "g" | "间隔" => return Ok(JointItem::Gap { t: number(value)? }),
        _ => {}
    }
    // 写法①：直接写族名：`hex_bolt_c=8[:40]`、`nut_c41=8`、`washer_971=8`
    if partgen::family_kind(&key) != "other" {
        let (d, l) = parse_size(value)?;
        return item_for(key, d, l);
    }
    // 写法②：件类型 + 族名：`bolt=hex_bolt_b_full:8[:40]`、`nut=nut_c41:8`
    let (family, rest) = value.split_once(':').ok_or_else(|| {
        format!("{token} 应为 <类型>=<族>:<规格>，如 bolt=hex_bolt_b_full:8")
    })?;
    let family = family.to_ascii_lowercase();
    let (d, l) = parse_size(rest)?;
    let expected = match key.as_str() {
        "bolt" | "screw" | "screw_bolt" => "bolt",
        "nut" => "nut",
        "washer" => "washer",
        other => return Err(format!("未知件类型 {other}（可用 bolt/nut/washer/plate/gap，或直接写族名）")),
    };
    if partgen::family_kind(&family) != expected {
        return Err(format!("{family} 不是{expected}族"));
    }
    item_for(family, d, l)
}

/// `d[:l]`（可选显式长度）。
fn parse_size(raw: &str) -> Result<(f64, Option<f64>), String> {
    let mut parts = raw.split(':');
    let d = number(parts.next().unwrap_or(""))?;
    let l = match parts.next() {
        Some(text) if !text.trim().is_empty() => Some(number(text)?),
        _ => None,
    };
    if parts.next().is_some() {
        return Err(format!("规格最多 <d>[:<l>]（收到 {raw}）"));
    }
    Ok((d, l))
}

fn item_for(family: String, d: f64, l: Option<f64>) -> Result<JointItem, String> {
    if partgen::size_row(&family, d).is_none() {
        return Err(format!("零件库没有 {family} 的 M{}（可用：{}）", trim(d), diameters_hint(&family)));
    }
    Ok(match partgen::family_kind(&family) {
        "bolt" => JointItem::Bolt { family, d, l },
        "nut" => JointItem::Nut { family, d },
        "washer" => JointItem::Washer { family, d },
        other => return Err(format!("{family} 是 {other} 类，件链里只支持螺栓/螺母/垫圈")),
    })
}

fn number(raw: &str) -> Result<f64, String> {
    let value: f64 = raw
        .trim()
        .trim_end_matches(['m', 'M', '°', '度'])
        .parse()
        .map_err(|_| format!("不是有效数字: {raw}"))?;
    if !value.is_finite() {
        return Err(format!("不是有限数字: {raw}"));
    }
    Ok(value)
}

fn label_d(d: f64) -> String {
    format!("M{}", trim(d))
}

fn trim(v: f64) -> String {
    let mut text = format!("{v:.3}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_command_line_forms() {
        let spec = JointSpec::parse(
            "at 50,10 rot -90 bolt=hex_bolt_b_full:8 plate=10 plate=10 nut=nut_c41:8",
        )
        .unwrap();
        assert_eq!(spec.at, [50.0, 10.0]);
        assert_eq!(spec.rot_deg, -90.0);
        assert_eq!(spec.protrude_turns, 2.5);
        assert_eq!(spec.items.len(), 4);
        assert_eq!(spec.items[0], JointItem::Bolt { family: "hex_bolt_b_full".into(), d: 8.0, l: None });

        // 空格分隔的 at、显式长度、大小写混写
        let spec = JointSpec::parse("AT 10 20 ROT 90 BOLT=HEX_BOLT_B_FULL:8:40 plate=5").unwrap();
        assert_eq!(spec.at, [10.0, 20.0]);
        assert_eq!(spec.rot_deg, 90.0);
        assert_eq!(spec.items[0], JointItem::Bolt { family: "hex_bolt_b_full".into(), d: 8.0, l: Some(40.0) });

        // 缺 at / 件链不以 bolt 开头 / 未知族 → 报错
        assert!(JointSpec::parse("bolt=hex_bolt_c:8 plate=10").is_err());
        assert!(JointSpec::parse("at 0,0 plate=10").is_err());
        assert!(JointSpec::parse("at 0,0 bolt=nut_c41:8").is_err());
    }

    #[test]
    fn plan_lands_on_standard_length_like_the_20mm_stack_case() {
        // 用户工况：两块 10mm 板 + M8（Ø10 孔）；GB/T 5783 全螺纹、GB/T 41 螺母 m=7.9
        let spec = JointSpec::parse(
            "at 50,10 rot -90 bolt=hex_bolt_b_full:8 plate=10 plate=10 nut=nut_c41:8",
        )
        .unwrap();
        let plan = plan(&spec).unwrap();
        assert!((plan.stack - 27.9).abs() < 1e-9, "Σ={}", plan.stack);
        assert!((plan.need - (27.9 + 2.5 * 1.25)).abs() < 1e-9, "need={}", plan.need);
        assert!((plan.bolt_l - 35.0).abs() < 1e-9, "应落 35（5783 M8 系列 30 不够 31.03）");
        // 放置：螺栓基点 = 头部支承面；螺母基点 = Σ 处（板厚累计 20）
        assert_eq!(plan.placements.len(), 2);
        assert_eq!(plan.placements[0].at, [50.0, 10.0, 0.0]);
        assert_eq!(plan.placements[1].at, [50.0, -10.0, 0.0]);
        assert!((plan.placements[1].rot_rad + std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert!(plan.report.contains("5783"), "{}", plan.report);
    }

    #[test]
    fn plan_handles_washers_and_reports_protrusion() {
        // 加两侧平垫 + 弹垫：Σ = 10+10+1.6+2.1+7.9 = 31.6 → need 34.7 → 35
        let spec = JointSpec::parse(
            "at 0,0 rot 0 bolt=hex_bolt_b_full:8 plate=10 plate=10 washer=washer_971:8 washer=washer_93:8 nut=nut_c41:8",
        )
        .unwrap();
        let built = plan(&spec).unwrap();
        assert!((built.stack - 31.6).abs() < 1e-9, "Σ={}", built.stack);
        assert!((built.bolt_l - 35.0).abs() < 1e-9, "l={}", built.bolt_l);
        // 件链顺序（rot 0 → 沿 +x）：螺栓@0 → 板10 → 板10 → 平垫@20 → 弹垫@21.6 → 螺母@23.7
        let offsets: Vec<f64> = built.placements.iter().map(|p| p.at[0]).collect();
        let expected = [0.0, 20.0, 21.6, 23.7];
        assert_eq!(offsets.len(), expected.len());
        for (got, want) in offsets.iter().zip(expected.iter()) {
            assert!((got - want).abs() < 1e-9, "{offsets:?} vs {expected:?}");
        }
        // 全族都用主视图：垫圈的主视图已改为侧视轮廓（2026-09-15 互换后）
        let views: Vec<&str> = built.placements.iter().map(|p| p.view.as_str()).collect();
        assert_eq!(views, vec!["main", "main", "main", "main"], "{views:?}");
        // 显式 view 覆盖全部件
        let forced = JointSpec::parse("at 0,0 view top bolt=hex_bolt_b_full:8 plate=10 nut=nut_c41:8").unwrap();
        assert!(plan(&forced).unwrap().placements.iter().all(|p| p.view == "top"));
    }

    #[test]
    fn plan_rejects_impossible_bolt_family() {
        // 5780 C 级 M8 的库内系列最短 35；件链超过它就必须报错而不是硬塞
        let spec = JointSpec::parse("at 0,0 bolt=hex_bolt_c:8 plate=40 plate=40 nut=nut_c41:8").unwrap();
        let err = plan(&spec).unwrap_err();
        assert!(err.contains("装不下") || err.contains("需"), "{err}");
    }
}
