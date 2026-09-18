//! **轴生成器**：行 DSL / JSON → 单视图侧视图（`OCSMSHAFT`）。
//!
//! 本文件只做三件事：**解析**、**段拼接几何**、**落层/放置**。
//! 不标尺寸、不打剖面线、不建块；更复杂特征（键槽/中心孔）仍留二期——
//! 行 DSL 里出现这些关键字会明确报「二期未实现」，不静默忽略。
//!
//! ## 行 DSL（一行一段，从左到右拼接）
//!
//! ```text
//! # 注释行；空行忽略；大小写不敏感；段内关键字顺序无关
//! S30 E30 L45 CH2@L
//! S40 E40 L30 CH2@R OV3
//! S50 E30 L20
//! S30 E30 L15 CH2@R
//! S40 E40 L7 M1.5
//! S36 E36 L5
//! GEAR M3 Z20
//! ```
//!
//! - `S` 起始直径（靠左）、`E` 终点直径（省略 = 圆柱段，`E=S`）、`L` 段长（必给）；
//! - `CH2@L` / `CH2@R`：端面倒角 C2，贴该段左/右端（`@` 省略默认右端）；
//! - `OV` / `OV3` / `OV3@L`：磨外圆砂轮越程槽（GB/T 6403.5），不写值 = 按该端
//!   直径查表；`@` 省略默认右端；
//! - `M` / `M1.5`：**螺纹段标记**（外螺纹侧视图）。不写值 = 简化画法小径 0.85d；
//!   `M1.5` = 给螺距 P，小径 = d − 1.0825P（精确）；只能标在圆柱段（S=E）上，
//!   与 `OV`/`GEAR` 同段报错、可与 `CH` 同段；
//! - **退刀槽不设专门关键字** —— 它就是一小段小直径轴段，例如螺纹段后的
//!   `S24 E24 L5`（φ24 = 槽底、L5 = 槽宽）；`ES5*3` 这类旧写法会报错并指路；
//! - `GEAR M5 Z10 H20`：**齿轮段（直齿）**，分度圆 d = m·z 由参数导出、**不给
//!   S/E**；H = 齿宽（省略 = 10m）；按齿轮工具 `side_view()` 的轴向投影口径：
//!   **只画齿顶线（= 该段轮廓，粗实线）+ 分度线（`3中心线层` 点划线），不画齿根线**；
//!   齿形用 `OCSMGEAR` 单独出，这里不画齿、本期不做斜齿（`BETA…` 报二期）；
//! - `VIEW 常规|剖视|双`：视图开关（默认 `常规`）；独立一行或段内关键字都认
//!   （`VIEW 剖视` / `VIEW=section`），只影响整体视图（`双` = 常规+剖视并排一次出）；
//! - 多段可用 `|` 或换行分隔；行尾可跟放置参数 `at x,y rot 度`；
//! - 解析错误报「第 N 行（第 k 段）：…」，几何错误报「第 N 段：…」。
//!
//! ## 螺纹段画法口径（外螺纹侧视图，GB 简化画法）
//!
//! 大径 = 段直径（`1轮廓实线层`，即该段上下轮廓，已有）；小径上下各一条
//! **细实线**（`2细线层`，半径 = 小径/2）贯穿该段；螺纹终止线 = 该段末端竖线
//! （复用既有端面线，不重复画），小径细实线止于终止线；同段端面若有倒角、且
//! 倒角切得比小径还深，细实线改止于倒角斜线交点（不挑出材料外）。
//! `M` 段必须是圆柱（S==E），锥面螺纹报错。
//!
//! ## 齿轮段画法口径（侧视图，与 `gear.rs::side_view()` 的轴向投影一致）
//!
//! 只画齿顶线 ra = d/2 + m（= `1轮廓实线层`，即该段上下轮廓）与分度线 r = d/2
//! （`3中心线层`，点划线），上下各一条、画在齿顶线内部；**不画齿根线** —— 与齿轮
//! 工具 `side_view()` 一模一样（它也只画齿顶轮廓 + 两条分度线 + 轴线）。派生尺寸
//! 取 `gear.rs` 同口径（ha*=1、c*=0.25、Xn=0 → ra = da/2、rf = df/2），不自己另立
//! 公式（rf 仍用于口径换算，只是不落图）。齿轮段与相邻段的过渡按台阶处理（不做
//! 过渡圆角）；**相邻段轮廓半径 > ra 会盖住齿顶线**，报「第 N 段」；≤ ra 一律放行
//! （不再受齿根圆 rf 限制）。
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
//! - 落层：轮廓/端面/倒角 → `1轮廓实线层`；砂轮细线与**螺纹小径细实线** →
//!   `2细线层`；轴线与分度线 → `3中心线层`（点划线，轴线长度 = 总长 +
//!   图框比例 × 6，两端各半）；`剖视` 的剖面线 → `5剖面线层`。
//!
//! ## 视图口径（`VIEW`）
//!
//! * `常规`（默认）= 只画外形可见线（就是上面这套几何，无剖面线）；
//! * `剖视` = **同一套轮廓 + 剖面线**：ANSI31、比例 1.0，落 `5剖面线层`，走
//!   `partgen_kit::hatch_ansi31_edges` 通路；边界 = 轴剖面外轮廓 —— 上半轮廓
//!   （左→右）→ 右端面 → 下半轮廓（右→左）→ 左端面，闭合成**一个简单环**
//!   （上轮廓按 x 排序拼接，含倒角/台阶/越程槽圆角；内部有孔/齿槽时每个环
//!   单独一条 BoundaryPath，采样折线时按 `dedup_ring` 口径去重点）；
//! * `双` = 常规视图与剖视图并排一次生成（读取顺序：左常规、右剖视），
//!   间距 = `max(总长 × 15%, 40 × 图框比例)`。
//!
//! ## JSON（同一模型，给 GUI/HTTP：`/api/shaft_parse` / `/api/shaft_preview` / `/api/shaft_export`）
//!
//! ```json
//! {"segments":[{"s":30,"e":30,"l":45,"ch":[{"c":2,"end":"L"}]},{"s":30,"e":30,"l":20,"thread":1.5}],"view":"section","at":[100,50],"rot":30}
//! ```

use ocs_plugin_api::host::acadrust::entities::hatch::BoundaryEdge;
use ocs_plugin_api::host::acadrust::entities::{EntityType, Hatch};
use ocs_plugin_api::host::acadrust::types::{Vector2, Vector3};
use serde::Deserialize;

use crate::detail;
use crate::gear::{GearKind, GearParams};
use crate::partgen_kit::{line, trim, HatchEdge, LAYER_CENTER, LAYER_HATCH, LAYER_MAIN, LAYER_THIN};

/// `OCSMSHAFT` 不带参数时打开轴生成器窗口；命令行带参数时此处是用法说明。
pub const USAGE: &str = "\
OCSMSHAFT 轴生成器：行 DSL / JSON → 单视图侧视图（段拼接 + 端面倒角 + 砂轮越程槽 + 螺纹段 M + 齿轮段 GEAR）。
用法：OCSMSHAFT <行 DSL 或 JSON>
  行 DSL：一行一段，从左到右拼接；多段用 | 或换行分隔；大小写不敏感、段内关键字顺序无关
    S 起始直径（靠左）   E 终点直径（省略 = 圆柱段 E=S）   L 段长（必给；齿轮段用 H 代替）
    CH2@L / CH2@R   端面倒角 C2（@ 省略默认 R）    OV / OV3 / OV3@L   砂轮越程槽
    M / M1.5   螺纹段：不写值 = 小径 0.85d；M1.5 = 螺距 P，小径 = d − 1.0825P（只能圆柱段）
               退刀槽 = 一小段小直径轴段（例 S24 E24 L5）；ES 已取消，写了会报错指路
    GEAR M5 Z10 H20   齿轮段（直齿）：d=m·z 导出、不给 S/E；H = 齿宽（省略 = 10m）
                      只画齿顶线（= 该段轮廓）+ 分度线（点划线），不画齿根线
    VIEW 常规|剖视|双   视图：常规（默认，只看外形）/ 剖视（轮廓 + ANSI31 剖面线）/ 双（并排一次出）
    at x,y rot 度   放置（不写 = 原点、不转）
  例：OCSMSHAFT S30 E30 L45 CH2@L | S40 E40 L30 CH2@R OV3 | S50 E30 L20 | S30 E30 L15 CH2@R | S40 E40 L7 M1.5 | S36 E36 L5 | GEAR M3 Z20 VIEW 剖视 at 100,50 rot 30
  JSON：{\"segments\":[{\"s\":30,\"e\":30,\"l\":45,\"ch\":[{\"c\":2,\"end\":\"L\"}]},{\"s\":30,\"e\":30,\"l\":20,\"thread\":1.5}],\"view\":\"section\",\"at\":[100,50],\"rot\":30}
轮廓/端面/倒角 → 1轮廓实线层，砂轮细线/螺纹小径 → 2细线层，轴线/分度线 → 3中心线层，剖视剖面线 → 5剖面线层。";

// ══════════════════════════════════════════════════════════════════════════
// 数据模型
// ══════════════════════════════════════════════════════════════════════════

/// 特征所在端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
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

/// 视图开关（用户 2026-09-18 定案）：`常规` / `剖视` / `双`。
///
/// * `常规` = 只画外形可见线（默认，与原来完全一致）；
/// * `剖视` = 同一套轮廓 + ANSI31 剖面线（`5剖面线层`）；
/// * `双` = 常规视图与剖视图并排一次出（左常规、右剖视）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ShaftView {
    Normal,
    Section,
    Both,
}

impl Default for ShaftView {
    fn default() -> Self {
        ShaftView::Normal
    }
}

impl ShaftView {
    /// 英文键（JSON/HTTP/GUI 传参、块名）。
    pub fn key(self) -> &'static str {
        match self {
            ShaftView::Normal => "normal",
            ShaftView::Section => "section",
            ShaftView::Both => "both",
        }
    }

    /// 中文名（DSL/GUI/报错提示）。
    pub fn label(self) -> &'static str {
        match self {
            ShaftView::Normal => "常规",
            ShaftView::Section => "剖视",
            ShaftView::Both => "双",
        }
    }

    /// 是否带剖面线（`剖视` / `双`）。
    pub fn has_hatch(self) -> bool {
        matches!(self, ShaftView::Section | ShaftView::Both)
    }

    pub const ALL: [ShaftView; 3] = [ShaftView::Normal, ShaftView::Section, ShaftView::Both];

    /// 解析视图名（中英文都认；`view=` / `视图` 前缀也剥掉）。
    pub fn parse(s: &str) -> Result<Self, String> {
        let t = s.trim().to_ascii_lowercase();
        let t = t
            .trim_start_matches("view")
            .trim_start_matches(['=', ':'])
            .trim_start_matches("视图")
            .trim();
        let hit = match t {
            "normal" | "regular" | "outline" | "常规" | "常规视图" | "外形" | "外观" | "不剖" => {
                Some(ShaftView::Normal)
            }
            "section" | "cut" | "剖" | "剖视" | "剖视图" | "剖面" | "剖开" => {
                Some(ShaftView::Section)
            }
            "both" | "dual" | "双" | "双视图" | "并排" | "常规+剖视" | "两个" => Some(ShaftView::Both),
            _ => None,
        };
        hit.ok_or_else(|| {
            format!(
                "视图名无法识别：`{}`。可用：normal|常规、section|剖视、both|双。",
                s.trim()
            )
        })
    }
}

/// 端面倒角 C（45°：沿轴向 C、半径方向 C）。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Chamfer {
    pub c: f64,
    pub end: End,
}

/// 砂轮越程槽：`b1 = None` = 按该端直径查 GB 表；`Some` = 显式槽宽。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Overtravel {
    pub b1: Option<f64>,
    pub end: End,
}

/// 螺纹段标记（外螺纹侧视图）：不写值 = 简化画法小径 0.85d；写螺距 = 精确小径。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thread {
    /// 螺距 P（> 0）；`None` = 不写值（小径按 0.85d 简化画法）。
    pub pitch: Option<f64>,
}

impl Thread {
    /// 小径 d1：不写值 = 0.85d；写螺距 = d − 1.0825P（GB/T 192 基本尺寸口径）。
    pub fn minor_diameter(&self, d: f64) -> f64 {
        match self.pitch {
            None => 0.85 * d,
            Some(p) => d - 1.0825 * p,
        }
    }

    /// 小径半径 = 小径/2（细实线所在半径）。
    pub fn minor_radius(&self, d: f64) -> f64 {
        self.minor_diameter(d) / 2.0
    }
}

/// 把 `thread` 序列化成「`true`（简化）/ 螺距数字（精确）」两种形式（给 GUI/HTTP）。
fn serialize_thread<S: serde::Serializer>(
    thread: &Option<Thread>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match thread {
        Some(Thread {
            pitch: Some(pitch),
        }) => serializer.serialize_f64(*pitch),
        _ => serializer.serialize_bool(true),
    }
}

/// 齿轮段参数（本期：**外齿轮、直齿**）。派生尺寸统一走 [`GearParams`]（gear.rs 口径）。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Gear {
    /// 模数 m（> 0）。
    pub m: f64,
    /// 齿数 z（整数，2..=1000，与 gear.rs 一致）。
    pub z: u32,
    /// 齿宽 H；`None` = 10m。
    pub h: Option<f64>,
    /// 螺旋角 β（度）；本期只支持 0（斜齿 = 二期）。
    #[serde(rename = "beta")]
    pub beta_deg: f64,
}

impl Gear {
    /// 齿宽（H 省略 = 10m）。
    pub fn width(&self) -> f64 {
        self.h.unwrap_or(10.0 * self.m)
    }

    /// 与 `gear.rs` 同口径的齿轮参数（外齿轮、ha*=1、c*=0.25、Xn=0、直齿）。
    pub fn params(&self) -> GearParams {
        GearParams {
            kind: GearKind::External,
            m: self.m,
            z: self.z,
            beta_deg: self.beta_deg,
            h: self.width(),
            ..GearParams::default()
        }
    }

    /// 分度圆半径 r = d/2（= m·z/2）。
    pub fn pitch_radius(&self) -> f64 {
        self.params().d() / 2.0
    }

    /// 齿顶圆半径 ra = da/2（外齿轮 = d/2 + m）。
    pub fn addendum_radius(&self) -> f64 {
        self.params().da() / 2.0
    }

    /// 齿根圆半径 rf = df/2（外齿轮 = d/2 − 1.25m）。
    pub fn root_radius(&self) -> f64 {
        self.params().df() / 2.0
    }
}

/// 一段轴（从左到右）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Segment {
    /// 起始直径（左端）。齿轮段 = 分度圆直径（由 M·Z 导出）。
    pub s: f64,
    /// 终点直径（右端）。齿轮段 = 分度圆直径。
    pub e: f64,
    /// 段长。齿轮段 = H（省略 10m）。
    pub l: f64,
    /// 倒角（同一端最多一个）。
    pub ch: Vec<Chamfer>,
    /// 越程槽（同一端最多一个）。
    pub ov: Vec<Overtravel>,
    /// 螺纹段标记（`None` = 普通轴段）。
    #[serde(serialize_with = "serialize_thread", skip_serializing_if = "Option::is_none")]
    pub thread: Option<Thread>,
    /// 齿轮段参数（`None` = 普通轴段）。
    pub gear: Option<Gear>,
}

impl Segment {
    /// 某端的**外轮廓半径**（齿轮段 = 齿顶圆半径）。
    pub fn outer_radius(&self, end: End) -> f64 {
        if let Some(gear) = &self.gear {
            return gear.addendum_radius();
        }
        match end {
            End::L => self.s / 2.0,
            End::R => self.e / 2.0,
        }
    }
}

/// 解析结果：段序列 + 放置参数（`at` / `rot` 由命令层应用）+ 视图开关。
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct Program {
    pub segments: Vec<Segment>,
    pub at: Option<[f64; 2]>,
    pub rot: Option<f64>,
    /// 视图（默认 `常规`）；`serde` 里序列化成 `normal`/`section`/`both`。
    #[serde(default)]
    pub view: ShaftView,
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
    let mut view_seen: Option<ShaftView> = None;
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
                // 单段行也带上「第 N 段」，便于用户按段定位（与几何错误口径一致）。
                format!("第 {line_no} 行（第 {} 段）", program.segments.len() + 1)
            };
            // `VIEW 剖视` / `视图 剖视` 是整体视图开关：独立一行或段内关键字都认。
            let tokens: Vec<&str> = chunk.split_whitespace().collect();
            let tokens = extract_view_directives(&tokens, &label, &mut view_seen)?;
            if tokens.is_empty() {
                continue;
            }
            if tokens[0].eq_ignore_ascii_case("at") || tokens[0] == "@" {
                parse_placement(&tokens, &label, &mut program)?;
                continue;
            }
            let seg = parse_segment(&tokens.join(" "), &label, &mut program)?;
            program.segments.push(seg);
        }
    }
    if program.segments.is_empty() {
        return Err("没有解析到任何轴段（至少给一段 `S… L…`）".into());
    }
    program.view = view_seen.unwrap_or_default();
    Ok(program)
}

/// 从一段的 token 里提取 `VIEW …` / `视图 …` 指令（独立一行或段内均认），
/// 返回去掉指令后的 token。同值重复忽略；不同值报冲突。
fn extract_view_directives<'a>(
    tokens: &[&'a str],
    label: &str,
    view: &mut Option<ShaftView>,
) -> Result<Vec<&'a str>, String> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        let upper = token.to_ascii_uppercase();
        let mut name: Option<&str> = None;
        if upper == "VIEW" || token == "视图" {
            index += 1;
            name = Some(tokens.get(index).copied().ok_or_else(|| {
                format!("{label}：关键字 VIEW 缺少视图名（常规/剖视/双）")
            })?);
        } else if let Some(rest) = upper.strip_prefix("VIEW") {
            name = rest.strip_prefix(['=', ':']);
        } else if let Some(rest) = token.strip_prefix("视图") {
            name = rest.strip_prefix(['=', ':']);
        }
        if let Some(name) = name {
            let parsed = ShaftView::parse(name).map_err(|e| format!("{label}：{e}"))?;
            match *view {
                Some(prev) if prev != parsed => {
                    return Err(format!(
                        "{label}：视图 VIEW 重复且冲突（已给「{}」，又给「{}」）",
                        prev.label(),
                        parsed.label()
                    ));
                }
                _ => *view = Some(parsed),
            }
        } else {
            out.push(token);
        }
        index += 1;
    }
    Ok(out)
}

fn unknown_keyword(token: &str, label: &str) -> String {
    format!("{label}：关键字「{token}」二期未实现（本期只支持 S/E/L/CH/OV/M/GEAR/VIEW）")
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

/// `ES` 旧关键字已取消：退刀槽 = 一小段小直径轴段，报错并给写法示例。
fn es_cancelled(label: &str) -> String {
    format!("{label}：`ES` 已取消 —— 退刀槽用一小段小直径轴段表示，例如 `S24 E24 L5`")
}

/// `M` / `M1.5`：不写值 = 简化画法；写值 = 螺距 P（> 0）。
fn parse_thread(rest: &str, label: &str) -> Result<Thread, String> {
    let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
    if rest.is_empty() {
        return Ok(Thread { pitch: None });
    }
    let pitch = parse_number(rest, label, "M")?;
    if pitch <= 0.0 {
        return Err(format!(
            "{label}：关键字 M 的螺距 P={} 非法（必须 > 0；不写值 = 小径 0.85d 简化画法）",
            trim(pitch)
        ));
    }
    Ok(Thread {
        pitch: Some(pitch),
    })
}

fn parse_segment(chunk: &str, label: &str, program: &mut Program) -> Result<Segment, String> {
    let tokens: Vec<&str> = chunk.split_whitespace().collect();
    let (mut s, mut e, mut l) = (None, None, None);
    let mut ch: Vec<Chamfer> = Vec::new();
    let mut ov: Vec<Overtravel> = Vec::new();
    let mut thread: Option<Thread> = None;
    // GEAR 子关键字（M/Z/H/BETA）先收齐，段内顺序无关；没有 GEAR 时 M = 螺纹。
    // 先扫一遍段里有没有 GEAR：有 GEAR 时 M 一律按模数收（保持段内顺序无关）。
    let has_gear = tokens.iter().any(|t| t.eq_ignore_ascii_case("GEAR"));
    let mut gear_on = false;
    let (mut gear_m, mut gear_z, mut gear_h, mut gear_beta) = (None, None, None, None);
    let mut loose_gear_token: Option<String> = None;
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
        } else if upper.starts_with("ES") {
            // ES 已取消：给指路提示（不静默丢特征，也不当成 E+S 乱解）。
            return Err(es_cancelled(label));
        } else if upper == "GEAR" {
            if gear_on {
                return Err(format!("{label}：关键字 GEAR 重复"));
            }
            gear_on = true;
        } else if upper.starts_with("GEAR") {
            return Err(unknown_keyword(token, label));
        } else if upper.starts_with('M') {
            if has_gear {
                let value = parse_gear_number(token, 1, "M", label)?;
                if gear_m.is_some() {
                    return Err(format!(
                        "{label}：关键字 M 重复（GEAR 段里的 M 是模数；螺纹 M 不能与 GEAR 同段）"
                    ));
                }
                gear_m = Some(value);
            } else {
                if thread.is_some() {
                    return Err(format!("{label}：关键字 M 重复"));
                }
                thread = Some(parse_thread(&token[1..], label)?);
            }
        } else if upper.starts_with('Z') {
            let rest = parse_gear_value(token, 1, "Z", label)?;
            let value: u32 = rest.parse().map_err(|_| {
                format!("{label}：关键字 Z 的值「{rest}」不是正整数（齿数 z 必须是整数）")
            })?;
            if gear_z.is_some() {
                return Err(format!("{label}：关键字 Z 重复"));
            }
            gear_z = Some(value);
            remember_loose_gear_token(&mut loose_gear_token, token);
        } else if upper.starts_with('H') {
            let value = parse_gear_number(token, 1, "H", label)?;
            if gear_h.is_some() {
                return Err(format!("{label}：关键字 H 重复"));
            }
            gear_h = Some(value);
            remember_loose_gear_token(&mut loose_gear_token, token);
        } else if upper.starts_with("BETA") {
            let value = parse_gear_number(token, 4, "BETA", label)?;
            if gear_beta.is_some() {
                return Err(format!("{label}：关键字 BETA 重复"));
            }
            gear_beta = Some(value);
            remember_loose_gear_token(&mut loose_gear_token, token);
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
    if !gear_on {
        // 没有 GEAR 的 Z/H/BETA 仍按二期关键字报（不静默忽略）。
        if let Some(token) = loose_gear_token {
            return Err(unknown_keyword(&token, label));
        }
        if thread.is_some() && !ov.is_empty() {
            return Err(format!("{label}：螺纹段 M 不能与越程槽 OV 同段"));
        }
        let s = s.ok_or_else(|| format!("{label}：缺少 S（起始直径）"))?;
        // E 省略 = 圆柱段；L 必给。
        let l = l.ok_or_else(|| format!("{label}：缺少 L（段长）"))?;
        return Ok(Segment {
            s,
            e: e.unwrap_or(s),
            l,
            ch,
            ov,
            thread,
            gear: None,
        });
    }
    // ── 齿轮段：直径由 M·Z 导出、长度用 H；CH/OV/M 同段冲突 ──
    if s.is_some() || e.is_some() {
        return Err(format!("{label}：齿轮段不给 S/E（直径由 M·Z 导出）"));
    }
    if l.is_some() {
        return Err(format!(
            "{label}：齿轮段长度用 H（省略 = 10m），不要再给 L"
        ));
    }
    if !ch.is_empty() {
        return Err(format!(
            "{label}：齿轮段不能与倒角 CH 同段（齿形用 OCSMGEAR 单独出）"
        ));
    }
    if !ov.is_empty() {
        return Err(format!("{label}：齿轮段不能与越程槽 OV 同段"));
    }
    if thread.is_some() {
        return Err(format!("{label}：齿轮段不能与螺纹段 M 同段"));
    }
    let m = gear_m.ok_or_else(|| format!("{label}：关键字 GEAR 缺少 M（模数）"))?;
    let z = gear_z.ok_or_else(|| format!("{label}：关键字 GEAR 缺少 Z（齿数）"))?;
    let beta_deg = gear_beta.unwrap_or(0.0);
    if beta_deg.abs() > 1e-9 {
        return Err(format!(
            "{label}：关键字 BETA 的斜齿（β={}°）二期未实现，本期只做直齿",
            trim(beta_deg)
        ));
    }
    let gear = Gear {
        m,
        z,
        h: gear_h,
        beta_deg,
    };
    let params = gear.params();
    params
        .validate()
        .map_err(|e| format!("{label}：齿轮段：{e}"))?;
    let d = params.d();
    Ok(Segment {
        s: d,
        e: d,
        l: gear.width(),
        ch,
        ov,
        thread,
        gear: Some(gear),
    })
}

/// GEAR 子关键字（M/Z/H/BETA）：`M=5` / `M5` 都收；缺值报错。
fn parse_gear_value<'a>(
    token: &'a str,
    prefix_len: usize,
    what: &str,
    label: &str,
) -> Result<&'a str, String> {
    let rest = &token[prefix_len..];
    let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
    if rest.is_empty() {
        return Err(format!("{label}：关键字 {what} 缺少数值"));
    }
    Ok(rest)
}

fn parse_gear_number(token: &str, prefix_len: usize, what: &str, label: &str) -> Result<f64, String> {
    let rest = parse_gear_value(token, prefix_len, what, label)?;
    parse_number(rest, label, what)
}

/// 没有 GEAR 时记住第一个 Z/H/BETA 原文（到段尾统一报二期/缺 GEAR）。
fn remember_loose_gear_token(slot: &mut Option<String>, token: &str) {
    if slot.is_none() {
        *slot = Some(token.to_string());
    }
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
        // 数组必须先试：结构体字段都有默认值时，serde 会把空数组 `[]` 当成
        // 「全默认的结构体」反序列化（`One` 会误吞空数组）——写成数组就一定是数组。
        Many(Vec<T>),
        One(T),
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
    /// 视图：`normal|section|both` 或中文名；缺省 = 常规。
    #[serde(default)]
    view: Option<String>,
}

#[derive(serde::Deserialize)]
struct JsonSegment {
    #[serde(default)]
    s: Option<f64>,
    #[serde(default)]
    e: Option<f64>,
    #[serde(default)]
    l: Option<f64>,
    #[serde(default, deserialize_with = "one_or_many")]
    ch: Vec<JsonChamfer>,
    #[serde(default, deserialize_with = "one_or_many")]
    ov: Vec<JsonOvertravel>,
    /// 旧字段：`ES` 退刀槽已取消 → 出现就报错指路（不静默丢特征）。
    #[serde(default)]
    es: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "json_thread")]
    thread: Option<Thread>,
    #[serde(default)]
    gear: Option<JsonGear>,
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

#[derive(serde::Deserialize)]
struct JsonGear {
    m: f64,
    z: u32,
    #[serde(default)]
    h: Option<f64>,
    #[serde(default)]
    beta: Option<f64>,
}

/// `thread` 允许：`true` / `1.5`（螺距）/ `{"p":1.5}` / `{"pitch":1.5}` / `"M1.5"`。
fn json_thread<'de, D>(de: D) -> Result<Option<Thread>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Raw {
        On(bool),
        Pitch(f64),
        Text(String),
        Obj {
            #[serde(default)]
            p: Option<f64>,
            #[serde(default)]
            pitch: Option<f64>,
        },
    }
    let raw = match Option::<Raw>::deserialize(de)? {
        None => return Ok(None),
        Some(raw) => raw,
    };
    match raw {
        Raw::On(false) => Ok(None),
        Raw::On(true) => Ok(Some(Thread { pitch: None })),
        Raw::Pitch(pitch) => Ok(Some(Thread {
            pitch: Some(pitch),
        })),
        Raw::Obj { p, pitch } => Ok(Some(Thread {
            pitch: p.or(pitch),
        })),
        Raw::Text(text) => {
            let text = text.trim();
            let text = text
                .strip_prefix(['M', 'm'])
                .map(str::trim)
                .unwrap_or(text);
            if text.is_empty() {
                return Ok(Some(Thread { pitch: None }));
            }
            let pitch = text.parse::<f64>().map_err(|_| {
                serde::de::Error::custom(format!("thread 字符串「{text}」不是螺距数字"))
            })?;
            Ok(Some(Thread {
                pitch: Some(pitch),
            }))
        }
    }
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
        if item.es.is_some() {
            return Err(format!(
                "第 {number} 段：`ES` 已取消 —— 退刀槽用一小段小直径轴段表示，例如 `S24 E24 L5`"
            ));
        }
        let thread = item.thread;
        // 齿轮段：直径由 m·z 导出、长度用 h；与 CH/OV/M 同段冲突。
        // （序列化回传的 s/e/l 允许出现，但要等于派生值，不允许相互矛盾。）
        if let Some(g) = &item.gear {
            if !ch.is_empty() {
                return Err(format!("第 {number} 段：齿轮段不能与倒角 ch 同段"));
            }
            if !ov.is_empty() {
                return Err(format!("第 {number} 段：齿轮段不能与越程槽 ov 同段"));
            }
            if thread.is_some() {
                return Err(format!("第 {number} 段：齿轮段不能与螺纹段 m 同段"));
            }
            let gear = Gear {
                m: g.m,
                z: g.z,
                h: g.h,
                beta_deg: g.beta.unwrap_or(0.0),
            };
            if gear.beta_deg.abs() > 1e-9 {
                return Err(format!(
                    "第 {number} 段：齿轮段斜齿（beta={}）二期未实现，本期只做直齿",
                    trim(gear.beta_deg)
                ));
            }
            let params = gear.params();
            params
                .validate()
                .map_err(|e| format!("第 {number} 段：齿轮段：{e}"))?;
            let d = params.d();
            if let Some(s) = item.s {
                if (s - d).abs() > 1e-9 {
                    return Err(format!(
                        "第 {number} 段：齿轮段的 s={} 应等于分度圆 d={}（由 m·z 导出）",
                        trim(s),
                        trim(d)
                    ));
                }
            }
            if let Some(e) = item.e {
                if (e - d).abs() > 1e-9 {
                    return Err(format!(
                        "第 {number} 段：齿轮段的 e={} 应等于分度圆 d={}（由 m·z 导出）",
                        trim(e),
                        trim(d)
                    ));
                }
            }
            if let Some(l) = item.l {
                if (l - gear.width()).abs() > 1e-9 {
                    return Err(format!(
                        "第 {number} 段：齿轮段的 l={} 应等于齿宽 h={}",
                        trim(l),
                        trim(gear.width())
                    ));
                }
            }
            segments.push(Segment {
                s: d,
                e: d,
                l: gear.width(),
                ch,
                ov,
                thread,
                gear: Some(gear),
            });
            continue;
        }
        let s = item
            .s
            .ok_or_else(|| format!("第 {number} 段：缺少 s（起始直径）"))?;
        let l = item
            .l
            .ok_or_else(|| format!("第 {number} 段：缺少 l（段长）"))?;
        segments.push(Segment {
            s,
            e: item.e.unwrap_or(s),
            l,
            ch,
            ov,
            thread,
            gear: None,
        });
    }
    let view = match &raw.view {
        None => ShaftView::default(),
        Some(text) => ShaftView::parse(text).map_err(|e| format!("JSON：{e}"))?,
    };
    if raw.rot.is_some_and(|v| !v.is_finite()) {
        return Err("JSON：rot 不是有限数".into());
    }
    Ok(Program {
        segments,
        at: raw.at,
        rot: raw.rot,
        view,
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
        if let Some(thread) = &seg.thread {
            if let Some(pitch) = thread.pitch {
                if !pitch.is_finite() || pitch <= 0.0 {
                    return Err(format!(
                        "第 {number} 段：螺纹螺距 P={} 必须 > 0",
                        trim(pitch)
                    ));
                }
                let minor = thread.minor_diameter(seg.s);
                if minor <= 0.0 {
                    return Err(format!(
                        "第 {number} 段：螺距 P={} 太大（小径 d−1.0825P={} ≤ 0）",
                        trim(pitch),
                        trim(minor)
                    ));
                }
            }
            if (seg.s - seg.e).abs() > 1e-9 {
                return Err(format!(
                    "第 {number} 段：螺纹段必须是圆柱（S==E，当前 S={}、E={}）",
                    trim(seg.s),
                    trim(seg.e)
                ));
            }
            if !seg.ov.is_empty() {
                return Err(format!("第 {number} 段：螺纹段 M 不能与越程槽 OV 同段"));
            }
            if seg.gear.is_some() {
                return Err(format!(
                    "第 {number} 段：螺纹段 M 不能与齿轮段 GEAR 同段"
                ));
            }
        }
        if let Some(gear) = &seg.gear {
            gear.params()
                .validate()
                .map_err(|e| format!("第 {number} 段：齿轮段：{e}"))?;
            if !seg.ch.is_empty() {
                return Err(format!("第 {number} 段：齿轮段不能与倒角 CH 同段"));
            }
            if !seg.ov.is_empty() {
                return Err(format!("第 {number} 段：齿轮段不能与越程槽 OV 同段"));
            }
            if seg.thread.is_some() {
                return Err(format!("第 {number} 段：齿轮段不能与螺纹段 M 同段"));
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

/// 某段某端**端面处实际轮廓半径**：越程槽圆角切点 / 齿顶圆 / 普通半径。
/// 只用于齿轮相邻直径检查；落图的端面几何在 `build` 里另算。
fn face_radius(seg: &Segment, end: End) -> f64 {
    if let Some(ov) = seg.ov.iter().find(|o| o.end == end) {
        let d = match end {
            End::L => seg.s,
            End::R => seg.e,
        };
        if let Ok((_, row)) = detail::groove_entities(d, ov.b1) {
            return detail::fillet_tangent_radius(d, row);
        }
    }
    seg.outer_radius(end)
}

/// 沿 x 平移（`双` 视图把第二张整体右移；Hatch 的边界段同步平移，
/// 图案 offset 是世界坐标下的周期量，不用改）。
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
        EntityType::Hatch(mut h) => {
            for path in &mut h.paths {
                for edge in &mut path.edges {
                    match edge {
                        BoundaryEdge::Line(e) => {
                            e.start.x += dx;
                            e.end.x += dx;
                        }
                        BoundaryEdge::CircularArc(e) => e.center.x += dx,
                        BoundaryEdge::EllipticArc(e) => {
                            e.center.x += dx;
                            e.major_axis_endpoint.x += dx;
                        }
                        BoundaryEdge::Polyline(e) => {
                            for v in &mut e.vertices {
                                v.x += dx;
                            }
                        }
                        BoundaryEdge::Spline(e) => {
                            for q in &mut e.control_points {
                                q.x += dx;
                            }
                            for q in &mut e.fit_points {
                                q.x += dx;
                            }
                        }
                    }
                }
            }
            EntityType::Hatch(h)
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

/// 一次构建出来的原始几何：常规视图轮廓 + 剖面线环（上半侧，左→右）。
struct Geometry {
    entities: Vec<EntityType>,
    /// 上半外轮廓边界段（含直线/圆弧，已按 x 排好序）；剖视时镜像闭合成一个简单环。
    ring: Vec<HatchEdge>,
    total_length: f64,
    max_diameter: f64,
    segment_count: usize,
}

/// 解析 → 校验 → 生成（`frame_scale` 用于轴线 `6n` 伸出量；无图框传 1.0）。
/// 返回的图元已按 `program.view` 组合（常规 / 剖视 / 双）。
pub fn build(program: &Program, frame_scale: f64) -> Result<Shaft, String> {
    let Geometry {
        entities,
        ring,
        total_length: total,
        max_diameter,
        segment_count,
    } = build_geometry(program, frame_scale)?;
    let hatch = || {
        if ring.is_empty() {
            Vec::new()
        } else {
            // ANSI31 / 比例 1.0，一个简单环（走 partgen_kit 的既有 HATCH 通路）。
            vec![crate::partgen_kit::hatch_ansi31_edges(&ring, 0.0, 1.0)]
        }
    };
    let mut entities = entities;
    match program.view {
        ShaftView::Normal => {}
        ShaftView::Section => entities.extend(hatch()),
        ShaftView::Both => {
            // 并排：左常规、右剖视；间距 = max(总长×15%, 40×图框比例)。
            let dx = total + both_view_gap(total, frame_scale);
            let mut second: Vec<EntityType> =
                entities.iter().cloned().map(|e| translate_x(e, dx)).collect();
            second.extend(hatch().into_iter().map(|e| translate_x(e, dx)));
            entities.extend(second);
        }
    }
    Ok(Shaft {
        entities,
        total_length: total,
        max_diameter,
        segment_count,
    })
}

/// `双` 视图两视图之间的间距口径：`max(总长 × 15%, 40 × 图框比例)`。
fn both_view_gap(total_length: f64, frame_scale: f64) -> f64 {
    (0.15 * total_length).max(40.0 * frame_scale)
}

/// 解析 → 校验 → 生成常规视图轮廓，并顺手记录剖面线环用的上半外轮廓段。
fn build_geometry(program: &Program, frame_scale: f64) -> Result<Geometry, String> {
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
        .map(|s| s.outer_radius(End::L).max(s.outer_radius(End::R)) * 2.0)
        .fold(0.0_f64, f64::max);

    // 倒角 / 越程槽落在**本段自己**身上的量（决定本段轮廓线被吃掉多少）。
    let mut own_ch = vec![[None::<f64>; 2]; count]; // [左, 右]
    let mut own_ov = vec![[None::<f64>; 2]; count];
    let mut entities: Vec<EntityType> = Vec::new();
    // 剖面线环用的上半外轮廓段（含倒角/台阶/越程槽圆角），最后按 x 排序闭合。
    let mut profile: Vec<HatchEdge> = Vec::new();

    // ── 齿轮段相邻直径检查（用户 2026-09-18 口径）：相邻半径 ≤ ra 放行；
    //    > ra 会盖住齿顶线 → 报「第 N 段」。不再受齿根圆 rf 限制。 ──
    for (index, seg) in segs.iter().enumerate() {
        let Some(gear) = &seg.gear else { continue };
        let ra = gear.addendum_radius();
        let neighbors = [
            (index.checked_sub(1), End::R),
            (
                if index + 1 < count {
                    Some(index + 1)
                } else {
                    None
                },
                End::L,
            ),
        ];
        for (j, end) in neighbors {
            let Some(j) = j else { continue };
            let r_n = face_radius(&segs[j], end);
            if r_n > ra + 1e-9 {
                return Err(format!(
                    "第 {} 段：齿轮段相邻第 {} 段 Ø{} 大于齿顶圆 Ø{}，会盖住齿顶线（相邻段半径必须 ≤ 齿顶圆半径）",
                    index + 1,
                    j + 1,
                    trim(r_n * 2.0),
                    trim(ra * 2.0)
                ));
            }
        }
    }

    // ── 内部端面（第 i 段右端 ↔ 第 i+1 段左端） ──
    for j in 0..count.saturating_sub(1) {
        let i = j;
        let k = j + 1;
        let x_face = x0s[k];
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
        // 该端面处两侧的轮廓半径。
        let ra = segs[i].outer_radius(End::R);
        let rb = segs[k].outer_radius(End::L);

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
            // 倒角贴**凸角**（大的一侧）；落在齿轮段那侧则冲突。
            let (land, _land_end) = if rb > ra { (k, End::L) } else { (i, End::R) };
            if segs[land].gear.is_some() {
                return Err(format!(
                    "第 {} 段：{}端倒角会落在第 {} 段齿轮段上（齿轮段不能倒角）",
                    requester + 1,
                    requester_end,
                    land + 1
                ));
            }
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
            // 剖面线环：斜坡 → 槽底 → 圆角（上半侧，左→右）
            let rg = segs[i].e / 2.0;
            let yb = rg - row.h;
            let slope_x1 = x_face - (row.b1 - row.h);
            let groove_x1 = x_face - row.r;
            profile.push(lr_line([x_face - row.b1, rg], [slope_x1, yb]));
            profile.push(lr_line([slope_x1, yb], [groove_x1, yb]));
            profile.push(HatchEdge::Arc {
                c: [groove_x1, fillet],
                r: row.r,
                start_deg: -90.0,
                end_deg: 0.0,
                ccw: true,
            });
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
            // 剖面线环：圆角 → 槽底 → 斜坡（上半侧，左→右）
            let rg = segs[k].s / 2.0;
            let yb = rg - row.h;
            let slope_x1 = x_face + (row.b1 - row.h);
            profile.push(HatchEdge::Arc {
                c: [x_face + row.r, fillet],
                r: row.r,
                start_deg: 180.0,
                end_deg: 270.0,
                ccw: true,
            });
            profile.push(lr_line([x_face + row.r, yb], [slope_x1, yb]));
            profile.push(lr_line([slope_x1, yb], [x_face + row.b1, rg]));
        }

        // 上半轮廓在端面两侧的实际半径（决定剖面线环竖直段的走向）：
        // 倒角/越程槽会把这侧表面从原始半径上切掉一块。
        let mut left_eff = ra;
        let mut right_eff = rb;
        if let Some((_, face_point)) = chamfer_lines {
            if rb > ra {
                right_eff = face_point[1];
            } else {
                left_eff = face_point[1];
            }
        }
        if ov_r.is_some() {
            left_eff = bottom;
        }
        if ov_l.is_some() {
            right_eff = bottom;
        }

        if top - bottom > 1e-9 {
            entities.push(line([x_face, bottom], [x_face, top], LAYER_MAIN));
            entities.push(line([x_face, -bottom], [x_face, -top], LAYER_MAIN));
            // 上半轮廓竖直段：左段实际表面 → 右段实际表面（方向就是环的走向）
            profile.push(HatchEdge::Line {
                a: [x_face, left_eff],
                b: [x_face, right_eff],
            });
        }
        if let Some((contour, face_point)) = chamfer_lines {
            entities.push(line(face_point, contour, LAYER_MAIN));
            entities.push(line(
                [face_point[0], -face_point[1]],
                [contour[0], -contour[1]],
                LAYER_MAIN,
            ));
            profile.push(lr_line(face_point, contour));
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
        profile.push(lr_line(face_point, contour));
        face_point[1]
    } else {
        segs[0].outer_radius(End::L)
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
        profile.push(lr_line(contour, face_point));
        face_point[1]
    } else {
        segs[last].outer_radius(End::R)
    };
    entities.push(line([x_end, -right_face], [x_end, right_face], LAYER_MAIN));

    // ── 每段上下轮廓线（两端被倒角/越程槽吃掉多少已定）；
    //    齿轮段 = 齿顶线（轮廓）+ 分度线（不画齿根线）；螺纹段另加小径细实线 ──
    for (index, seg) in segs.iter().enumerate() {
        if let Some(gear) = &seg.gear {
            let (x0, x1) = (x0s[index], x0s[index] + seg.l);
            let (r, ra) = (gear.pitch_radius(), gear.addendum_radius());
            // 齿顶线 = 该段轮廓（粗实线）
            entities.push(line([x0, ra], [x1, ra], LAYER_MAIN));
            entities.push(line([x0, -ra], [x1, -ra], LAYER_MAIN));
            profile.push(lr_line([x0, ra], [x1, ra]));
            // 分度线（点划线，3中心线层）
            entities.push(line([x0, r], [x1, r], LAYER_CENTER));
            entities.push(line([x0, -r], [x1, -r], LAYER_CENTER));
            continue;
        }
        let start_shift = own_ch[index][0]
            .unwrap_or(0.0)
            .max(own_ov[index][0].unwrap_or(0.0));
        let end_shift = own_ch[index][1]
            .unwrap_or(0.0)
            .max(own_ov[index][1].unwrap_or(0.0));
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
            profile.push(lr_line([xs, ys], [xe, ye]));
        }
        // ── 螺纹段：小径细实线上下各一条（2细线层），止于段末终止线；
        //    同段端面倒角比小径深时改止于倒角斜线交点（不挑出材料外）。 ──
        if let Some(thread) = &seg.thread {
            let (x0, x1) = (x0s[index], x0s[index] + seg.l);
            let minor = thread.minor_radius(seg.s);
            let cut = seg.s / 2.0 - minor;
            let inset = |chamfer: Option<f64>| {
                chamfer
                    .filter(|c| *c > cut + 1e-12)
                    .map(|c| c - cut)
                    .unwrap_or(0.0)
            };
            let xt0 = x0 + inset(own_ch[index][0]);
            let xt1 = x1 - inset(own_ch[index][1]);
            if xt1 - xt0 > 1e-9 {
                entities.push(line([xt0, minor], [xt1, minor], LAYER_THIN));
                entities.push(line([xt0, -minor], [xt1, -minor], LAYER_THIN));
            }
        }
    }

    // ── 轴线（3中心线层）：长度 = 总长 + 图框比例 × 6，两端各伸出 3n ──
    let half_overhang = 3.0 * frame_scale;
    entities.push(line(
        [-half_overhang, 0.0],
        [total + half_overhang, 0.0],
        LAYER_CENTER,
    ));

    // ── 剖面线环：上半轮廓（左→右）按 x 拼接后镜像闭合 ──
    let ring = close_hatch_ring(profile, x_end, right_face, left_face);

    Ok(Geometry {
        entities,
        ring,
        total_length: total,
        max_diameter,
        segment_count: count,
    })
}

/// 上半轮廓段按 x 排序后，镜像闭合成剖面线的**一个简单环**：
/// 上半轮廓（左→右）→ 右端面 → 下半轮廓（右→左，镜像）→ 左端面。
fn close_hatch_ring(
    mut profile: Vec<HatchEdge>,
    x_end: f64,
    right_face: f64,
    left_face: f64,
) -> Vec<HatchEdge> {
    profile.retain(|e| !edge_is_degenerate(e));
    profile.sort_by(|a, b| {
        let (a0, a1) = edge_span(a);
        let (b0, b1) = edge_span(b);
        a0.partial_cmp(&b0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a1.partial_cmp(&b1).unwrap_or(std::cmp::Ordering::Equal))
    });
    let mut ring = Vec::with_capacity(profile.len() * 2 + 2);
    ring.extend(profile.iter().copied());
    ring.push(HatchEdge::Line {
        a: [x_end, right_face],
        b: [x_end, -right_face],
    });
    for edge in profile.iter().rev() {
        ring.push(mirror_reverse_edge(edge));
    }
    ring.push(HatchEdge::Line {
        a: [0.0, -left_face],
        b: [0.0, left_face],
    });
    ring
}

/// 直线段按 x 方向排成左→右（竖直段不要用它，方向要按环的走向手写）。
fn lr_line(a: [f64; 2], b: [f64; 2]) -> HatchEdge {
    if a[0] <= b[0] {
        HatchEdge::Line { a, b }
    } else {
        HatchEdge::Line { a: b, b: a }
    }
}

/// 边界段两端的 x 范围（弧用起讫点；本文件的弧都是 x 单调的四分之一弧）。
fn edge_span(edge: &HatchEdge) -> (f64, f64) {
    let (a, b) = match *edge {
        HatchEdge::Line { a, b } => (a, b),
        HatchEdge::Arc {
            c,
            r,
            start_deg,
            end_deg,
            ..
        } => (
            [
                c[0] + r * start_deg.to_radians().cos(),
                c[1] + r * start_deg.to_radians().sin(),
            ],
            [
                c[0] + r * end_deg.to_radians().cos(),
                c[1] + r * end_deg.to_radians().sin(),
            ],
        ),
    };
    if a[0] <= b[0] {
        (a[0], b[0])
    } else {
        (b[0], a[0])
    }
}

/// 零长边界段（与 `partgen_b4::dedup_ring` 同口径：重复点会让宿主判边界无效）。
fn edge_is_degenerate(edge: &HatchEdge) -> bool {
    match *edge {
        HatchEdge::Line { a, b } => (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9,
        HatchEdge::Arc {
            r,
            start_deg,
            end_deg,
            ..
        } => r < 1e-9 || (end_deg - start_deg).abs() < 1e-9,
    }
}

/// 关于 x 轴镜像 + 反向（环从上半侧折回下半侧时用）。
///
/// 圆弧推导：镜像是 `θ' = −θ`；再反向一次 ⇒ `start' = −end`、`end' = −start`、
/// `ccw' = ccw`（两次方向翻转相互抵消）。
fn mirror_reverse_edge(edge: &HatchEdge) -> HatchEdge {
    match *edge {
        HatchEdge::Line { a, b } => HatchEdge::Line {
            a: [b[0], -b[1]],
            b: [a[0], -a[1]],
        },
        HatchEdge::Arc {
            c,
            r,
            start_deg,
            end_deg,
            ccw,
        } => HatchEdge::Arc {
            c: [c[0], -c[1]],
            r,
            start_deg: -end_deg,
            end_deg: -start_deg,
            ccw,
        },
    }
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
            EntityType::Hatch(mut h) => {
                // 剖面线边界跟着转/移；图案方向（ANSI31 45°）是材质约定，不随视图旋转。
                for path in &mut h.paths {
                    for edge in &mut path.edges {
                        match edge {
                            BoundaryEdge::Line(e) => {
                                let a = map([e.start.x, e.start.y]);
                                let b = map([e.end.x, e.end.y]);
                                e.start = Vector2::new(a[0], a[1]);
                                e.end = Vector2::new(b[0], b[1]);
                            }
                            BoundaryEdge::CircularArc(e) => {
                                let c = map([e.center.x, e.center.y]);
                                e.center = Vector2::new(c[0], c[1]);
                                e.start_angle += rad;
                                e.end_angle += rad;
                            }
                            BoundaryEdge::EllipticArc(e) => {
                                let c = map([e.center.x, e.center.y]);
                                e.center = Vector2::new(c[0], c[1]);
                                let (vx, vy) = (e.major_axis_endpoint.x, e.major_axis_endpoint.y);
                                e.major_axis_endpoint =
                                    Vector2::new(vx * cos - vy * sin, vx * sin + vy * cos);
                                e.start_angle += rad;
                                e.end_angle += rad;
                            }
                            BoundaryEdge::Polyline(e) => {
                                for v in &mut e.vertices {
                                    let p = map([v.x, v.y]);
                                    v.x = p[0];
                                    v.y = p[1];
                                }
                            }
                            BoundaryEdge::Spline(e) => {
                                for q in &mut e.control_points {
                                    let p = map([q.x, q.y]);
                                    q.x = p[0];
                                    q.y = p[1];
                                }
                                for q in &mut e.fit_points {
                                    let p = map([q.x, q.y]);
                                    q.x = p[0];
                                    q.y = p[1];
                                }
                            }
                        }
                    }
                }
                EntityType::Hatch(h)
            }
            other => other,
        })
        .collect()
}

// ══════════════════════════════════════════════════════════════════════════
// 对外：初始化拦截 / 块名 / SVG 预览
// ══════════════════════════════════════════════════════════════════════════

/// OCSM 初始化检查（判据与 `gear::ocsm_ready` 相同，措辞改成轴）：
/// 图形层在、中心线层挂着点划线；不满足就在插入前拦下，避免中心线变实线白线。
pub fn ocsm_ready(doc: &ocs_plugin_api::host::acadrust::CadDocument) -> Result<(), String> {
    let find = |name: &str| {
        doc.layers
            .iter()
            .find(|ly| ly.name.eq_ignore_ascii_case(name))
    };
    const NEED: [&str; 4] = [LAYER_MAIN, LAYER_THIN, LAYER_CENTER, LAYER_HATCH];
    let missing: Vec<&str> = NEED
        .iter()
        .copied()
        .filter(|n| find(n).is_none())
        .collect();
    let center_ok = find(LAYER_CENTER)
        .map(|ly| ly.line_type.eq_ignore_ascii_case("CENTER2"))
        .unwrap_or(false);
    if missing.is_empty() && center_ok {
        return Ok(());
    }
    let why = if !missing.is_empty() {
        format!("（缺图层：{}）", missing.join("、"))
    } else {
        format!("（{LAYER_CENTER} 没挂 CENTER2 点划线）")
    };
    Err(format!(
        "这张图还没跑过 OCSM 初始化{why} —— 先执行 OCSM（建 10 个图层 + 线型 + 文字/标注样式），\
         再生成轴；不然中心线会是实线白线。"
    ))
}

/// 轴块名：只由几何（段序列 + 视图）决定 —— 同模型复用一个块，不同模型不会误用别人的块。
/// 用 FNV-1a 64 位而不是 `DefaultHasher`：跨进程/跨版本稳定，写进 DWG 的块名可复现。
pub fn block_name(program: &Program) -> String {
    let canonical = format!(
        "{}|{}",
        program.view.key(),
        serde_json::to_string(&program.segments).unwrap_or_default()
    );
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in canonical.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("OCSM_SHAFT_{hash:016X}")
}

/// GUI 预览：轴的侧视图 SVG（图元渲染复用 `gear.rs` 的口径）。
///
/// `gear::svg_of` 的 Hatch 预览只按包围盒裁 45° 线（它那里全是轴对齐矩形环），
/// 轴的剖面环是带台阶/倒角/槽的多边形 → 先在 `preview_entities` 里把 HATCH
/// 换成**裁剪到真实边界**的 45° 线，再交给同一套渲染。导出图纸仍是真 HATCH。
pub fn preview_svg(program: &Program) -> Result<String, String> {
    let shaft = build(program, 1.0)?;
    Ok(crate::gear::svg_of(&preview_entities(&shaft.entities), 460.0))
}

/// ANSI31 基准线间距（与 `partgen_kit::hatch_ansi31_edges` 的 offset 同源）。
const ANSI31_OFFSET: f64 = 2.245_064_030_267_288;

/// 预览专用：把每个 HATCH 换成其边界内的 45° 图案线（`5剖面线层`）。
fn preview_entities(entities: &[EntityType]) -> Vec<EntityType> {
    let mut out = Vec::with_capacity(entities.len());
    for entity in entities {
        match entity {
            EntityType::Hatch(h) => out.extend(hatch_preview_lines(h)),
            other => out.push(other.clone()),
        }
    }
    out
}

/// 把一个 HATCH 的每条边界环展平成线段（弧按 24 段采样），再用奇偶规则裁成 45° 线。
fn hatch_preview_lines(h: &Hatch) -> Vec<EntityType> {
    let mut out = Vec::new();
    let step = 2.0 * ANSI31_OFFSET * h.pattern_scale.max(0.05);
    for path in &h.paths {
        let mut segs: Vec<([f64; 2], [f64; 2])> = Vec::new();
        for edge in &path.edges {
            match edge {
                BoundaryEdge::Line(e) => {
                    segs.push(([e.start.x, e.start.y], [e.end.x, e.end.y]))
                }
                BoundaryEdge::CircularArc(a) => {
                    let tau = std::f64::consts::TAU;
                    let sweep = if a.counter_clockwise {
                        (a.end_angle - a.start_angle).rem_euclid(tau)
                    } else {
                        -((a.start_angle - a.end_angle).rem_euclid(tau))
                    };
                    let point = |angle: f64| {
                        [
                            a.center.x + a.radius * angle.cos(),
                            a.center.y + a.radius * angle.sin(),
                        ]
                    };
                    let n = 24;
                    let mut prev = point(a.start_angle);
                    for i in 1..=n {
                        let p = point(a.start_angle + sweep * i as f64 / n as f64);
                        segs.push((prev, p));
                        prev = p;
                    }
                }
                BoundaryEdge::Polyline(p) => {
                    let pts: Vec<[f64; 2]> = p.vertices.iter().map(|v| [v.x, v.y]).collect();
                    for i in 0..pts.len().saturating_sub(1) {
                        segs.push((pts[i], pts[i + 1]));
                    }
                    if p.is_closed && pts.len() > 2 {
                        segs.push((pts[pts.len() - 1], pts[0]));
                    }
                }
                _ => {}
            }
        }
        if segs.is_empty() {
            continue;
        }
        let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
        let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (p, q) in &segs {
            for pt in [p, q] {
                x0 = x0.min(pt[0]);
                x1 = x1.max(pt[0]);
                y0 = y0.min(pt[1]);
                y1 = y1.max(pt[1]);
            }
        }
        // 45° 线方向 (1,1)：`x − y = c`；相邻线间距（法向）= step / √2。
        let k0 = ((x0 - y1) / step).floor() as i64 - 1;
        let k1 = ((x1 - y0) / step).ceil() as i64 + 1;
        for k in k0..=k1 {
            let c = k as f64 * step;
            let mut ts: Vec<f64> = Vec::new();
            for &(p, q) in &segs {
                let (cp, cq) = (p[0] - p[1], q[0] - q[1]);
                if (cp <= c) == (cq <= c) || (cp - cq).abs() < 1e-12 {
                    continue;
                }
                let t = (c - cp) / (cq - cp);
                let ix = p[0] + (q[0] - p[0]) * t;
                let iy = p[1] + (q[1] - p[1]) * t;
                ts.push(ix + iy); // 沿 45° 方向的参数
            }
            ts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            for pair in ts.chunks_exact(2) {
                let (a, b) = (pair[0], pair[1]);
                let p = [(a + c) / 2.0, (a - c) / 2.0];
                let q = [(b + c) / 2.0, (b - c) / 2.0];
                out.push(line(p, q, LAYER_HATCH));
            }
        }
    }
    out
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

    /// 图元序列 → CSV（列：entity,x1,y1,x2,y2,layer,cx,cy,r,a0,a1）。
    /// LINE 用两端点；ARC 用起终点 + 圆心/半径/角度；HATCH 用边界 bbox 一行。
    fn entities_csv(entities: &[EntityType]) -> String {
        let mut csv = String::from("entity,x1,y1,x2,y2,layer,cx,cy,r,a0,a1\n");
        for entity in entities {
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
                EntityType::Hatch(h) => {
                    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
                    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
                    for path in &h.paths {
                        for edge in &path.edges {
                            match edge {
                                BoundaryEdge::Line(e) => {
                                    for p in [e.start, e.end] {
                                        x0 = x0.min(p.x);
                                        x1 = x1.max(p.x);
                                        y0 = y0.min(p.y);
                                        y1 = y1.max(p.y);
                                    }
                                }
                                BoundaryEdge::CircularArc(e) => {
                                    x0 = x0.min(e.center.x - e.radius);
                                    x1 = x1.max(e.center.x + e.radius);
                                    y0 = y0.min(e.center.y - e.radius);
                                    y1 = y1.max(e.center.y + e.radius);
                                }
                                _ => {}
                            }
                        }
                    }
                    if !x0.is_finite() {
                        x0 = 0.0;
                        y0 = 0.0;
                        x1 = 0.0;
                        y1 = 0.0;
                    }
                    csv.push_str(&format!(
                        "HATCH,{:.6},{:.6},{:.6},{:.6},{},,,,,\n",
                        x0, y0, x1, y1, h.common.layer
                    ));
                }
                other => panic!("dump 只支持 LINE/ARC/HATCH，得到 {other:?}"),
            }
        }
        csv
    }

    const DEMO: &str = "\
S30 E30 L45 CH2@L
S40 E40 L30 CH2@R OV3
S50 E30 L20
S30 E30 L15 CH2@R
S40 E40 L7 M1.5
S36 E36 L5
GEAR M3 Z20";

    // ── 解析 ──────────────────────────────────────────────────────────────

    #[test]
    fn dsl_demo_parses() {
        let program = parse_program(DEMO).unwrap();
        assert_eq!(program.segments.len(), 7);
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
        // M1.5：螺纹段，螺距 P=1.5（第 5 段）
        assert_eq!(
            program.segments[4].thread,
            Some(Thread { pitch: Some(1.5) })
        );
        // 退刀槽 = 一小段小直径轴段（第 6 段 Ø36）
        assert_eq!(program.segments[5].s, 36.0);
        assert_eq!(program.segments[5].thread, None);
        // GEAR M3 Z20：d = m·z = 60；H 省略 = 10m = 30
        let gear = program.segments[6].gear.expect("第 7 段是齿轮段");
        assert_eq!((gear.m, gear.z), (3.0, 20));
        assert_eq!(gear.h, None);
        assert!(near(program.segments[6].s, 60.0));
        assert!(near(program.segments[6].e, 60.0));
        assert!(near(program.segments[6].l, 30.0));
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
        // 二期关键字（THREAD / Z / H / KEYWAY）→ 二期未实现 + 行号/词，不静默忽略
        for token in ["THREAD", "Z10", "H20", "KEYWAY"] {
            let text = format!("S30 E30 L45\nS40 E40 L30 {token}");
            let err = parse_program(&text).unwrap_err();
            assert!(err.contains("第 2 行"), "{token}: {err}");
            assert!(err.contains(token), "{token}: {err}");
            assert!(err.contains("二期未实现"), "{token}: {err}");
        }
        // ES 已取消：报错 + 指路（写法示例）
        for token in ["ES5*3", "ES", "ES2*1@L"] {
            let text = format!("S30 E30 L45\nS40 E40 L12 {token}");
            let err = parse_program(&text).unwrap_err();
            assert!(err.contains("第 2 行"), "{token}: {err}");
            assert!(err.contains("ES` 已取消"), "{token}: {err}");
            assert!(err.contains("S24 E24 L5"), "{token}: {err}");
        }
        // 同一行多段时标注「第 k 段」（GEAR 段缺 M）
        let err = parse_program("S30 E30 L10 | GEAR").unwrap_err();
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
        // M 光杆现在是合法的螺纹段标记（不再报二期）
        let m = parse_program("S30 E30 L20 M").unwrap();
        assert_eq!(m.segments[0].thread, Some(Thread { pitch: None }));
    }

    #[test]
    fn json_model_matches_dsl() {
        let json = r#"{"segments":[
            {"s":30,"e":30,"l":45,"ch":[{"c":2,"end":"L"}]},
            {"s":40,"e":40,"l":30,"ch":{"c":2,"end":"R"},"ov":{"b1":3,"end":"R"}},
            {"s":50,"e":30,"l":20},
            {"s":30,"e":30,"l":15,"ch":[{"c":2,"end":"R"}]},
            {"s":40,"e":40,"l":7,"thread":1.5},
            {"s":36,"e":36,"l":5},
            {"gear":{"m":3,"z":20}}
        ],"at":[10,20],"rot":90}"#;
        let program = parse_program(json).unwrap();
        assert_eq!(program.segments.len(), 7);
        assert_eq!(program.at, Some([10.0, 20.0]));
        assert_eq!(program.rot, Some(90.0));
        assert_eq!(program.segments, parse_program(DEMO).unwrap().segments);
        // `at` 也接受 {"x":…,"y":…}
        let program = parse_program(r#"{"segments":[{"s":30,"l":10}],"at":{"x":1,"y":2}}"#).unwrap();
        assert_eq!(program.at, Some([1.0, 2.0]));
        // `thread` 多种写法：true / 数字 / 对象 / 字符串
        for text in [
            r#"{"segments":[{"s":30,"l":10,"thread":true}]}"#,
            r#"{"segments":[{"s":30,"l":10,"thread":{"p":1.5}}]}"#,
            r#"{"segments":[{"s":30,"l":10,"thread":"M1.5"}]}"#,
        ] {
            let program = parse_program(text).unwrap_or_else(|e| panic!("{text}: {e}"));
            let want = if text.contains("true") {
                None
            } else {
                Some(1.5)
            };
            assert_eq!(program.segments[0].thread, Some(Thread { pitch: want }));
        }
        // JSON 里写旧 `es` 字段 → 报取消 + 指路
        let err = parse_program(r#"{"segments":[{"s":30,"l":10,"es":{"b":5,"h":3}}]}"#)
            .unwrap_err();
        assert!(err.contains("`ES` 已取消") && err.contains("S24 E24 L5"), "{err}");
        assert!(parse_program("{not json").unwrap_err().contains("JSON"));
    }

    #[test]
    fn json_round_trip_and_block_name() {
        // 序列化 → 再解析等于原 program（GUI 的 parse/export 走同一条路）。
        let program = parse_program(DEMO).unwrap();
        let json = serde_json::to_string(&program).unwrap();
        assert!(json.contains("\"thread\":1.5"), "螺距序列化成数字：{json}");
        assert_eq!(parse_program(&json).unwrap(), program);
        // 光杆 M 序列化成 true；再解析回来
        let program = parse_program("S30 E30 L20 M").unwrap();
        let json = serde_json::to_string(&program).unwrap();
        assert!(json.contains("\"thread\":true"), "{json}");
        assert_eq!(parse_program(&json).unwrap(), program);
        // 齿轮段的派生 s/e/l 允许回传（= 派生值）；不一致就报错
        let program = parse_program("GEAR M3 Z20").unwrap();
        assert_eq!(parse_program(&serde_json::to_string(&program).unwrap()).unwrap(), program);
        let bad = r#"{"segments":[{"s":999,"gear":{"m":3,"z":20}}]}"#;
        assert!(parse_program(bad).unwrap_err().contains("应等于分度圆"));
        let bad = r#"{"segments":[{"l":999,"gear":{"m":3,"z":20}}]}"#;
        assert!(parse_program(bad).unwrap_err().contains("应等于齿宽"));
        // 块名：只由几何决定（同模型稳定、不同模型不同）
        let a = parse_program("S30 E30 L20 M").unwrap();
        let b = parse_program("s30 l20 e30 m").unwrap();
        let c = parse_program("S30 E30 L21 M").unwrap();
        assert_eq!(block_name(&a), block_name(&b));
        assert_ne!(block_name(&a), block_name(&c));
        assert!(block_name(&a).starts_with("OCSM_SHAFT_"));
    }

    #[test]
    fn preview_svg_renders_and_reports_errors() {
        let svg = preview_svg(&parse_program(DEMO).unwrap()).unwrap();
        assert!(svg.starts_with("<svg") || svg.contains("<svg"), "{svg}");
        assert!(svg.contains("#5aa0ff"), "细线层颜色（螺纹小径/砂轮细线）");
        assert!(svg.contains("#ff5555"), "中心线层颜色");
        // 剖视：HATCH 在预览里被裁成真实边界内的 45° 线（剖面线层绿色）
        let section = preview_svg(&parse_program("S30 E30 L20 VIEW 剖视").unwrap()).unwrap();
        assert!(section.contains("#3fa13f"), "剖面线颜色");
        // 几何非法同样带「第 N 段」原因
        let err = preview_svg(&parse_program("S30 E40 L10 M").unwrap()).unwrap_err();
        assert!(err.contains("第 1 段") && err.contains("圆柱"), "{err}");
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

    // ── 螺纹段 M ─────────────────────────────────────────────────────────

    #[test]
    fn dsl_thread_parses_and_reports_errors() {
        // 光杆 M = 简化画法；M1.5 / M=1.5 = 螺距 P
        let program = parse_program("S30 E30 L20 M").unwrap();
        assert_eq!(program.segments[0].thread, Some(Thread { pitch: None }));
        let program = parse_program("S36 E36 L5 M1.5").unwrap();
        assert_eq!(
            program.segments[0].thread,
            Some(Thread { pitch: Some(1.5) })
        );
        let program = parse_program("S36 E36 L5 M=1.5").unwrap();
        assert_eq!(
            program.segments[0].thread,
            Some(Thread { pitch: Some(1.5) })
        );
        // 可与 CH 同段（螺纹端倒角是常规画法）、段内顺序无关
        let program = parse_program("S30 E30 L20 CH2@R M1.5").unwrap();
        assert_eq!(program.segments[0].ch, vec![Chamfer { c: 2.0, end: End::R }]);
        assert_eq!(
            program.segments[0].thread,
            Some(Thread { pitch: Some(1.5) })
        );
        // 解析错误（行号 + 原因）
        let cases: &[(&str, &str)] = &[
            ("S30 E30 L20 M0", "螺距 P=0"),
            ("S30 E30 L20 M-1.5", "螺距 P=-1.5"),
            ("S30 E30 L20 M1.5 M2", "M 重复"),
            ("S30 E30 L20 M OV3", "不能与越程槽"),
            ("GEAR M3 Z20 M1.5", "M 重复"),
            ("M1.5 GEAR M3 Z20", "M 重复"),
        ];
        for (text, needle) in cases {
            let err = parse_program(text).unwrap_err();
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
    }

    #[test]
    fn thread_geometry_minor_lines_and_termination() {
        // 自由端 M1.5：d1 = 30 − 1.0825×1.5 = 28.37625，半径 14.188125
        let program = parse_program("S30 E30 L20 M1.5").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let r1 = (30.0 - 1.0825 * 1.5) / 2.0;
        assert!(near(r1, 14.188125));
        // 大径轮廓还是 ±15；小径细实线贯穿该段，落 2细线层
        assert!(has_line(&shaft, [0.0, 15.0], [20.0, 15.0]));
        assert!(has_line(&shaft, [0.0, r1], [20.0, r1]));
        assert!(has_line(&shaft, [0.0, -r1], [20.0, -r1]));
        let thin: Vec<&str> = shaft
            .entities
            .iter()
            .filter(|e| layer_of(e) == LAYER_THIN)
            .map(layer_of)
            .collect();
        assert_eq!(thin.len(), 2, "小径细实线上下各一条");
        // 小径细实线止于段末终止线（此处 = 自由端端面）
        assert!(has_line(&shaft, [20.0, -15.0], [20.0, 15.0]));
        // 光杆 M：0.85d = 25.5，半径 12.75
        let shaft = build(&parse_program("S30 E30 L20 M").unwrap(), 1.0).unwrap();
        assert!(has_line(&shaft, [0.0, 12.75], [20.0, 12.75]));
        // 螺纹段 + 小直径退刀槽段：终止线 = 既有台阶面（18 ↔ 20）
        let program = parse_program("S40 E40 L7 M1.5\nS36 E36 L5").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let r1 = (40.0 - 1.0825 * 1.5) / 2.0;
        assert!(near(r1, 19.188125));
        assert!(has_line(&shaft, [0.0, r1], [7.0, r1]));
        assert!(
            has_line(&shaft, [7.0, 18.0], [7.0, 20.0]),
            "终止线（既有台阶面）从退刀槽底 18 到螺纹大径 20"
        );
        // 同段端面有深倒角（伸进小径）→ 细实线止于倒角斜线交点，不挑出材料外
        let program = parse_program("S30 E30 L20 M1.5 CH2@R").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let r1 = (30.0 - 1.0825 * 1.5) / 2.0;
        let cut = 15.0 - r1;
        assert!(has_line(&shaft, [0.0, r1], [20.0 - (2.0 - cut), r1]));
        assert!(has_line(&shaft, [18.0, 15.0], [20.0, 13.0]), "倒角线还在");
        // 浅倒角（切不到小径）→ 细实线仍到端面
        let program = parse_program("S30 E30 L20 M5 CH2@R").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let r1 = (30.0 - 1.0825 * 5.0) / 2.0; // 12.29375；cut = 2.70625 > c=2
        assert!(has_line(&shaft, [0.0, r1], [20.0, r1]));
    }

    #[test]
    fn thread_validity_errors_point_at_the_segment() {
        let cases: &[(&str, &str)] = &[
            ("S30 E40 L20 M1.5", "必须是圆柱"),
            ("S30 E30 L20 M30", "太大"),
        ];
        for (text, needle) in cases {
            let program = parse_program(text).unwrap_or_else(|e| panic!("{text}: 解析失败 {e}"));
            let err = build(&program, 1.0).unwrap_err();
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
        // 手搓模型（绕过 parser）：M 与 OV 同段也不能通过 validate
        let segment = Segment {
            s: 30.0,
            e: 30.0,
            l: 20.0,
            ch: Vec::new(),
            ov: vec![Overtravel { b1: Some(3.0), end: End::R }],
            thread: Some(Thread { pitch: Some(1.5) }),
            gear: None,
        };
        let err = validate(&Program {
            segments: vec![segment],
            at: None,
            rot: None,
            view: ShaftView::Normal,
        })
        .unwrap_err();
        assert!(err.contains("不能与越程槽"), "{err}");
    }

    // ── 齿轮段 GEAR ──────────────────────────────────────────────────────

    #[test]
    fn dsl_gear_parses_and_derives() {
        // 直齿轮段：d = m·z 导出；默认齿宽 10m
        let program = parse_program("GEAR M3 Z20").unwrap();
        let seg = &program.segments[0];
        assert!(near(seg.s, 60.0) && near(seg.e, 60.0));
        assert!(near(seg.l, 30.0), "默认 H = 10m");
        let gear = seg.gear.expect("齿轮段");
        assert_eq!((gear.m, gear.z), (3.0, 20));
        assert_eq!(gear.h, None);
        // H 显式 + BETA0（直齿）可写
        let program = parse_program("GEAR M5 Z10 H20 BETA0").unwrap();
        let seg = &program.segments[0];
        assert!(near(seg.l, 20.0));
        assert_eq!(seg.gear.unwrap().h, Some(20.0));
        // 大小写 / 顺序无所谓（Z10 H20 GEAR M5）
        let shuffled = parse_program("z10 h20 gear m5").unwrap();
        assert_eq!(shuffled.segments[0], program.segments[0]);
        // 派生尺寸与 gear.rs 同口径（ra/da/2、r/d/2、rf/df/2）
        let gear = parse_program("GEAR M5 Z10 H20")
            .unwrap()
            .segments[0]
            .gear
            .unwrap();
        let params = crate::gear::GearParams {
            kind: crate::gear::GearKind::External,
            m: 5.0,
            z: 10,
            h: 20.0,
            ..crate::gear::GearParams::default()
        };
        assert!(near(gear.pitch_radius(), params.d() / 2.0));
        assert!(near(gear.addendum_radius(), params.da() / 2.0));
        assert!(near(gear.root_radius(), params.df() / 2.0));
        assert!(near(gear.pitch_radius(), 25.0));
        assert!(near(gear.addendum_radius(), 30.0));
        assert!(near(gear.root_radius(), 18.75));
    }

    #[test]
    fn dsl_gear_reports_errors() {
        let cases: &[(&str, &str)] = &[
            ("GEAR M3", "缺少 Z"),
            ("GEAR Z20", "缺少 M"),
            ("GEAR M3 Z20.5", "不是正整数"),
            ("GEAR M0 Z20", "模数 m 必须是正数"),
            ("GEAR M-3 Z20", "模数 m 必须是正数"),
            ("GEAR M3 Z1", "齿数 z 超出范围"),
            ("GEAR M3 Z20 S40", "不给 S/E"),
            ("GEAR M3 Z20 E40", "不给 S/E"),
            ("GEAR M3 Z20 L30", "不要再给 L"),
            ("GEAR M3 Z20 BETA12", "二期未实现"),
            ("GEAR M3 Z20 H0", "厚度 h 必须是正数"),
            ("GEAR M3 Z20 CH2", "不能与倒角"),
            ("GEAR M3 Z20 OV", "不能与越程槽"),
            ("GEAR M3 Z20 M1.5", "M 重复"),
            ("GEAR GEAR M3 Z20", "GEAR 重复"),
            ("S40 E40 L20 GEAR M3 Z20", "不给 S/E"),
        ];
        for (text, needle) in cases {
            let err = parse_program(text).unwrap_err();
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
        // GEAR 的 m/z 错误也要能按「第 N 段」定位（含跨行）
        let err = parse_program("GEAR M0 Z20").unwrap_err();
        assert!(err.contains("第 1 段"), "{err}");
        let err = parse_program("S30 E30 L10\nGEAR M3 Z20.5").unwrap_err();
        assert!(err.contains("第 2 段"), "{err}");
    }

    #[test]
    fn gear_geometry_addendum_pitch_layers_without_root_line() {
        // 第 1 段 Ø40，第 2 段齿轮 M3 Z20（x=20..50）：d=60、ra=33、rf=26.25（rf 不落图）
        let program = parse_program("S40 E40 L20\nGEAR M3 Z20 H30").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let gear = program.segments[1].gear.unwrap();
        let (r, ra, rf) = (
            gear.pitch_radius(),
            gear.addendum_radius(),
            gear.root_radius(),
        );
        // 齿顶线 = 该段轮廓（粗实线，上下各一条）
        assert!(has_line(&shaft, [20.0, ra], [50.0, ra]));
        assert!(has_line(&shaft, [20.0, -ra], [50.0, -ra]));
        let layer_of_y = |y: f64| -> Option<&str> {
            shaft.entities.iter().find_map(|e| match e {
                EntityType::Line(l)
                    if near(l.start.y, y)
                        && near(l.end.y, y)
                        && near(l.start.x, 20.0)
                        && near(l.end.x, 50.0) =>
                {
                    Some(l.common.layer.as_str())
                }
                _ => None,
            })
        };
        // 分度线 → 3中心线层（点划线）
        assert!(has_line(&shaft, [20.0, r], [50.0, r]));
        assert!(has_line(&shaft, [20.0, -r], [50.0, -r]));
        assert_eq!(layer_of_y(r), Some(crate::partgen_kit::LAYER_CENTER));
        // 齿根线：与 gear.rs::side_view() 一致，**不画**
        assert!(!has_line(&shaft, [20.0, rf], [50.0, rf]), "不画齿根线");
        assert!(!has_line(&shaft, [20.0, -rf], [50.0, -rf]), "不画齿根线");
        assert_eq!(layer_of_y(rf), None, "齿根线上没有图元");
        // 分度圆半径 30 不是轮廓（轮廓在齿顶 33）
        assert!(!shaft.entities.iter().any(|e| matches!(e, EntityType::Line(l)
            if near(l.start.y, r) && near(l.end.y, r) && l.common.layer == crate::partgen_kit::LAYER_MAIN)));
        // 端面：x=20 从 Ø40/2=20 到 ra=33；x=50 自由端面 ±33
        assert!(has_line(&shaft, [20.0, 20.0], [20.0, 33.0]));
        assert!(has_line(&shaft, [50.0, -33.0], [50.0, 33.0]));
        // 最大直径按齿顶圆算
        assert!(near(shaft.max_diameter, 66.0));
        assert!(near(shaft.total_length, 50.0));
    }

    #[test]
    fn gear_adjacent_diameter_rule_only_addendum_matters() {
        // Ø58（r=29 > 齿根 26.25，但 ≤ 齿顶 33）→ 放行（不再受 rf 限制）
        let program = parse_program("S58 E58 L20\nGEAR M3 Z20").unwrap();
        assert!(build(&program, 1.0).is_ok(), "相邻半径 ≤ ra 应放行");
        // 和齿顶齐平（Ø66）也放行
        let program = parse_program("S66 E66 L20\nGEAR M3 Z20").unwrap();
        assert!(build(&program, 1.0).is_ok());
        // Ø70（r=35 > 齿顶 33）→ 报「第 N 段」+「会盖住齿顶线」
        let program = parse_program("S70 E70 L20\nGEAR M3 Z20").unwrap();
        let err = build(&program, 1.0).unwrap_err();
        assert!(err.contains("第 2 段") && err.contains("会盖住齿顶线"), "{err}");
        // 同参数相邻齿轮（齿顶齐平）→ 放行
        let program = parse_program("GEAR M3 Z20 H20\nGEAR M3 Z20 H30").unwrap();
        assert!(build(&program, 1.0).is_ok());
        // 倒角会落在齿轮段上 → 报（第 1 段 Ø40 的 CH2@R 贴到齿轮凸角）
        let program = parse_program("S40 E40 L20 CH2@R\nGEAR M3 Z20").unwrap();
        let err = build(&program, 1.0).unwrap_err();
        assert!(err.contains("齿轮段"), "{err}");
    }

    // ── 视图 VIEW（常规 / 剖视 / 双） ────────────────────────────────────

    #[test]
    fn dsl_view_parses_default_inline_standalone_and_json() {
        // 默认 = 常规
        assert_eq!(parse_program("S30 E30 L10").unwrap().view, ShaftView::Normal);
        // 独立一行 / 段内关键字 / = 或 : / 中文或英文
        for (text, want) in [
            ("VIEW 剖视\nS30 E30 L10", ShaftView::Section),
            ("S30 E30 L10 VIEW 双", ShaftView::Both),
            ("VIEW=section S30 E30 L10", ShaftView::Section),
            ("S30 E30 L10 视图:剖视图", ShaftView::Section),
            ("VIEW 常规\nS30 E30 L10", ShaftView::Normal),
            ("S30 E30 L10", ShaftView::Normal),
        ] {
            assert_eq!(parse_program(text).unwrap().view, want, "{text}");
        }
        // 所有键可解析回自身
        for view in ShaftView::ALL {
            assert_eq!(ShaftView::parse(view.key()).unwrap(), view);
        }
        // 多段行里也可放（且不影响段数）
        let program = parse_program("VIEW 剖视 | S30 E30 L10 | S20 E20 L5").unwrap();
        assert_eq!(program.view, ShaftView::Section);
        assert_eq!(program.segments.len(), 2);
        // JSON 同名字段：英文键或中文名
        for view in ["section", "剖视", "both", "双", "normal"] {
            let text = format!(r#"{{"segments":[{{"s":30,"l":10}}],"view":"{view}"}}"#);
            assert_eq!(
                parse_program(&text).unwrap().view,
                ShaftView::parse(view).unwrap()
            );
        }
        // 序列化回传（GUI/HTTP）：view 键可来回
        let program = parse_program("VIEW 双\nS30 E30 L10").unwrap();
        let json = serde_json::to_string(&program).unwrap();
        assert!(json.contains("\"view\":\"both\""), "{json}");
        assert_eq!(parse_program(&json).unwrap(), program);
        // 错误：视图名非法 / 缺名 / 冲突
        let err = parse_program("VIEW 隐藏\nS30 E30 L10").unwrap_err();
        assert!(err.contains("视图名无法识别"), "{err}");
        let err = parse_program("VIEW\nS30 E30 L10").unwrap_err();
        assert!(err.contains("缺少视图名"), "{err}");
        let err = parse_program("VIEW 剖视\nS30 E30 L10\nVIEW 双").unwrap_err();
        assert!(err.contains("重复且冲突"), "{err}");
        // 块名把视图算进去（常规 ≠ 剖视）
        let normal = parse_program("S30 E30 L10").unwrap();
        let section = parse_program("S30 E30 L10 VIEW 剖视").unwrap();
        assert_ne!(block_name(&normal), block_name(&section));
    }

    #[test]
    fn section_view_hatch_ring_closes_on_hatch_layer() {
        // 带自由端倒角 + 越程槽 + 锥面：确认环每段接得上、最后闭合
        let program = parse_program(
            "S30 E30 L20 CH2@L\nS40 E40 L10 OV3\nS50 E30 L12\nVIEW 剖视",
        )
        .unwrap();
        // 常规视图没有 HATCH
        let normal = build(
            &Program {
                view: ShaftView::Normal,
                ..program.clone()
            },
            1.0,
        )
        .unwrap();
        assert!(!normal.entities.iter().any(|e| matches!(e, EntityType::Hatch(_))));
        let shaft = build(&program, 1.0).unwrap();
        let hatches: Vec<&Hatch> = shaft
            .entities
            .iter()
            .filter_map(|e| match e {
                EntityType::Hatch(h) => Some(h),
                _ => None,
            })
            .collect();
        assert_eq!(hatches.len(), 1, "剖视只有一个 HATCH（一个简单环）");
        let h = hatches[0];
        assert_eq!(h.common.layer, crate::partgen_kit::LAYER_HATCH);
        assert_eq!(h.pattern.name, "ANSI31");
        assert_eq!(h.pattern_scale, 1.0);
        assert_eq!(h.paths.len(), 1, "上半 + 镜像合成一个简单环");
        // 边界首尾相接（每个 edge 的终点 = 下一个 edge 的起点）
        let pts = |edge: &BoundaryEdge| -> ([f64; 2], [f64; 2]) {
            match edge {
                BoundaryEdge::Line(e) => ([e.start.x, e.start.y], [e.end.x, e.end.y]),
                BoundaryEdge::CircularArc(a) => (
                    [
                        a.center.x + a.radius * a.start_angle.cos(),
                        a.center.y + a.radius * a.start_angle.sin(),
                    ],
                    [
                        a.center.x + a.radius * a.end_angle.cos(),
                        a.center.y + a.radius * a.end_angle.sin(),
                    ],
                ),
                other => panic!("剖面环只应有 LINE/ARC，得到 {other:?}"),
            }
        };
        let edges = &h.paths[0].edges;
        assert!(edges.len() >= 8, "轮廓段数：{}", edges.len());
        let mut prev: Option<[f64; 2]> = None;
        let mut first: Option<[f64; 2]> = None;
        for edge in edges {
            let (a, b) = pts(edge);
            if let Some(p) = prev {
                assert!(near(p[0], a[0]) && near(p[1], a[1]), "边界断开：{p:?} → {a:?}");
            } else {
                first = Some(a);
            }
            prev = Some(b);
        }
        let start = first.unwrap();
        let last = prev.unwrap();
        assert!(
            near(last[0], start[0]) && near(last[1], start[1]),
            "环未闭合：{last:?} → {start:?}"
        );
        // 环的 bbox 盖住整个轴剖面（总长 42，最大半径 25）
        let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
        let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for edge in edges {
            let (a, b) = pts(edge);
            for p in [a, b] {
                x0 = x0.min(p[0]);
                x1 = x1.max(p[0]);
                y0 = y0.min(p[1]);
                y1 = y1.max(p[1]);
            }
        }
        assert!(
            near(x0, 0.0) && near(x1, 42.0) && near(y0, -25.0) && near(y1, 25.0),
            "环 bbox = ({x0},{y0})..({x1},{y1})"
        );
    }

    #[test]
    fn both_view_places_section_side_by_side_with_gap_rule() {
        // 短轴：间距 = max(20×15%=3, 40×1) 取 40 → 第二张起点 x = 20 + 40 = 60
        let program = parse_program("S30 E30 L20 VIEW 双").unwrap();
        let normal = build(
            &Program {
                view: ShaftView::Normal,
                ..program.clone()
            },
            1.0,
        )
        .unwrap();
        let both = build(&program, 1.0).unwrap();
        assert_eq!(
            both.entities.len(),
            normal.entities.len() * 2 + 1,
            "常规×2 + 一个 HATCH"
        );
        assert!(has_line(&both, [60.0, 15.0], [80.0, 15.0]), "第二张轮廓右移 60");
        assert!(has_line(&both, [57.0, 0.0], [83.0, 0.0]), "第二张轴线");
        // 第二张的 HATCH 边界也右移 60
        let hatch = both
            .entities
            .iter()
            .find_map(|e| match e {
                EntityType::Hatch(h) => Some(h),
                _ => None,
            })
            .expect("双视图要有一个 HATCH");
        let mut min_x = f64::INFINITY;
        for path in &hatch.paths {
            for edge in &path.edges {
                if let BoundaryEdge::Line(e) = edge {
                    min_x = min_x.min(e.start.x).min(e.end.x);
                }
            }
        }
        assert!(near(min_x, 60.0), "HATCH 边界右移：min_x={min_x}");
        // 长轴：间距 = 总长×15%（400 → 60）
        let long = parse_program("S30 E30 L400 VIEW 双").unwrap();
        let both_long = build(&long, 1.0).unwrap();
        assert!(
            has_line(&both_long, [460.0, 15.0], [860.0, 15.0]),
            "总长 400 时间距 = 60（15%）"
        );
    }

    #[test]
    fn place_transforms_hatch_boundaries() {
        let program = parse_program("S30 E30 L20 VIEW 剖视").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let bbox = |entities: &[EntityType]| -> [f64; 4] {
            let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
            let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
            for e in entities {
                if let EntityType::Hatch(h) = e {
                    for path in &h.paths {
                        for edge in &path.edges {
                            match edge {
                                BoundaryEdge::Line(e) => {
                                    for p in [e.start, e.end] {
                                        x0 = x0.min(p.x);
                                        x1 = x1.max(p.x);
                                        y0 = y0.min(p.y);
                                        y1 = y1.max(p.y);
                                    }
                                }
                                BoundaryEdge::CircularArc(a) => {
                                    for (px, py) in [
                                        (a.center.x + a.radius * a.start_angle.cos(),
                                         a.center.y + a.radius * a.start_angle.sin()),
                                        (a.center.x + a.radius * a.end_angle.cos(),
                                         a.center.y + a.radius * a.end_angle.sin()),
                                    ] {
                                        x0 = x0.min(px);
                                        x1 = x1.max(px);
                                        y0 = y0.min(py);
                                        y1 = y1.max(py);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            [x0, y0, x1, y1]
        };
        // 平移：边界 bbox → (100,35)..(120,65)
        let placed = place(shaft.entities, [100.0, 50.0], 0.0);
        let b = bbox(&placed);
        assert!(near(b[0], 100.0) && near(b[1], 35.0) && near(b[2], 120.0) && near(b[3], 65.0), "{b:?}");
        // 旋转 90°：原 (x,y) → (−y,x)，bbox x ∈ [−65,−35]、y ∈ [100,120]
        let rotated = place(placed, [0.0, 0.0], 90.0);
        let b = bbox(&rotated);
        assert!(near(b[0], -65.0) && near(b[1], 100.0) && near(b[2], -35.0) && near(b[3], 120.0), "{b:?}");
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

    /// 把 7 段 demo 的几何落成 CSV（列：entity,x1,y1,x2,y2,layer,cx,cy,r,a0,a1），
    /// 给用户画叠合/预览图。LINE 用两端点；ARC 用起终点 + 圆心/半径/角度。
    #[test]
    fn dump_demo_geometry_csv() {
        let program = parse_program(DEMO).unwrap();
        let shaft = build(&program, 1.0).unwrap();
        assert!(near(shaft.total_length, 152.0));
        assert!(near(shaft.max_diameter, 66.0));
        assert_eq!(shaft.segment_count, 7);

        let csv = entities_csv(&shaft.entities);

        // 关键几何自检（demo 7 段：30 / 40（OV3 + φ50 台阶倒角）/ 锥 50→30 / 30 /
        // 40（M1.5 螺纹段）/ 36（退刀槽）/ 齿轮段 M3 Z20（d=60、da=66、df=52.5 不落图））
        assert!(has_line(&shaft, [72.0, 20.0], [57.0, 35.0]), "φ40 的 OV 砂轮细线");
        assert!(
            has_line(&shaft, [75.0, 22.0], [77.0, 24.0]),
            "φ50 台阶左端的 CH2（贴凸角）"
        );
        assert!(
            has_line(&shaft, [110.0, 15.0], [110.0, 18.0]),
            "前段 φ30 → φ40 台阶面"
        );
        assert!(has_line(&shaft, [110.0, 18.0], [112.0, 20.0]), "φ40 段左端 CH2");
        // M1.5：轮廓到 117；小径 d1 = 40 − 1.0825×1.5 = 38.37625（r=19.188125）
        // 细实线从倒角斜线交点 111.188125 起到终止线 117
        let r1 = (40.0 - 1.0825 * 1.5) / 2.0;
        assert!(has_line(&shaft, [112.0, 20.0], [117.0, 20.0]), "螺纹段轮廓");
        assert!(
            has_line(&shaft, [111.188125, r1], [117.0, r1]),
            "小径细实线（止于终止线）"
        );
        assert!(has_line(&shaft, [111.188125, -r1], [117.0, -r1]), "下半小径细实线");
        assert!(
            has_line(&shaft, [117.0, 18.0], [117.0, 20.0]),
            "螺纹终止线 = 既有台阶面（18 ↔ 20）"
        );
        // 退刀槽 = 小直径轴段 Ø36；其轮廓 117→122
        assert!(has_line(&shaft, [117.0, 18.0], [122.0, 18.0]), "退刀槽段轮廓");
        assert!(
            has_line(&shaft, [122.0, 18.0], [122.0, 33.0]),
            "退刀槽端面从 18 到齿顶 33"
        );
        // 齿轮两线：齿顶（轮廓，±33）/ 分度（点划线，±30）；齿根线（±26.25）不画
        assert!(has_line(&shaft, [122.0, 33.0], [152.0, 33.0]), "齿顶线");
        assert!(has_line(&shaft, [122.0, 30.0], [152.0, 30.0]), "分度线");
        assert!(
            !has_line(&shaft, [122.0, 26.25], [152.0, 26.25]),
            "齿根线不画（与 gear.rs::side_view 一致）"
        );
        assert!(
            !has_line(&shaft, [122.0, -26.25], [152.0, -26.25]),
            "下半齿根线也不画"
        );
        assert!(
            has_line(&shaft, [152.0, -33.0], [152.0, 33.0]),
            "齿轮自由端面"
        );
        let layer_at = |x: f64, y: f64| -> Option<&str> {
            shaft.entities.iter().find_map(|e| match e {
                EntityType::Line(l)
                    if near(l.start.x, x) && near(l.start.y, y) && near(l.end.y, y) =>
                {
                    Some(l.common.layer.as_str())
                }
                _ => None,
            })
        };
        assert_eq!(
            layer_at(122.0, 30.0),
            Some(crate::partgen_kit::LAYER_CENTER)
        );
        assert_eq!(layer_at(122.0, 26.25), None, "没有齿根线");

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
        // 44 = 旧 demo 46 − 齿轮段齿根线 2（用户 2026-09-18：按 gear 工具口径去掉齿根线）
        assert_eq!(shaft.entities.len(), 44, "demo 图元数");

        // 剖视 diff：同一套轮廓 + 一个 HATCH（CSV 记 bbox 一行）；落盘供人工核对
        let section_program = parse_program(&format!("{DEMO}\nVIEW 剖视")).unwrap();
        let section = build(&section_program, 1.0).unwrap();
        assert_eq!(section.entities.len(), shaft.entities.len() + 1);
        let section_csv = entities_csv(&section.entities);
        assert!(
            section_csv.contains("HATCH,0.000000,-33.000000,152.000000,33.000000,5剖面线层"),
            "HATCH 一行记 bbox + 5剖面线层：{section_csv}"
        );
        let file = path.join("shaft_demo_section.csv");
        std::fs::write(&file, &section_csv).expect("写 shaft_demo_section.csv");
        assert!(file.is_file(), "剖面 CSV 已落盘：{}", file.display());
    }
}
