//! **轴生成器（一期骨架）**：行 DSL / JSON → 单视图侧视图（`OCSMSHAFT`）。
//!
//! 本文件只做三件事：**解析**、**段拼接几何**、**落层/放置**。
//! 不标尺寸、不打剖面线、不建块；GUI 与更复杂特征（键槽/退刀槽/螺纹/齿轮/中心孔）
//! 留二期——行 DSL 里出现这些关键字会明确报「二期未实现」，不静默忽略。
//!
//! ## 行 DSL（一行一段，从左到右拼接）
//!
//! ```text
//! # 注释行；空行忽略；大小写不敏感；段内关键字顺序无关
//! S30 E30 L45 CH2@L
//! S40 E40 L30 CH2@R OV3
//! S50 E30 L20
//! S30 E30 L15 CH2@R
//! ```
//!
//! - `S` 起始直径（靠左）、`E` 终点直径（省略 = 圆柱段，`E=S`）、`L` 段长（必给）；
//! - `CH2@L` / `CH2@R`：端面倒角 C2，贴该段左/右端（`@` 省略默认右端）；
//! - `OV` / `OV3` / `OV3@L`：磨外圆砂轮越程槽（GB/T 6403.5），不写值 = 按该端
//!   直径查表；`@` 省略默认右端；
//! - 多段可用 `|` 或换行分隔；行尾可跟放置参数 `at x,y rot 度`；
//! - 解析错误报「第 N 行（第 k 段）：…」，几何错误报「第 N 段：…」。
//!
//! ## 几何口径（单视图侧视图）
//!
//! 轴线 = x 轴（x 向右、y = 半径），第 1 段左端面在 x=0；上下对称：
//! - 每段上下各一条线：圆柱 = 水平线，圆锥 = 斜线；
//! - 段间端面 / 两端外端面用竖直线闭合（直径相同且无特征时不画）；
//! - **倒角贴端面凸角**：相邻段更大 → 倒角落在相邻（大）段一侧；相邻段更小或
//!   本段是自由端 → 倒角落在本段一侧。这样 `S40 E40 L30 CH2@R OV3`（右端有
//!   越程槽 + 相邻 φ50 台阶）里倒角落在 φ50 上，两个特征不重叠——这是本期的
//!   定案口径（待用户复核）。
//! - `OV` 复用 `detail.rs` 的查表与画法（R 圆角/槽底/45° 斜坡/砂轮细线），
//!   只要求台阶面一侧更高、且槽落在圆柱端；
//! - 落层：轮廓/端面/倒角 → `1轮廓实线层`；砂轮细线 → `2细线层`；
//!   轴线 → `3中心线层`（点划线，长度 = 总长 + 图框比例 × 6，两端各半）。
//!
//! ## JSON（同一模型，给以后的 GUI/HTTP）
//!
//! ```json
//! {"segments":[{"s":30,"e":30,"l":45,"ch":[{"c":2,"end":"L"}]}],"at":[100,50],"rot":30}
//! ```

use ocs_plugin_api::host::acadrust::entities::EntityType;
use ocs_plugin_api::host::acadrust::types::Vector3;
use serde::Deserialize;

use crate::detail;
use crate::partgen_kit::{line, trim, LAYER_CENTER, LAYER_MAIN};

/// `OCSMSHAFT` 不带参数时打印的用法。
pub const USAGE: &str = "\
OCSMSHAFT 轴生成器（一期骨架）：行 DSL / JSON → 单视图侧视图（段拼接 + 端面倒角 + 砂轮越程槽）。
用法：OCSMSHAFT <行 DSL 或 JSON>
  行 DSL：一行一段，从左到右拼接；多段用 | 或换行分隔；大小写不敏感、段内关键字顺序无关
    S 起始直径（靠左）   E 终点直径（省略 = 圆柱段 E=S）   L 段长（必给）
    CH2@L / CH2@R   端面倒角 C2（@ 省略默认 R）    OV / OV3 / OV3@L   砂轮越程槽
    at x,y rot 度   放置（不写 = 原点、不转）
  例：OCSMSHAFT S30 E30 L45 CH2@L | S40 E40 L30 CH2@R OV3 | S50 E30 L20 | S30 E30 L15 CH2@R at 100,50 rot 30
  JSON：{\"segments\":[{\"s\":30,\"e\":30,\"l\":45,\"ch\":[{\"c\":2,\"end\":\"L\"}]}],\"at\":[100,50],\"rot\":30}
不标尺寸、不打剖面线；轮廓 → 1轮廓实线层，砂轮细线 → 2细线层，轴线 → 3中心线层。";

// ══════════════════════════════════════════════════════════════════════════
// 数据模型
// ══════════════════════════════════════════════════════════════════════════

/// 特征所在端。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    L,
    R,
}

fn end_cn(end: End) -> &'static str {
    match end {
        End::L => "左",
        End::R => "右",
    }
}

/// 端面倒角 C（45°：沿轴向 C、半径方向 C）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chamfer {
    pub c: f64,
    pub end: End,
}

/// 砂轮越程槽：`b1 = None` = 按该端直径查 GB 表；`Some` = 显式槽宽。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Overtravel {
    pub b1: Option<f64>,
    pub end: End,
}

/// 一段轴（从左到右）。
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// 起始直径（左端）。
    pub s: f64,
    /// 终点直径（右端）。
    pub e: f64,
    /// 段长。
    pub l: f64,
    /// 倒角（同一端最多一个）。
    pub ch: Vec<Chamfer>,
    /// 越程槽（同一端最多一个）。
    pub ov: Vec<Overtravel>,
}

/// 解析结果：段序列 + 放置参数（`at` / `rot` 由命令层应用）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub segments: Vec<Segment>,
    pub at: Option<[f64; 2]>,
    pub rot: Option<f64>,
}

impl Program {
    /// 总长。
    pub fn total_length(&self) -> f64 {
        self.segments.iter().map(|s| s.l).sum()
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 解析：行 DSL
// ══════════════════════════════════════════════════════════════════════════

/// 解析行 DSL 或 JSON（首字符 `{` = JSON）。返回的几何还没做合法性检查，
/// 校验在 [`validate`] / [`build`] 里（这样测试与 GUI 能分别拿到解析层错误）。
pub fn parse_program(text: &str) -> Result<Program, String> {
    if text.trim_start().starts_with('{') {
        return parse_json(text);
    }
    let mut program = Program::default();
    // 全文一个「逻辑段」计数器对不上的情况：同一行用 `|` 分多段时单独标「第 k 段」。
    for (line_idx, raw_line) in text.lines().enumerate() {
        let line_no = line_idx + 1;
        // 注释：整行 `#` 或行内 `#` 之后都忽略。
        let line = raw_line.split('#').next().unwrap_or("");
        if line.trim().is_empty() {
            continue;
        }
        let chunks: Vec<&str> = line.split('|').collect();
        let multi = chunks.iter().filter(|c| !c.trim().is_empty()).count() > 1;
        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            let chunk = chunk.trim();
            if chunk.is_empty() {
                continue;
            }
            let label = if multi {
                format!("第 {line_no} 行第 {} 段", chunk_idx + 1)
            } else {
                format!("第 {line_no} 行")
            };
            let first = chunk.split_whitespace().next().unwrap_or("");
            if first.eq_ignore_ascii_case("at") || first == "@" {
                parse_placement(&chunk.split_whitespace().collect::<Vec<_>>(), &label, &mut program)?;
                continue;
            }
            let seg = parse_segment(chunk, &label, &mut program)?;
            program.segments.push(seg);
        }
    }
    if program.segments.is_empty() {
        return Err("没有解析到任何轴段（至少给一段 `S… L…`）".into());
    }
    Ok(program)
}

fn unknown_keyword(token: &str, label: &str) -> String {
    format!("{label}：关键字「{token}」二期未实现（本期只支持 S/E/L/CH/OV）")
}

fn starts_number(s: &str) -> bool {
    matches!(s.chars().next(), Some(c) if c.is_ascii_digit() || c == '+' || c == '-' || c == '.')
}

/// 关键字后面是否跟了「像数值」的尾巴（空 = 缺值，另行报错；字母开头 = 未知关键字）。
fn looks_like_keyword_value(rest: &str) -> bool {
    let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
    rest.is_empty() || starts_number(rest)
}

fn parse_end(text: &str) -> Result<End, String> {
    match text.to_ascii_uppercase().as_str() {
        "L" | "LEFT" | "左" => Ok(End::L),
        "R" | "RIGHT" | "右" => Ok(End::R),
        other => Err(other.to_string()),
    }
}

/// `2@L` → `("2", Some("L"))`；`3` → `("3", None)`。
fn split_end(rest: &str) -> (&str, Option<&str>) {
    match rest.split_once('@') {
        Some((value, end)) => (value, Some(end)),
        None => (rest, None),
    }
}

fn parse_end_token(end: Option<&str>, label: &str, what: &str) -> Result<End, String> {
    match end {
        None => Ok(End::R),
        Some(text) if text.is_empty() => {
            Err(format!("{label}：关键字 {what} 的端别缺省（@ 后要 L 或 R）"))
        }
        Some(text) => parse_end(text)
            .map_err(|bad| format!("{label}：端别「{bad}」非法（只能用 L 或 R）")),
    }
}

fn parse_number(value: &str, label: &str, what: &str) -> Result<f64, String> {
    value
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{label}：关键字 {what} 的值「{value}」不是数字"))
}

fn parse_diameter(token: &str, rest: &str, what: &str, label: &str) -> Result<f64, String> {
    let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
    if rest.is_empty() {
        return Err(format!("{label}：关键字 {what} 缺少数值"));
    }
    if !starts_number(rest) {
        return Err(unknown_keyword(token, label));
    }
    parse_number(rest, label, what)
}

fn parse_ch(token: &str, rest: &str, label: &str) -> Result<Chamfer, String> {
    let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
    if rest.is_empty() {
        return Err(format!(
            "{label}：关键字 CH 缺少数值（写法 CH2 / CH2@L / CH2@R）"
        ));
    }
    if !starts_number(rest) {
        return Err(unknown_keyword(token, label));
    }
    let (value, end) = split_end(rest);
    let c = parse_number(value, label, "CH")?;
    if c <= 0.0 {
        return Err(format!(
            "{label}：关键字 CH 的 C={} 非法（必须 > 0）",
            trim(c)
        ));
    }
    Ok(Chamfer {
        c,
        end: parse_end_token(end, label, "CH")?,
    })
}

fn parse_ov(token: &str, rest: &str, label: &str) -> Result<Overtravel, String> {
    let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
    let (value, end) = split_end(rest);
    let b1 = if value.is_empty() {
        None
    } else {
        if !starts_number(value) {
            return Err(unknown_keyword(token, label));
        }
        let b1 = value
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("{label}：关键字 OV 的值「{value}」不是数字"))?;
        if b1 <= 0.0 {
            return Err(format!(
                "{label}：关键字 OV 的 b1={} 非法（必须 > 0）",
                trim(b1)
            ));
        }
        Some(b1)
    };
    Ok(Overtravel {
        b1,
        end: parse_end_token(end, label, "OV")?,
    })
}

fn parse_segment(chunk: &str, label: &str, program: &mut Program) -> Result<Segment, String> {
    let tokens: Vec<&str> = chunk.split_whitespace().collect();
    let (mut s, mut e, mut l) = (None, None, None);
    let mut ch: Vec<Chamfer> = Vec::new();
    let mut ov: Vec<Overtravel> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        // 放置参数只允许出现在一段的末尾（`… at x,y rot 30`），吃掉剩余 token。
        if token.eq_ignore_ascii_case("at") || token == "@" {
            parse_placement(&tokens[index..], label, program)?;
            break;
        }
        let upper = token.to_ascii_uppercase();
        if upper.starts_with("CH") {
            let item = parse_ch(token, &token[2..], label)?;
            if ch.iter().any(|x| x.end == item.end) {
                return Err(format!(
                    "{label}：关键字 CH 在{}端重复",
                    end_cn(item.end)
                ));
            }
            ch.push(item);
        } else if upper.starts_with("OV") {
            let item = parse_ov(token, &token[2..], label)?;
            if ov.iter().any(|x| x.end == item.end) {
                return Err(format!(
                    "{label}：关键字 OV 在{}端重复",
                    end_cn(item.end)
                ));
            }
            ov.push(item);
        } else if upper.starts_with('S') {
            if !looks_like_keyword_value(&token[1..]) {
                return Err(unknown_keyword(token, label));
            }
            if s.is_some() {
                return Err(format!("{label}：关键字 S 重复"));
            }
            s = Some(parse_diameter(token, &token[1..], "S", label)?);
        } else if upper.starts_with('E') {
            if !looks_like_keyword_value(&token[1..]) {
                return Err(unknown_keyword(token, label));
            }
            if e.is_some() {
                return Err(format!("{label}：关键字 E 重复"));
            }
            e = Some(parse_diameter(token, &token[1..], "E", label)?);
        } else if upper.starts_with('L') {
            if !looks_like_keyword_value(&token[1..]) {
                return Err(unknown_keyword(token, label));
            }
            if l.is_some() {
                return Err(format!("{label}：关键字 L 重复"));
            }
            l = Some(parse_diameter(token, &token[1..], "L", label)?);
        } else {
            return Err(unknown_keyword(token, label));
        }
        index += 1;
    }
    let s = s.ok_or_else(|| format!("{label}：缺少 S（起始直径）"))?;
    // E 省略 = 圆柱段；L 必给。
    let l = l.ok_or_else(|| format!("{label}：缺少 L（段长）"))?;
    Ok(Segment {
        s,
        e: e.unwrap_or(s),
        l,
        ch,
        ov,
    })
}

fn is_rot(token: &str) -> bool {
    token.eq_ignore_ascii_case("rot") || token.eq_ignore_ascii_case("rotation")
}

fn number_or_err(text: &str, what: &str, label: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{label}：{what}「{text}」不是数字"))
}

fn parse_placement(tokens: &[&str], label: &str, program: &mut Program) -> Result<(), String> {
    if program.at.is_some() || program.rot.is_some() {
        return Err(format!("{label}：放置参数 at/rot 重复"));
    }
    let mut index = 0;
    if index < tokens.len() && (tokens[index].eq_ignore_ascii_case("at") || tokens[index] == "@") {
        index += 1;
    }
    if index < tokens.len() && !is_rot(tokens[index]) {
        let token = tokens[index];
        let (x, y) = if let Some((a, b)) = token.split_once(',') {
            (
                number_or_err(a, "at 坐标", label)?,
                number_or_err(b, "at 坐标", label)?,
            )
        } else {
            let b = tokens
                .get(index + 1)
                .ok_or_else(|| format!("{label}：at 缺 y 坐标（写 at x,y）"))?;
            if is_rot(b) {
                return Err(format!("{label}：at 缺 y 坐标（写 at x,y）"));
            }
            (
                number_or_err(token, "at 坐标", label)?,
                number_or_err(b, "at 坐标", label)?,
            )
        };
        program.at = Some([x, y]);
        index += if token.contains(',') { 1 } else { 2 };
    }
    if index < tokens.len() && is_rot(tokens[index]) {
        index += 1;
        let value = tokens
            .get(index)
            .ok_or_else(|| format!("{label}：rot 缺少数值"))?;
        program.rot = Some(number_or_err(value, "rot", label)?);
        index += 1;
    }
    if index < tokens.len() {
        return Err(format!(
            "{label}：无法识别的词「{}」（放置段只认 at x,y rot 度）",
            tokens[index]
        ));
    }
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════
// 解析：JSON（同一模型）
// ══════════════════════════════════════════════════════════════════════════

/// 字段允许单对象或数组（`"ov":{"b1":3}` 与 `"ov":[{"b1":3}]` 都收）。
fn one_or_many<'de, D, T>(de: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Raw<T> {
        One(T),
        Many(Vec<T>),
    }
    Ok(match Option::<Raw<T>>::deserialize(de)? {
        None => Vec::new(),
        Some(Raw::One(value)) => vec![value],
        Some(Raw::Many(values)) => values,
    })
}

#[derive(serde::Deserialize)]
struct JsonProgram {
    #[serde(default)]
    segments: Vec<JsonSegment>,
    #[serde(default, deserialize_with = "json_point")]
    at: Option<[f64; 2]>,
    #[serde(default)]
    rot: Option<f64>,
}

#[derive(serde::Deserialize)]
struct JsonSegment {
    s: f64,
    #[serde(default)]
    e: Option<f64>,
    l: f64,
    #[serde(default, deserialize_with = "one_or_many")]
    ch: Vec<JsonChamfer>,
    #[serde(default, deserialize_with = "one_or_many")]
    ov: Vec<JsonOvertravel>,
}

#[derive(serde::Deserialize)]
struct JsonChamfer {
    c: f64,
    #[serde(default)]
    end: Option<String>,
}

#[derive(serde::Deserialize)]
struct JsonOvertravel {
    #[serde(default)]
    b1: Option<f64>,
    #[serde(default)]
    end: Option<String>,
}

/// `at` 允许 `[x,y]` / `"x,y"` / `{"x":…,"y":…}`。
fn json_point<'de, D>(de: D) -> Result<Option<[f64; 2]>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Pair([f64; 2]),
        Text(String),
        Xy { x: f64, y: f64 },
    }
    match Option::<Raw>::deserialize(de)? {
        None => Ok(None),
        Some(Raw::Pair(pair)) => Ok(Some(pair)),
        Some(Raw::Xy { x, y }) => Ok(Some([x, y])),
        Some(Raw::Text(text)) => {
            let (x, y) = text
                .split_once(',')
                .ok_or_else(|| serde::de::Error::custom("at 字符串要写成 \"x,y\""))?;
            let x = x
                .trim()
                .parse::<f64>()
                .map_err(|_| serde::de::Error::custom("at 的 x 不是数字"))?;
            let y = y
                .trim()
                .parse::<f64>()
                .map_err(|_| serde::de::Error::custom("at 的 y 不是数字"))?;
            Ok(Some([x, y]))
        }
    }
}

fn parse_json(text: &str) -> Result<Program, String> {
    let raw: JsonProgram =
        serde_json::from_str(text).map_err(|e| format!("JSON 解析失败：{e}"))?;
    if raw.segments.is_empty() {
        return Err("JSON 里没有 segments（至少给一段）".into());
    }
    let mut segments = Vec::with_capacity(raw.segments.len());
    for (index, item) in raw.segments.iter().enumerate() {
        let number = index + 1;
        let mut ch = Vec::new();
        for (k, value) in item.ch.iter().enumerate() {
            let end = match &value.end {
                None => End::R,
                Some(text) => parse_end(text).map_err(|bad| {
                    format!("第 {number} 段：ch[{k}] 的端别「{bad}」非法（只能用 L 或 R）")
                })?,
            };
            if ch.iter().any(|x: &Chamfer| x.end == end) {
                return Err(format!("第 {number} 段：ch 在{}端重复", end_cn(end)));
            }
            ch.push(Chamfer { c: value.c, end });
        }
        let mut ov = Vec::new();
        for (k, value) in item.ov.iter().enumerate() {
            let end = match &value.end {
                None => End::R,
                Some(text) => parse_end(text).map_err(|bad| {
                    format!("第 {number} 段：ov[{k}] 的端别「{bad}」非法（只能用 L 或 R）")
                })?,
            };
            if ov.iter().any(|x: &Overtravel| x.end == end) {
                return Err(format!("第 {number} 段：ov 在{}端重复", end_cn(end)));
            }
            ov.push(Overtravel { b1: value.b1, end });
        }
        segments.push(Segment {
            s: item.s,
            e: item.e.unwrap_or(item.s),
            l: item.l,
            ch,
            ov,
        });
    }
    if raw.rot.is_some_and(|v| !v.is_finite()) {
        return Err("JSON：rot 不是有限数".into());
    }
    Ok(Program {
        segments,
        at: raw.at,
        rot: raw.rot,
    })
}

// ══════════════════════════════════════════════════════════════════════════
// 合法性检查
// ══════════════════════════════════════════════════════════════════════════

/// 与端面无关的逐段检查（段长/直径/倒角/重复特征）。
/// 端面之间的检查（倒角直径变化量、越程槽台阶…）在 [`build`] 里做。
pub fn validate(program: &Program) -> Result<(), String> {
    if program.segments.is_empty() {
        return Err("至少要有一段（S… L…）".into());
    }
    for (index, seg) in program.segments.iter().enumerate() {
        let number = index + 1;
        if !seg.l.is_finite() || seg.l <= 0.0 {
            return Err(format!(
                "第 {number} 段：段长 L={} 必须 > 0",
                trim(seg.l)
            ));
        }
        if !seg.s.is_finite() || seg.s <= 0.0 {
            return Err(format!(
                "第 {number} 段：起始直径 S={} 必须 > 0",
                trim(seg.s)
            ));
        }
        if !seg.e.is_finite() || seg.e <= 0.0 {
            return Err(format!(
                "第 {number} 段：终点直径 E={} 必须 > 0",
                trim(seg.e)
            ));
        }
        for chamfer in &seg.ch {
            if !chamfer.c.is_finite() || chamfer.c <= 0.0 {
                return Err(format!(
                    "第 {number} 段：倒角 C={} 必须 > 0",
                    trim(chamfer.c)
                ));
            }
            if chamfer.c >= seg.l / 2.0 {
                return Err(format!(
                    "第 {number} 段：{}端倒角 C={} ≥ 段长/2（l={}），特征重叠",
                    end_cn(chamfer.end),
                    trim(chamfer.c),
                    trim(seg.l)
                ));
            }
        }
        for (a, first) in seg.ch.iter().enumerate() {
            for second in &seg.ch[a + 1..] {
                if first.end == second.end {
                    return Err(format!(
                        "第 {number} 段：倒角 CH 在{}端重复",
                        end_cn(first.end)
                    ));
                }
            }
        }
        for ov in &seg.ov {
            if let Some(b1) = ov.b1 {
                if !b1.is_finite() || b1 <= 0.0 {
                    return Err(format!(
                        "第 {number} 段：越程槽 b1={} 必须 > 0",
                        trim(b1)
                    ));
                }
                if b1 > seg.l + 1e-9 {
                    return Err(format!(
                        "第 {number} 段：越程槽 b1={} > 段长 l={}",
                        trim(b1),
                        trim(seg.l)
                    ));
                }
            }
        }
        for (a, first) in seg.ov.iter().enumerate() {
            for second in &seg.ov[a + 1..] {
                if first.end == second.end {
                    return Err(format!(
                        "第 {number} 段：越程槽 OV 在{}端重复",
                        end_cn(first.end)
                    ));
                }
            }
        }
    }
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════
// 几何
// ══════════════════════════════════════════════════════════════════════════

/// 构建结果。
#[derive(Debug, Clone, PartialEq)]
pub struct Shaft {
    pub entities: Vec<EntityType>,
    pub total_length: f64,
    pub max_diameter: f64,
    pub segment_count: usize,
}

/// 段内某 x 处的轮廓半径（上半侧；圆锥按线性插值）。
fn radius_at(seg: &Segment, x0: f64, x: f64) -> f64 {
    let r0 = seg.s / 2.0;
    let r1 = seg.e / 2.0;
    r0 + (r1 - r0) * ((x - x0) / seg.l)
}

/// 某端做 C 倒角：返回（轮廓点，端面点）。端面点 = 轮廓点在段内 C 处 − C（45°）。
fn chamfer_geom(seg: &Segment, x0: f64, end: End, c: f64) -> ([f64; 2], [f64; 2]) {
    match end {
        End::L => {
            let x_contour = x0 + c;
            let y_contour = radius_at(seg, x0, x_contour);
            ([x_contour, y_contour], [x0, y_contour - c])
        }
        End::R => {
            let x_face = x0 + seg.l;
            let x_contour = x_face - c;
            let y_contour = radius_at(seg, x0, x_contour);
            ([x_contour, y_contour], [x_face, y_contour - c])
        }
    }
}

/// 端面一侧的竖直平移。
fn translate_x(entity: EntityType, dx: f64) -> EntityType {
    match entity {
        EntityType::Line(mut l) => {
            l.start.x += dx;
            l.end.x += dx;
            EntityType::Line(l)
        }
        EntityType::Arc(mut a) => {
            a.center.x += dx;
            EntityType::Arc(a)
        }
        other => other,
    }
}

/// 局部坐标（端面在 x=0）→ 镜像到端面 `face_x` 左侧：`x' = face_x − x`。
/// 圆弧镜像后保持 DXF 的逆时针读法（角度按 `π − θ` 处理）。
fn mirror_x(entity: EntityType, face_x: f64) -> EntityType {
    match entity {
        EntityType::Line(mut l) => {
            l.start.x = face_x - l.start.x;
            l.end.x = face_x - l.end.x;
            EntityType::Line(l)
        }
        EntityType::Arc(mut a) => {
            a.center.x = face_x - a.center.x;
            let (start, end) = (a.start_angle, a.end_angle);
            a.start_angle = std::f64::consts::PI - end;
            a.end_angle = std::f64::consts::PI - start;
            EntityType::Arc(a)
        }
        other => other,
    }
}

/// 解析 → 校验 → 生成（`frame_scale` 用于轴线 `6n` 伸出量；无图框传 1.0）。
pub fn build(program: &Program, frame_scale: f64) -> Result<Shaft, String> {
    validate(program)?;
    let segs = &program.segments;
    let count = segs.len();

    // 每段起点 x（第 1 段左端面 = 0）。
    let mut x0s = Vec::with_capacity(count);
    let mut cursor = 0.0;
    for seg in segs {
        x0s.push(cursor);
        cursor += seg.l;
    }
    let total = program.total_length();
    let max_diameter = segs
        .iter()
        .map(|s| s.s.max(s.e))
        .fold(0.0_f64, f64::max);

    // 倒角 / 越程槽落在**本段自己**身上的量（决定本段轮廓线被吃掉多少）。
    let mut own_ch = vec![[None::<f64>; 2]; count]; // [左, 右]
    let mut own_ov = vec![[None::<f64>; 2]; count];
    let mut entities: Vec<EntityType> = Vec::new();

    // ── 内部端面（第 i 段右端 ↔ 第 i+1 段左端） ──
    for j in 0..count.saturating_sub(1) {
        let i = j;
        let k = j + 1;
        let x_face = x0s[k];
        let ra = segs[i].e / 2.0;
        let rb = segs[k].s / 2.0;
        let ch_r = segs[i].ch.iter().find(|c| c.end == End::R).map(|c| c.c);
        let ch_l = segs[k].ch.iter().find(|c| c.end == End::L).map(|c| c.c);
        let (chamfer, ch_from_left) = match (ch_r, ch_l) {
            (Some(_), Some(_)) => {
                return Err(format!(
                    "第 {} 段：右端倒角与第 {} 段左端倒角落在同一端面，特征重叠",
                    i + 1,
                    k + 1
                ))
            }
            (Some(c), None) => (Some(c), true),
            (None, Some(c)) => (Some(c), false),
            (None, None) => (None, false),
        };
        let ov_r = segs[i].ov.iter().find(|o| o.end == End::R).copied();
        let ov_l = segs[k].ov.iter().find(|o| o.end == End::L).copied();
        if ov_r.is_some() && ov_l.is_some() {
            return Err(format!(
                "第 {} 段：右端与第 {} 段左端都写了越程槽（同一端面只能一侧）",
                i + 1,
                k + 1
            ));
        }

        let mut bottom = ra.min(rb);
        let mut top = ra.max(rb);
        let mut chamfer_lines: Option<([f64; 2], [f64; 2])> = None;

        if let Some(c) = chamfer {
            let requester = if ch_from_left { i } else { k };
            let requester_end = if ch_from_left { "右" } else { "左" };
            if (ra - rb).abs() < 1e-12 {
                return Err(format!(
                    "第 {} 段：{}端没有端面（相邻段直径相同），无法倒角",
                    requester + 1,
                    requester_end
                ));
            }
            let delta = (ra - rb).abs();
            if c >= delta - 1e-12 {
                return Err(format!(
                    "第 {} 段：{}端倒角 C={} ≥ 端面直径变化量的一半（Ø{} → Ø{} 的 {}），端面被吃掉",
                    requester + 1,
                    requester_end,
                    trim(c),
                    trim(ra * 2.0),
                    trim(rb * 2.0),
                    trim(delta)
                ));
            }
            // 倒角贴**凸角**（大的一侧）。
            let (contour, face_point) = if rb > ra {
                chamfer_geom(&segs[k], x0s[k], End::L, c)
            } else {
                chamfer_geom(&segs[i], x0s[i], End::R, c)
            };
            if face_point[1] <= bottom + 1e-9 {
                return Err(format!(
                    "第 {} 段：{}端倒角与相邻面重叠（端面点 {} ≤ {}）",
                    requester + 1,
                    requester_end,
                    trim(face_point[1]),
                    trim(bottom)
                ));
            }
            top = face_point[1];
            chamfer_lines = Some((contour, face_point));
            if rb > ra {
                own_ch[k][0] = Some(c);
            } else {
                own_ch[i][1] = Some(c);
            }
        }

        if let Some(ov) = ov_r {
            // 槽开在第 i 段（磨出的外圆），台阶 = 第 i+1 段。
            if !(rb > ra + 1e-12) {
                return Err(format!(
                    "第 {} 段：右端越程槽没有台阶面（相邻段 Ø{} 不大于本段 Ø{}）",
                    i + 1,
                    trim(rb * 2.0),
                    trim(ra * 2.0)
                ));
            }
            if (segs[i].s - segs[i].e).abs() > 1e-9 {
                return Err(format!("第 {} 段：右端是锥面，越程槽只能开在圆柱端", i + 1));
            }
            let (groove, row) = detail::groove_entities(segs[i].e, ov.b1)
                .map_err(|e| format!("第 {} 段：右端越程槽：{e}", i + 1))?;
            if row.b1 > segs[i].l + 1e-9 {
                return Err(format!(
                    "第 {} 段：越程槽 b1={} > 段长 l={}",
                    i + 1,
                    trim(row.b1),
                    trim(segs[i].l)
                ));
            }
            let fillet = detail::fillet_tangent_radius(segs[i].e, row);
            if fillet >= top - 1e-9 {
                return Err(format!(
                    "第 {} 段：右端越程槽的 R 圆角切点 {} 不低于台阶面 {}（相邻台阶太小）",
                    i + 1,
                    trim(fillet),
                    trim(top)
                ));
            }
            bottom = fillet;
            own_ov[i][1] = Some(row.b1);
            entities.extend(groove.into_iter().map(|e| mirror_x(e, x_face)));
        } else if let Some(ov) = ov_l {
            // 槽开在第 k 段（磨出的外圆），台阶 = 第 i 段。
            if !(ra > rb + 1e-12) {
                return Err(format!(
                    "第 {} 段：左端越程槽没有台阶面（相邻段 Ø{} 不大于本段 Ø{}）",
                    k + 1,
                    trim(ra * 2.0),
                    trim(rb * 2.0)
                ));
            }
            if (segs[k].s - segs[k].e).abs() > 1e-9 {
                return Err(format!("第 {} 段：左端是锥面，越程槽只能开在圆柱端", k + 1));
            }
            let (groove, row) = detail::groove_entities(segs[k].s, ov.b1)
                .map_err(|e| format!("第 {} 段：左端越程槽：{e}", k + 1))?;
            if row.b1 > segs[k].l + 1e-9 {
                return Err(format!(
                    "第 {} 段：越程槽 b1={} > 段长 l={}",
                    k + 1,
                    trim(row.b1),
                    trim(segs[k].l)
                ));
            }
            let fillet = detail::fillet_tangent_radius(segs[k].s, row);
            if fillet >= top - 1e-9 {
                return Err(format!(
                    "第 {} 段：左端越程槽的 R 圆角切点 {} 不低于台阶面 {}（相邻台阶太小）",
                    k + 1,
                    trim(fillet),
                    trim(top)
                ));
            }
            bottom = fillet;
            own_ov[k][0] = Some(row.b1);
            entities.extend(groove.into_iter().map(|e| translate_x(e, x_face)));
        }

        if top - bottom > 1e-9 {
            entities.push(line([x_face, bottom], [x_face, top], LAYER_MAIN));
            entities.push(line([x_face, -bottom], [x_face, -top], LAYER_MAIN));
        }
        if let Some((contour, face_point)) = chamfer_lines {
            entities.push(line(face_point, contour, LAYER_MAIN));
            entities.push(line(
                [face_point[0], -face_point[1]],
                [contour[0], -contour[1]],
                LAYER_MAIN,
            ));
        }
    }

    // ── 左自由端（第 1 段左端面） ──
    if segs[0].ov.iter().any(|o| o.end == End::L) {
        return Err("第 1 段：左端是自由端，越程槽没有台阶面".into());
    }
    let left_face = if let Some(c) = segs[0].ch.iter().find(|c| c.end == End::L).map(|c| c.c) {
        let radius = segs[0].s / 2.0;
        if c >= radius - 1e-12 {
            return Err(format!(
                "第 1 段：左端倒角 C={} ≥ 端面半径（Ø{} 的 {}），端面被吃掉",
                trim(c),
                trim(segs[0].s),
                trim(radius)
            ));
        }
        let (contour, face_point) = chamfer_geom(&segs[0], 0.0, End::L, c);
        if face_point[1] <= 1e-9 {
            return Err(format!(
                "第 1 段：左端倒角 C={} 把端面吃穿了（端面点 {} ≤ 0）",
                trim(c),
                trim(face_point[1])
            ));
        }
        own_ch[0][0] = Some(c);
        entities.push(line(face_point, contour, LAYER_MAIN));
        entities.push(line(
            [face_point[0], -face_point[1]],
            [contour[0], -contour[1]],
            LAYER_MAIN,
        ));
        face_point[1]
    } else {
        segs[0].s / 2.0
    };
    entities.push(line([0.0, -left_face], [0.0, left_face], LAYER_MAIN));

    // ── 右自由端（最后一段右端面） ──
    let last = count - 1;
    let x_end = x0s[last] + segs[last].l;
    if segs[last].ov.iter().any(|o| o.end == End::R) {
        return Err(format!(
            "第 {} 段：右端是自由端，越程槽没有台阶面",
            last + 1
        ));
    }
    let right_face = if let Some(c) = segs[last]
        .ch
        .iter()
        .find(|c| c.end == End::R)
        .map(|c| c.c)
    {
        let radius = segs[last].e / 2.0;
        if c >= radius - 1e-12 {
            return Err(format!(
                "第 {} 段：右端倒角 C={} ≥ 端面半径（Ø{} 的 {}），端面被吃掉",
                last + 1,
                trim(c),
                trim(segs[last].e),
                trim(radius)
            ));
        }
        let (contour, face_point) = chamfer_geom(&segs[last], x0s[last], End::R, c);
        if face_point[1] <= 1e-9 {
            return Err(format!(
                "第 {} 段：右端倒角 C={} 把端面吃穿了（端面点 {} ≤ 0）",
                last + 1,
                trim(c),
                trim(face_point[1])
            ));
        }
        own_ch[last][1] = Some(c);
        entities.push(line(face_point, contour, LAYER_MAIN));
        entities.push(line(
            [face_point[0], -face_point[1]],
            [contour[0], -contour[1]],
            LAYER_MAIN,
        ));
        face_point[1]
    } else {
        segs[last].e / 2.0
    };
    entities.push(line([x_end, -right_face], [x_end, right_face], LAYER_MAIN));

    // ── 每段上下轮廓线（两端被倒角/越程槽吃掉多少已定） ──
    for (index, seg) in segs.iter().enumerate() {
        let start_shift = own_ch[index][0].unwrap_or(0.0).max(own_ov[index][0].unwrap_or(0.0));
        let end_shift = own_ch[index][1].unwrap_or(0.0).max(own_ov[index][1].unwrap_or(0.0));
        if start_shift + end_shift > seg.l + 1e-9 {
            return Err(format!(
                "第 {} 段：两端特征重叠（左 {} + 右 {} > 段长 {}）",
                index + 1,
                trim(start_shift),
                trim(end_shift),
                trim(seg.l)
            ));
        }
        let xs = x0s[index] + start_shift;
        let xe = x0s[index] + seg.l - end_shift;
        if xe - xs > 1e-9 {
            let ys = radius_at(seg, x0s[index], xs);
            let ye = radius_at(seg, x0s[index], xe);
            entities.push(line([xs, ys], [xe, ye], LAYER_MAIN));
            entities.push(line([xs, -ys], [xe, -ye], LAYER_MAIN));
        }
    }

    // ── 轴线（3中心线层）：长度 = 总长 + 图框比例 × 6，两端各伸出 3n ──
    let half_overhang = 3.0 * frame_scale;
    entities.push(line(
        [-half_overhang, 0.0],
        [total + half_overhang, 0.0],
        LAYER_CENTER,
    ));

    Ok(Shaft {
        entities,
        total_length: total,
        max_diameter,
        segment_count: count,
    })
}

/// 按 `at` / `rot`（度）放置：绕基点旋转后平移。基点 = **第 1 段轴线左端**。
pub fn place(entities: Vec<EntityType>, at: [f64; 2], rot_deg: f64) -> Vec<EntityType> {
    let rad = rot_deg.to_radians();
    let (cos, sin) = (rad.cos(), rad.sin());
    let map = move |p: [f64; 2]| {
        [
            at[0] + p[0] * cos - p[1] * sin,
            at[1] + p[0] * sin + p[1] * cos,
        ]
    };
    entities
        .into_iter()
        .map(|entity| match entity {
            EntityType::Line(mut l) => {
                let a = map([l.start.x, l.start.y]);
                let b = map([l.end.x, l.end.y]);
                l.start = Vector3::new(a[0], a[1], l.start.z);
                l.end = Vector3::new(b[0], b[1], l.end.z);
                EntityType::Line(l)
            }
            EntityType::Arc(mut a) => {
                let center = map([a.center.x, a.center.y]);
                a.center = Vector3::new(center[0], center[1], a.center.z);
                a.start_angle += rad;
                a.end_angle += rad;
                EntityType::Arc(a)
            }
            other => other,
        })
        .collect()
}

// ══════════════════════════════════════════════════════════════════════════
// 测试
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detail;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn has_line(shaft: &Shaft, a: [f64; 2], b: [f64; 2]) -> bool {
        shaft.entities.iter().any(|e| match e {
            EntityType::Line(l) => {
                let (p, q) = ([l.start.x, l.start.y], [l.end.x, l.end.y]);
                (near(p[0], a[0]) && near(p[1], a[1]) && near(q[0], b[0]) && near(q[1], b[1]))
                    || (near(p[0], b[0]) && near(p[1], b[1]) && near(q[0], a[0]) && near(q[1], a[1]))
            }
            _ => false,
        })
    }

    fn has_arc(shaft: &Shaft, center: [f64; 2], radius: f64, start_deg: f64, end_deg: f64) -> bool {
        shaft.entities.iter().any(|e| match e {
            EntityType::Arc(a) => {
                near(a.center.x, center[0])
                    && near(a.center.y, center[1])
                    && near(a.radius, radius)
                    && near(normalize_deg(a.start_angle.to_degrees()), normalize_deg(start_deg))
                    && near(normalize_deg(a.end_angle.to_degrees()), normalize_deg(end_deg))
            }
            _ => false,
        })
    }

    fn normalize_deg(deg: f64) -> f64 {
        deg.rem_euclid(360.0)
    }

    fn layer_of(entity: &EntityType) -> &str {
        entity.common().layer.as_str()
    }

    const DEMO: &str = "\
S30 E30 L45 CH2@L
S40 E40 L30 CH2@R OV3
S50 E30 L20
S30 E30 L15 CH2@R";

    // ── 解析 ──────────────────────────────────────────────────────────────

    #[test]
    fn dsl_demo_parses() {
        let program = parse_program(DEMO).unwrap();
        assert_eq!(program.segments.len(), 4);
        assert_eq!(program.at, None);
        assert_eq!(program.rot, None);
        assert_eq!(
            (program.segments[0].s, program.segments[0].e, program.segments[0].l),
            (30.0, 30.0, 45.0)
        );
        assert_eq!(
            program.segments[0].ch,
            vec![Chamfer { c: 2.0, end: End::L }]
        );
        assert_eq!(
            program.segments[1].ov,
            vec![Overtravel { b1: Some(3.0), end: End::R }]
        );
        // E 省略 = 圆柱段
        assert_eq!((program.segments[2].s, program.segments[2].e), (50.0, 30.0));
        // 关键字顺序无关 + 大小写不敏感
        let shuffled = parse_program("ch2@l l45 e30 s30").unwrap();
        assert_eq!(shuffled.segments[0], program.segments[0]);
        // OV 不带值 = 查表；OV@L 端别
        let ov = parse_program("S30 E30 L20 OV@L").unwrap();
        assert_eq!(
            ov.segments[0].ov,
            vec![Overtravel { b1: None, end: End::L }]
        );
        // 多段用 | 分隔 + 行尾放置
        let one_line =
            parse_program("S30 E30 L45 CH2@L | S40 E40 L30 OV at 100,50 rot 30").unwrap();
        assert_eq!(one_line.segments.len(), 2);
        assert_eq!(one_line.at, Some([100.0, 50.0]));
        assert_eq!(one_line.rot, Some(30.0));
    }

    #[test]
    fn dsl_errors_report_line_and_word() {
        // 第 2 行 OV b1=0
        let err = parse_program("S30 E30 L45\nS40 E40 L30 OV0").unwrap_err();
        assert!(err.contains("第 2 行"), "{err}");
        assert!(err.contains("OV") && err.contains("b1=0"), "{err}");
        // 未知关键字（含用户点名的 ES / GEAR / THREAD / M）→ 二期未实现 + 行号/词
        for token in ["ES", "GEAR", "THREAD", "M", "M10", "KEYWAY"] {
            let text = format!("S30 E30 L45\nS40 E40 L30 {token}");
            let err = parse_program(&text).unwrap_err();
            assert!(err.contains("第 2 行"), "{token}: {err}");
            assert!(err.contains(token), "{token}: {err}");
            assert!(err.contains("二期未实现"), "{token}: {err}");
        }
        // 同一行多段时标注「第 k 段」
        let err = parse_program("S30 E30 L10 | S40 L20 GEAR").unwrap_err();
        assert!(err.contains("第 1 行第 2 段") && err.contains("GEAR"), "{err}");
        // 注释行不影响物理行号
        let err = parse_program("# 注释\nS30 E30 L10\nOV0").unwrap_err();
        assert!(err.contains("第 3 行"), "{err}");
        // 其它语法错误
        assert!(parse_program("S30 E30").unwrap_err().contains("缺少 L"));
        assert!(parse_program("E30 L10").unwrap_err().contains("缺少 S"));
        assert!(parse_program("S30 E30 L10 CH2@X")
            .unwrap_err()
            .contains("端别"));
        assert!(parse_program("S30 E30 L10 S40")
            .unwrap_err()
            .contains("S 重复"));
        assert!(parse_program("S30 E30 L10 OV2 OV3")
            .unwrap_err()
            .contains("OV 在右端重复"));
        assert!(parse_program("S30 E30 L10 at x,1")
            .unwrap_err()
            .contains("at 坐标"));
        assert!(parse_program("S30 E30 L10 rot")
            .unwrap_err()
            .contains("rot"));
    }

    #[test]
    fn json_model_matches_dsl() {
        let json = r#"{"segments":[
            {"s":30,"e":30,"l":45,"ch":[{"c":2,"end":"L"}]},
            {"s":40,"e":40,"l":30,"ch":{"c":2,"end":"R"},"ov":{"b1":3,"end":"R"}},
            {"s":50,"e":30,"l":20},
            {"s":30,"e":30,"l":15,"ch":[{"c":2,"end":"R"}]}
        ],"at":[10,20],"rot":90}"#;
        let program = parse_program(json).unwrap();
        assert_eq!(program.segments.len(), 4);
        assert_eq!(program.at, Some([10.0, 20.0]));
        assert_eq!(program.rot, Some(90.0));
        assert_eq!(program.segments, parse_program(DEMO).unwrap().segments);
        // `at` 也接受 {"x":…,"y":…}
        let program = parse_program(r#"{"segments":[{"s":30,"l":10}],"at":{"x":1,"y":2}}"#).unwrap();
        assert_eq!(program.at, Some([1.0, 2.0]));
        assert!(parse_program("{not json").unwrap_err().contains("JSON"));
    }

    // ── 段拼接 ────────────────────────────────────────────────────────────

    #[test]
    fn segments_concatenate_total_diameter_and_cone() {
        let program = parse_program("S30 E30 L45\nS40 E30 L20\nS30 L15").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        assert!(near(shaft.total_length, 80.0));
        assert!(near(shaft.max_diameter, 40.0));
        assert_eq!(shaft.segment_count, 3);
        // 圆柱段：x=0..45 的水平线 y=±15
        assert!(has_line(&shaft, [0.0, 15.0], [45.0, 15.0]));
        // 圆锥段：x=45..65，Ø40 → Ø30（半径 20 → 15）
        assert!(has_line(&shaft, [45.0, 20.0], [65.0, 15.0]));
        assert!(has_line(&shaft, [45.0, -20.0], [65.0, -15.0]));
        // 台阶面 x=45：15 ↔ 20（上下两条）
        assert!(has_line(&shaft, [45.0, 15.0], [45.0, 20.0]));
        assert!(has_line(&shaft, [45.0, -15.0], [45.0, -20.0]));
        // 圆锥末端 Ø30 与第 3 段 Ø30 相同 → 不画内部端面线；第 3 段轮廓继续到 80
        assert!(has_line(&shaft, [65.0, 15.0], [80.0, 15.0]));
        assert!(has_line(&shaft, [80.0, -15.0], [80.0, 15.0]));
        // 轴两端外端面
        assert!(has_line(&shaft, [0.0, -15.0], [0.0, 15.0]));
        // 轴线长度 = 总长 + 6n
        assert!(has_line(&shaft, [-3.0, 0.0], [83.0, 0.0]));
    }

    #[test]
    fn chamfer_on_both_ends_of_a_segment() {
        let program = parse_program("S30 E30 L45 CH2@L CH2@R").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        // 左端倒角：端面缩到 ±13，45° 线 (0,13)→(2,15)
        assert!(has_line(&shaft, [0.0, -13.0], [0.0, 13.0]));
        assert!(has_line(&shaft, [0.0, 13.0], [2.0, 15.0]));
        assert!(has_line(&shaft, [0.0, -13.0], [2.0, -15.0]));
        // 右端倒角：端面缩到 ±13，45° 线 (43,15)→(45,13)
        assert!(has_line(&shaft, [45.0, -13.0], [45.0, 13.0]));
        assert!(has_line(&shaft, [43.0, 15.0], [45.0, 13.0]));
        assert!(has_line(&shaft, [43.0, -15.0], [45.0, -13.0]));
        // 轮廓缩短到 2..43
        assert!(has_line(&shaft, [2.0, 15.0], [43.0, 15.0]));
    }

    #[test]
    fn chamfer_on_a_shoulder_lands_on_the_convex_corner() {
        // 第 1 段 Ø30 → 第 2 段 Ø40；CH2@R 落在第 2 段（大）的左端
        let program = parse_program("S30 E30 L20 CH2@R\nS40 E40 L10").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        // 端面 x=20 从 15 到 18（20−2）；倒角线 (20,18)→(22,20)
        assert!(has_line(&shaft, [20.0, 15.0], [20.0, 18.0]));
        assert!(has_line(&shaft, [20.0, 18.0], [22.0, 20.0]));
        // 第 2 段轮廓从 x=22 开始
        assert!(has_line(&shaft, [22.0, 20.0], [30.0, 20.0]));
        // 等直径端面不画内部线：把 CH 挪到不产生倒角的地方仍然没有 x=20 的 15↔15 线
        let flat = build(&parse_program("S30 E30 L20\nS30 E30 L10").unwrap(), 1.0).unwrap();
        assert!(!has_line(&flat, [20.0, -15.0], [20.0, 15.0]));
    }

    // ── 越程槽 ────────────────────────────────────────────────────────────

    #[test]
    fn overtravel_groove_keeps_detail_geometry_on_both_ends() {
        // 右端槽：第 1 段 Ø50（地面）→ 第 2 段 Ø60（台阶）；OV 默认右端 + 查表
        let program = parse_program("S50 E50 L20 OV\nS60 E60 L10").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let row = detail::row_for(50.0, None).unwrap();
        let (b1, h, r) = (row.b1, row.h, row.r);
        let fillet = 25.0 + r - h;
        // 右端面 x=20：槽向下镜像，圆角心 (20−r, fillet)，角度 270→360
        assert!(has_arc(&shaft, [20.0 - r, fillet], r, 270.0, 360.0));
        assert!(has_arc(&shaft, [20.0 - r, -fillet], r, 0.0, 90.0));
        // 槽底 + 45° 斜坡
        assert!(has_line(&shaft, [20.0 - b1 + h, 25.0 - h], [20.0 - r, 25.0 - h]));
        assert!(has_line(&shaft, [20.0 - b1, 25.0], [20.0 - b1 + h, 25.0 - h]));
        // 端面线从圆角切点画到台阶顶
        assert!(has_line(&shaft, [20.0, fillet], [20.0, 30.0]));
        // 轮廓从 x=0 到槽的斜坡外侧（20−b1），不是到端面
        assert!(has_line(&shaft, [0.0, 25.0], [20.0 - b1, 25.0]));
        // 砂轮细线：镜像后 45° 上扬到左上方，落 2细线层
        assert!(has_line(
            &shaft,
            [20.0 - b1, 25.0],
            [20.0 - b1 - detail::WHEEL_TAIL, 25.0 + detail::WHEEL_TAIL]
        ));
        let thin: Vec<&str> = shaft
            .entities
            .iter()
            .filter(|e| layer_of(e) == crate::partgen_kit::LAYER_THIN)
            .map(layer_of)
            .collect();
        assert_eq!(thin.len(), 1, "砂轮细线只有一条且落 2细线层");
        // OV@L：地面 = 第 2 段，台阶 = 第 1 段
        let program = parse_program("S60 E60 L10\nS50 E50 L20 OV@L").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let row = detail::row_for(50.0, None).unwrap();
        let (b1, h, r) = (row.b1, row.h, row.r);
        let fillet = 25.0 + r - h;
        assert!(has_arc(&shaft, [10.0 + r, fillet], r, 180.0, 270.0));
        assert!(has_line(&shaft, [10.0, fillet], [10.0, 30.0]));
        assert!(has_line(&shaft, [10.0 + b1, 25.0], [30.0, 25.0]));
        assert!(has_line(
            &shaft,
            [10.0 + b1, 25.0],
            [10.0 + b1 + detail::WHEEL_TAIL, 25.0 + detail::WHEEL_TAIL]
        ));
    }

    // ── 合法性 ────────────────────────────────────────────────────────────

    #[test]
    fn validity_errors_point_at_the_segment() {
        let cases: &[(&str, &str)] = &[
            ("S30 E30 L0", "第 1 段"),
            ("S30 E30 L-5", "第 1 段"),
            ("S0 E30 L10", "第 1 段"),
            ("S30 E-1 L10", "第 1 段"),
            ("S30 E30 L10 CH5@L", "段长/2"),
            ("S30 E30 L20 CH10@L", "段长/2"),
            // C ≥ 直径变化量的一半（Ø30→Ø40 的 5）
            ("S30 E30 L20 CH5@R\nS40 E40 L10", "直径变化量"),
            // 直径相同 → 没有端面可倒角
            ("S30 E30 L20 CH2@R\nS30 E30 L10", "没有端面"),
            // b1 > 段长（显式 3 > 2）
            ("S50 E50 L2 OV3\nS60 E60 L10", "b1=3 > 段长"),
            // b1 > 段长（查表默认 5 > 4；d=60 落 50<d<100 档）
            ("S60 E60 L4 OV\nS70 E70 L10", "b1=5 > 段长"),
            // 显式 b1 不在该 d 档的表里
            ("S40 E40 L20 OV4\nS50 E50 L10", "可选 b1"),
            // 两端槽重叠（2 + 2 > 3；两档都能取 b1=2）
            ("S60 E60 L10\nS50 E50 L3 OV@L OV@R\nS60 E60 L10", "两端特征重叠"),
            // 没有台阶（相邻段更小）
            ("S50 E50 L20 OV\nS40 E40 L10", "没有台阶面"),
            // 自由端不能做槽
            ("S50 E50 L20 OV", "自由端"),
            // 台阶太小：R 圆角切点（25.6）高过相邻台阶面（25.5）
            ("S50 E50 L20 OV\nS51 E51 L10", "台阶太小"),
            // 锥面不能做槽
            ("S50 E30 L20 OV\nS60 E60 L10", "锥面"),
            // 陡锥自由端倒角可能把端面吃穿（C < 端面半径也会发生）
            ("S10 E0.2 L10 CH4.9@L", "吃穿"),
            // 同一端面两侧都写倒角
            ("S30 E30 L20 CH2@R\nS40 E40 L10 CH2@L", "特征重叠"),
        ];
        for (text, needle) in cases {
            let program = parse_program(text).unwrap_or_else(|e| panic!("{text}: 解析失败 {e}"));
            let err = build(&program, 1.0).unwrap_err();
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
    }

    // ── 轴线 / 放置 ───────────────────────────────────────────────────────

    #[test]
    fn axis_length_is_total_plus_6n() {
        let program = parse_program("S30 E30 L45\nS40 E30 L20").unwrap();
        for (n, total, start, end) in [(1.0, 71.0, -3.0, 68.0), (2.0, 77.0, -6.0, 71.0)] {
            let shaft = build(&program, n).unwrap();
            let axis = shaft
                .entities
                .iter()
                .filter_map(|e| match e {
                    EntityType::Line(l) if l.common.layer == LAYER_CENTER => {
                        Some((l.start.x, l.end.x))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(axis.len(), 1, "轴线只有一条");
            let (start_x, end_x) = axis[0];
            assert!(near(end_x - start_x, total), "n={n}: {start_x}..{end_x} 长 {}", end_x - start_x);
            assert!(near(start_x, start) && near(end_x, end), "n={n}: {start_x}..{end_x}");
        }
    }

    #[test]
    fn place_rotates_and_translates() {
        let program = parse_program("S30 E30 L10").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let placed = place(shaft.entities, [100.0, 50.0], 0.0);
        let has = |entities: &Vec<EntityType>, a: [f64; 2], b: [f64; 2]| {
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
        };
        assert!(has(&placed, [97.0, 50.0], [113.0, 50.0]), "平移");
        let rotated = place(placed, [0.0, 0.0], 90.0);
        // 轴线 (97,50)..(113,50) 绕原点转 90° → (−50,97)..(−50,113)
        assert!(has(&rotated, [-50.0, 97.0], [-50.0, 113.0]), "旋转");
    }

    // ── 交付：demo 几何 dump（人工核对用） ────────────────────────────────

    /// 把 4 段 demo 的几何落成 CSV（列：entity,x1,y1,x2,y2,layer,cx,cy,r,a0,a1），
    /// 给用户画叠合/预览图。LINE 用两端点；ARC 用起终点 + 圆心/半径/角度。
    #[test]
    fn dump_demo_geometry_csv() {
        let program = parse_program(DEMO).unwrap();
        let shaft = build(&program, 1.0).unwrap();
        assert!(near(shaft.total_length, 110.0));

        let mut csv = String::from("entity,x1,y1,x2,y2,layer,cx,cy,r,a0,a1\n");
        for entity in &shaft.entities {
            match entity {
                EntityType::Line(l) => csv.push_str(&format!(
                    "LINE,{:.6},{:.6},{:.6},{:.6},{},,,,,\n",
                    l.start.x, l.start.y, l.end.x, l.end.y, l.common.layer
                )),
                EntityType::Arc(a) => {
                    let sx = a.center.x + a.radius * a.start_angle.cos();
                    let sy = a.center.y + a.radius * a.start_angle.sin();
                    let ex = a.center.x + a.radius * a.end_angle.cos();
                    let ey = a.center.y + a.radius * a.end_angle.sin();
                    csv.push_str(&format!(
                        "ARC,{:.6},{:.6},{:.6},{:.6},{},{:.6},{:.6},{:.6},{:.4},{:.4}\n",
                        sx,
                        sy,
                        ex,
                        ey,
                        a.common.layer,
                        a.center.x,
                        a.center.y,
                        a.radius,
                        a.start_angle.to_degrees(),
                        a.end_angle.to_degrees()
                    ));
                }
                other => panic!("demo 里只应出现 LINE/ARC，得到 {other:?}"),
            }
        }

        // 关键几何自检（demo 的 4 段：30 / 40（右端 OV3 + φ50 台阶倒角）/ 锥 50→30 / 30）
        assert!(has_line(&shaft, [72.0, 20.0], [57.0, 35.0]), "φ40 的 OV 砂轮细线");
        assert!(
            has_line(&shaft, [75.0, 22.0], [77.0, 24.0]),
            "φ50 台阶左端的 CH2（贴凸角）"
        );

        let path = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            .join("桌面/OCSM/review");
        std::fs::create_dir_all(&path).expect("建 review 目录");
        let file = path.join("shaft_demo.csv");
        std::fs::write(&file, &csv).expect("写 shaft_demo.csv");
        assert!(file.is_file(), "demo CSV 已落盘：{}", file.display());
        assert!(csv.contains("LINE") && csv.contains("ARC"));
        assert!(csv.contains("3中心线层") && csv.contains("2细线层"));
        // 28 = 轮廓 8 + 端面/倒角 12 + 槽体 7 + 轴线 1
        assert_eq!(shaft.entities.len(), 28, "demo 图元数");
    }
}
