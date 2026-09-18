//! **结构要素**（轴上的工艺结构轮廓）参数化生成。
//!
//! 第一期：GB/T 6403.5-2008 砂轮越程槽 —— **磨外圆**（族 id `detail_grind_od`）。
//!
//! ## 数据出处（唯一数据源 = `GROOVE_BANDS`，改表只改这一处）
//! - 尺寸表：用户提供的参数表 `磨外圆_GB-T6403.png`（列：b1 / h / r / d）；
//! - 几何模板：`磨外圆_GB-T6403.5-2008.dxf`（1:1，逐图元反解核对；
//!   d=100 → 最后一行 b1=10 / h=1.2 / r=3）。
//!
//! ## 画法（模板 `1轮廓实线层` 逐图元反解；`2细线层` 砂轮线不再画）
//! 轴为 x 轴（x 向右、y = 半径），**锚点 = 台阶面与轴线交点 (0,0)**；上半侧 + 下半侧镜像：
//! - 台阶面竖线 `x=0`：`y = 0 … d/2 + r`（模板 53 = 50 + 3）；
//! - 圆角弧 `R=r`：圆心 `(r, d/2−h+r)`（模板 `(3, 51.8)`），与台阶面/槽底相切；
//! - 槽底 `y = d/2−h`：`x = r … b1−h`（模板 48.8：x 3…8.8）；
//! - 45° 斜线：`(b1−h, d/2−h) → (b1, d/2)`（模板 `(8.8,48.8) → (10,50)`）；
//! - 右端竖线 `x=b1`：`y = 0 … ±d/2`（与"磨出的外圆"闭合）；
//! - **不画**模板里的砂轮细实线（`2细线层`，从 `(b1, d/2)` 45° 上扬 15 的尾线）：
//!   用户 2026-09-18 定案，青线属标注性质（覆盖「模板即权威」）；回归测试里
//!   用 `15` 做「不应再出现」的负断言。
//! - 自洽关系：`b1 = r + 平段 + h`（45° 段水平长 = h）。
//! - **不画**模板里的 `10外部结构层` / `7标注层`。
//!
//! ## 扩展下一个要素（退刀槽 / 键槽）
//! 1. 在本文件写数据表 + 图元构造（照 `GROOVE_BANDS` / `build_grind_od`）；
//! 2. 实现 `DetailElement`（family / name / code / views / generate / catalog_extra / base_hint）；
//! 3. 把实例加进 `ELEMENTS` 即可：目录、文件树（`tree_dir` 决定挂哪棵根树）、
//!    GUI 自由输入面板、预览 SVG、出库/插入会经 `partgen::catalog_json`、
//!    `partgen::generate_requested` 自动接通，不必再改别的注册点。

use ocs_plugin_api::host::acadrust::entities::EntityType;

use crate::partgen::{GenPart, PartMeta};
use crate::partgen_kit::{arc, line, trim, LAYER_MAIN};

// ══════════════════════════════════════════════════════════════════════════
// 通用框架（一族 = 一个 DetailElement；新增要素只动本文件）
// ══════════════════════════════════════════════════════════════════════════

/// 结构要素的**通用数值参数**（键大小写不敏感，内部统一小写；保序便于错误信息稳定）。
///
/// 第一期磨外圆只有一个可选值 `b1`（历史槽位，仍走本结构）；第二期外螺纹退刀槽
/// 用 `P`（必给）+ `g1/g2/dg/r/alpha`（可选覆盖）——不给每个要素单加一个字段。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DetailParams {
    values: Vec<(String, f64)>,
    /// 可选的**规格代号**（字符串参数；`6x23x26x6` 这类代号不能塞进 f64 表）。
    spec: Option<String>,
}

impl DetailParams {
    pub fn new() -> Self {
        Self::default()
    }

    /// 规格代号（字符串参数；`preview_svg` / CLI / GUI 都经它传入）。
    pub fn spec(&self) -> Option<&str> {
        self.spec.as_deref()
    }

    /// 设置规格代号（空串 = 清除）。
    pub fn set_spec(&mut self, spec: impl Into<String>) {
        let spec = spec.into();
        self.spec = if spec.trim().is_empty() {
            None
        } else {
            Some(spec.trim().to_string())
        };
    }

    /// 旧入口的 `b1` 槽位 → 参数集（`None` = 空）。
    pub fn from_b1(b1: Option<f64>) -> Self {
        let mut params = Self::new();
        if let Some(value) = b1 {
            params.insert("b1", value);
        }
        params
    }

    /// 从 `(键, 值)` 列表构造（测试与后续批量入口用；目前只在 `#[cfg(test)]` 里调用）。
    #[allow(dead_code)]
    pub fn from_pairs<K: Into<String>>(pairs: impl IntoIterator<Item = (K, f64)>) -> Self {
        let mut params = Self::new();
        for (key, value) in pairs {
            params.insert(&key.into(), value);
        }
        params
    }

    /// 插入（键统一小写；同键覆盖原值）。
    pub fn insert(&mut self, key: &str, value: f64) {
        let key = key.to_ascii_lowercase();
        if let Some(slot) = self.values.iter_mut().find(|(name, _)| *name == key) {
            slot.1 = value;
        } else {
            self.values.push((key, value));
        }
    }

    pub fn get(&self, key: &str) -> Option<f64> {
        let key = key.to_ascii_lowercase();
        self.values
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| *value)
    }

    /// 历史 `b1` 槽位。
    pub fn b1(&self) -> Option<f64> {
        self.get("b1")
    }

    /// 全部键（小写，保持插入顺序）。
    pub fn keys(&self) -> Vec<&str> {
        self.values.iter().map(|(key, _)| key.as_str()).collect()
    }
}

/// 一个「结构要素」族。每族自带数据表与画法；d 必给，`b1` 可选覆盖。
pub trait DetailElement: Sync {
    /// 族 id（CLI / GUI / 树 / xdata 用）。
    fn family(&self) -> &'static str;
    /// 显示名（树叶子 = `name + " " + code`）。
    fn name(&self) -> &'static str;
    /// 标准号。
    fn code(&self) -> &'static str;
    /// 支持的视图 id（第一期都只有主视图）。
    fn views(&self) -> &'static [&'static str];
    /// 基点说明（GUI 面板用）。
    fn base_hint(&self) -> &'static str;
    /// 生成图元。`b1 = None` → 该 d 档默认行；`Some(b1)` → 档内按 b1 匹配（匹配不到报错）。
    fn generate(&self, d: f64, b1: Option<f64>, view: &str) -> Result<GenPart, String>;
    /// **通用参数**入口（第二期起的新要素实现它；默认只认历史 `b1`）。
    ///
    /// 老要素（磨外圆）不必实现：默认实现把 `b1` 之外的键当错误报出来，
    /// 保证「不认识 P/g1/… 的族」不会被静默按默认值出图。
    fn generate_params(
        &self,
        d: f64,
        params: &DetailParams,
        view: &str,
    ) -> Result<GenPart, String> {
        let extra: Vec<&str> = params
            .keys()
            .into_iter()
            .filter(|key| *key != "b1")
            .collect();
        if !extra.is_empty() {
            return Err(format!(
                "{}：不认识参数 {}（本族只支持 b1）",
                self.name(),
                extra.join("、")
            ));
        }
        self.generate(d, params.b1(), view)
    }
    /// 目录 JSON 的补充字段（`free_d` / `bands` 等，GUI 自由输入表单用）。
    fn catalog_extra(&self) -> serde_json::Value;
    /// 视图按钮的中文名覆盖（默认空 = 用 `partgen_kit::views_json` 的通用名）。
    fn view_labels(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }
    /// **规格代号入口**：把非数字的第二 token（`6x23x26x6`）交给族自己解析，
    /// 返回 `(主参数 d, 预设参数)`。默认 `None` = 本族不认规格代号。
    fn parse_spec_token(&self, _token: &str) -> Option<(f64, DetailParams)> {
        None
    }
}

/// 已登记的要素（新增要素往这里加一项）。
pub static ELEMENTS: &[&dyn DetailElement] = &[&GRIND_OD, &THREAD_RELIEF, &SPLINE_RECT];

/// 族 id → 要素定义。
pub fn find(family: &str) -> Option<&'static dyn DetailElement> {
    ELEMENTS.iter().copied().find(|e| e.family() == family)
}

/// 是否结构要素族（标准件/结构要素的分派判据）。
pub fn is_detail(family: &str) -> bool {
    find(family).is_some()
}

/// 族 → 视图 id（`partgen_more::family_views` 会先问这里）。
pub fn family_views(family: &str) -> Vec<&'static str> {
    find(family).map(|e| e.views().to_vec()).unwrap_or_default()
}

/// 结构要素的「规格代号」入口（CLI `XL` 用）：族不认返回 None。
pub fn parse_spec_token(family: &str, token: &str) -> Option<(f64, DetailParams)> {
    find(family).and_then(|element| element.parse_spec_token(token))
}

/// 生成（找不到族报错）。
pub fn generate(family: &str, d: f64, b1: Option<f64>, view: &str) -> Result<GenPart, String> {
    generate_params(family, d, &DetailParams::from_b1(b1), view)
}

/// 通用参数入口（外螺纹退刀槽等新要素；HTTP/CLI 的新参数都汇到这里）。
pub fn generate_params(
    family: &str,
    d: f64,
    params: &DetailParams,
    view: &str,
) -> Result<GenPart, String> {
    let element = find(family).ok_or_else(|| format!("结构要素族 {family} 尚未实现"))?;
    if !element.views().contains(&view) {
        return Err(format!(
            "{} 只有视图 {}（收到 {view}）",
            element.name(),
            element.views().join("/")
        ));
    }
    element.generate_params(d, params, view)
}

/// 生成（非结构要素族返回 None，交给标准件派发）。
pub fn try_generate(
    family: &str,
    d: f64,
    b1: Option<f64>,
    view: &str,
) -> Option<Result<GenPart, String>> {
    find(family).map(|_| generate(family, d, b1, view))
}

/// 各族目录 JSON 片段（键 = 族 id）——`partgen::catalog_json` 会合并进去。
pub fn families_json() -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for element in ELEMENTS {
        let mut entry = serde_json::json!({
            "id": element.family(),
            "name": element.name(),
            "code": element.code(),
            "iso": "—",
            "implemented": true,
            "views": crate::partgen_kit::views_json(element.family()),
            "sizes": [],
            "free_d": true,
            "len_label": "b1（mm，留空 = 该 d 档默认）",
            "base_hint": element.base_hint(),
        });
        // 视图按钮中文名可由族覆盖（花键：正视图/常规侧视图/侧剖视图）。
        if !element.view_labels().is_empty() {
            if let Some(views) = entry["views"].as_array_mut() {
                for view in views.iter_mut() {
                    if let Some(id) = view["id"].as_str() {
                        if let Some((_, label)) =
                            element.view_labels().iter().find(|(key, _)| *key == id)
                        {
                            view["name"] = serde_json::json!(label);
                        }
                    }
                }
            }
        }
        if let (Some(obj), Some(extra)) = (entry.as_object_mut(), element.catalog_extra().as_object())
        {
            for (key, value) in extra {
                obj.insert(key.clone(), value.clone());
            }
        }
        map.insert(element.family().to_string(), entry);
    }
    map
}

/// `/api/part_svg` 的**结构要素**分支（`family=…&d=…&view=main`）。
///
/// 除 `family` / `d` / `view` 外的键都当该族的参数（磨外圆的 `b1`、退刀槽的
/// `P/g1/g2/dg/r/alpha`）交给 `generate_params`；参数不是数字直接报错。
/// 返回 `None` = 不是结构要素，调用方走标准件分支。
pub fn preview_svg(query: &str) -> Option<Result<String, String>> {
    let pairs: Vec<(&str, &str)> = query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .collect();
    let get = |key: &str| {
        pairs
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| (*v).to_string())
    };
    let family = get("family")?;
    let element = find(&family)?;
    Some((|| {
        // `spec`（规格代号）可以替代 `d`（花键：d 由 N×d×D×B 派生）。
        let spec = get("spec").filter(|s| !s.trim().is_empty());
        let d: f64 = match get("d") {
            Some(text) => text.parse().map_err(|_| "d 不是数字".to_string())?,
            None if spec.is_some() => 0.0,
            None => return Err("缺少参数 d（或 spec 规格代号）".to_string()),
        };
        let view = get("view").unwrap_or_else(|| "main".to_string());
        let mut params = DetailParams::new();
        if let Some(spec) = spec {
            params.set_spec(spec);
        }
        for (key, value) in &pairs {
            // `family/d/view/l/spec` 已单独取；`l` 是标准件的长度槽位（结构要素里
            // 需要长度的族用 `len`）——这些不当参数。空值当没给（旧 `&b1=` 的行为）。
            if matches!(*key, "family" | "d" | "view" | "l" | "spec") || value.is_empty() {
                continue;
            }
            let value: f64 = value
                .parse()
                .map_err(|_| format!("参数 {key} 不是数字"))?;
            params.insert(key, value);
        }
        let part = generate_params(element.family(), d, &params, &view)?;
        Ok(crate::partgen::to_svg(
            &part,
            &format!("{} {}", part.meta.code, part.meta.spec),
            520.0,
            260.0,
        ))
    })())
}

/// 图元包围盒（`[xmin, ymin, xmax, ymax]`）。圆弧按**落在弧上的**四个象限点精确取。
pub(crate) fn entity_bbox(entities: &[EntityType]) -> [f64; 4] {
    let (mut x0, mut y0, mut x1, mut y1) =
        (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut put = |x: f64, y: f64| {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    };
    for e in entities {
        match e {
            EntityType::Line(l) => {
                put(l.start.x, l.start.y);
                put(l.end.x, l.end.y);
            }
            EntityType::Arc(a) => {
                put(a.center.x, a.center.y);
                let span = (a.end_angle - a.start_angle).rem_euclid(std::f64::consts::TAU);
                for k in 0..4 {
                    let angle = k as f64 * std::f64::consts::FRAC_PI_2;
                    if (angle - a.start_angle).rem_euclid(std::f64::consts::TAU) <= span + 1e-12 {
                        put(
                            a.center.x + a.radius * angle.cos(),
                            a.center.y + a.radius * angle.sin(),
                        );
                    }
                }
                put(
                    a.center.x + a.radius * a.start_angle.cos(),
                    a.center.y + a.radius * a.start_angle.sin(),
                );
                put(
                    a.center.x + a.radius * a.end_angle.cos(),
                    a.center.y + a.radius * a.end_angle.sin(),
                );
            }
            _ => {}
        }
    }
    [x0, y0, x1, y1]
}

// ══════════════════════════════════════════════════════════════════════════
// 磨外圆：数据表（GB/T 6403.5-2008）
// ══════════════════════════════════════════════════════════════════════════

/// 族 id。
pub const FAMILY_GRIND_OD: &str = "detail_grind_od";

/// 表的一行：一个 b1 方案。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrooveRow {
    /// 槽宽 b1（= r + 平段 + h；45° 段水平长 = h）。
    pub b1: f64,
    /// 槽深 h（相对磨出的外圆半径）。
    pub h: f64,
    /// 过渡圆角半径 r（槽底与台阶面之间）。
    pub r: f64,
}

/// 一个 d 档：档内 2~3 个 b1 方案；**缺省取最后一行**。
#[derive(Debug, Clone, Copy)]
pub struct GrooveBand {
    /// 区间的可读写法（GUI 提示 / 报错列出可选值）。
    pub label: &'static str,
    /// 下界；`lo_incl=false` 时开区间。
    pub lo: f64,
    pub lo_incl: bool,
    /// 上界；`None` = 无上界。
    pub hi: Option<f64>,
    pub hi_incl: bool,
    /// 候选行（顺序即"原表行序"；缺省 = `rows.last()`）。
    pub rows: &'static [GrooveRow],
}

/// **GB/T 6403.5-2008 磨外圆 数据唯一来源**（改表只改这里）。
///
/// 出处：用户提供的参数表 `磨外圆_GB-T6403.png`；模板
/// `磨外圆_GB-T6403.5-2008.dxf` 交叉验证（d=100 → 第 4 档最后一行 b1=10/h=1.2/r=3）。
///
/// 原表波浪号区间的定稿解释（用户 2026-09-18 确认）：
/// - 第 1 档 `≤10`     → `0 < d ≤ 10`
/// - 第 2 档 `>10~50`  → `10 < d ≤ 50`
/// - 第 3 档 `>50~100` → `50 < d < 100`（**不含 100**）
/// - 第 4 档 `>100`    → `d ≥ 100`（**含 100** —— d=100 落 b1=10，与模板一致）
///
/// 表覆盖 `d > 0` 全范围（只有 `d ≤ 0` / 非数才报错），不外推。
pub const GROOVE_BANDS: &[GrooveBand] = &[
    GrooveBand {
        label: "d ≤ 10",
        lo: 0.0,
        lo_incl: false,
        hi: Some(10.0),
        hi_incl: true,
        rows: &[
            GrooveRow { b1: 0.6, h: 0.1, r: 0.2 },
            GrooveRow { b1: 1.0, h: 0.2, r: 0.5 },
            GrooveRow { b1: 1.6, h: 0.2, r: 0.5 },
        ],
    },
    GrooveBand {
        label: "10 < d ≤ 50",
        lo: 10.0,
        lo_incl: false,
        hi: Some(50.0),
        hi_incl: true,
        rows: &[
            GrooveRow { b1: 2.0, h: 0.3, r: 0.8 },
            GrooveRow { b1: 3.0, h: 0.4, r: 1.0 },
        ],
    },
    GrooveBand {
        label: "50 < d < 100",
        lo: 50.0,
        lo_incl: false,
        hi: Some(100.0),
        hi_incl: false,
        rows: &[
            GrooveRow { b1: 4.0, h: 0.4, r: 1.0 },
            GrooveRow { b1: 5.0, h: 0.6, r: 1.6 },
        ],
    },
    GrooveBand {
        label: "d ≥ 100",
        lo: 100.0,
        lo_incl: true,
        hi: None,
        hi_incl: false,
        rows: &[
            GrooveRow { b1: 8.0, h: 0.8, r: 2.0 },
            GrooveRow { b1: 10.0, h: 1.2, r: 3.0 },
        ],
    },
];

impl GrooveBand {
    /// `d` 是否落在本档。
    fn contains(&self, d: f64) -> bool {
        let lower = if self.lo_incl { d >= self.lo } else { d > self.lo };
        let upper = match self.hi {
            None => true,
            Some(hi) => {
                if self.hi_incl {
                    d <= hi
                } else {
                    d < hi
                }
            }
        };
        lower && upper
    }
}

/// d → 所在档。`d ≤ 0` / 非有限数报错（表覆盖 d>0 全范围，不外推）。
pub fn band_of(d: f64) -> Result<&'static GrooveBand, String> {
    if !d.is_finite() || d <= 0.0 {
        return Err(format!("磨外圆：d 必须是正数（收到 {d}）"));
    }
    GROOVE_BANDS
        .iter()
        .find(|band| band.contains(d))
        .ok_or_else(|| format!("磨外圆：d={d} 不在数据表范围内"))
}

/// `d` + 可选 `b1` → 数据行。
///
/// - `b1 = None`：取该 d 档**最后一行**（d=100 → b1=10，与模板一致）；
/// - `b1 = Some(v)`：在该档候选行里按 `b1` 匹配；匹配不到报错并列出可选 b1。
pub fn row_for(d: f64, b1: Option<f64>) -> Result<&'static GrooveRow, String> {
    let band = band_of(d)?;
    match b1 {
        None => band
            .rows
            .last()
            .ok_or_else(|| "磨外圆：数据表为空".to_string()),
        Some(b1) => {
            if !b1.is_finite() || b1 <= 0.0 {
                return Err(format!("磨外圆：b1 必须是正数（收到 {b1}）"));
            }
            band.rows
                .iter()
                .find(|row| (row.b1 - b1).abs() < 1e-9)
                .ok_or_else(|| {
                    let choices = band
                        .rows
                        .iter()
                        .map(|row| trim(row.b1))
                        .collect::<Vec<_>>()
                        .join("、");
                    format!(
                        "磨外圆：d={} 属于「{}」档，该档可选 b1 = {}（收到 {}）",
                        trim(d),
                        band.label,
                        choices,
                        trim(b1)
                    )
                })
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 磨外圆：画法
// ══════════════════════════════════════════════════════════════════════════

/// 磨外圆图元（对照模板 `磨外圆_GB-T6403.5-2008.dxf` 逐图元）：
/// `1轮廓实线层` 10 条（上半 5 + 下半镜像 5）。模板里的砂轮细实线（`2细线层`）
/// 属标注性质，用户 2026-09-18 定案不画（覆盖「模板即权威」）。
fn build_grind_od(d: f64, row: &GrooveRow) -> GenPart {
    let rg = d / 2.0; // 磨出的外圆半径
    let rs = rg + row.r; // 台阶面高度（模板 53 = 50 + 3；等价于圆角切点再往上 h）
    let yb = rg - row.h; // 槽底
    let yc = yb + row.r; // 圆角圆心高度（= rg − h + r）
    let xe = row.b1 - row.h; // 平段右端 = 45° 起点（45° 段水平长 = h）
    let b1 = row.b1;
    let mut entities = Vec::with_capacity(11);

    // 上半侧：台阶面竖线 / 圆角弧 / 槽底 / 45° 斜线 / 右端竖线
    entities.push(line([0.0, 0.0], [0.0, rs], LAYER_MAIN));
    entities.push(arc([row.r, yc], row.r, 180.0, 270.0, LAYER_MAIN));
    entities.push(line([row.r, yb], [xe, yb], LAYER_MAIN));
    entities.push(line([xe, yb], [b1, rg], LAYER_MAIN));
    entities.push(line([b1, 0.0], [b1, rg], LAYER_MAIN));

    // 下半侧：对 y 镜像
    entities.push(line([0.0, 0.0], [0.0, -rs], LAYER_MAIN));
    entities.push(arc([row.r, -yc], row.r, 90.0, 180.0, LAYER_MAIN));
    entities.push(line([row.r, -yb], [xe, -yb], LAYER_MAIN));
    entities.push(line([xe, -yb], [b1, -rg], LAYER_MAIN));
    entities.push(line([b1, 0.0], [b1, -rg], LAYER_MAIN));

    let bbox = entity_bbox(&entities);
    GenPart {
        entities,
        meta: PartMeta {
            code: "GB/T 6403.5-2008".into(),
            name: "砂轮越程槽 磨外圆".into(),
            spec: format!("d{} b1 {}", trim(d), trim(row.b1)),
            material: String::new(),
            weight: String::new(),
        },
        bbox,
    }
}

/// 磨外圆的**槽体**图元（轴生成器等复用；与 `build_grind_od` 同一套画法）。
///
/// 与 `build_grind_od` 的差别：只去掉**独立要素图自己的两条闭合线**
/// （台阶面竖线 `x=0`、右端竖线 `x=b1`）——放进轴里那两条会变成内部线；
/// 保留上半/下半的 R 圆角、槽底、45° 斜坡，落层与原来一致。
/// （砂轮细线已在 [`build_grind_od`] 里按用户定案去掉，这里自然也没有。）
///
/// 局部坐标与 `build_grind_od` 完全一致：台阶面 = `x=0`、槽向 `+x` 展开、
/// 外圆 `y = ±d/2`；返回 `(图元, 数据行)`，图元顺序 = 上圆角/槽底/斜坡、
/// 下圆角/槽底/斜坡。
///
/// **不改 `build_grind_od` 的输出**（独立要素预览/DXF 不受影响）。
pub fn groove_entities(
    d: f64,
    b1: Option<f64>,
) -> Result<(Vec<EntityType>, &'static GrooveRow), String> {
    let row = row_for(d, b1)?;
    let part = build_grind_od(d, row);
    // build_grind_od 的固定顺序：上（台阶面/圆角/槽底/斜坡/右端）→ 下（同构）；
    // 丢弃 0/5（台阶面）与 4/9（右端闭合竖线）。砂轮细线已按用户定案去掉。
    let keep = [1usize, 2, 3, 6, 7, 8];
    let entities = keep
        .iter()
        .map(|&index| part.entities[index].clone())
        .collect();
    Ok((entities, row))
}

/// 槽体 R 圆角与台阶面（端面）的相切点半径：`d/2 + r − h`。
/// 轴生成器用它作为端面线在槽一侧的下端。
pub fn fillet_tangent_radius(d: f64, row: &GrooveRow) -> f64 {
    d / 2.0 + row.r - row.h
}

/// 磨外圆的要素定义（登记到 `ELEMENTS`）。
pub struct GrindOd;

/// 单例（`ELEMENTS` 里的引用）。
pub static GRIND_OD: GrindOd = GrindOd;

impl DetailElement for GrindOd {
    fn family(&self) -> &'static str {
        FAMILY_GRIND_OD
    }

    fn name(&self) -> &'static str {
        "磨外圆"
    }

    fn code(&self) -> &'static str {
        "GB/T 6403.5-2008"
    }

    fn views(&self) -> &'static [&'static str] {
        &["main"]
    }

    fn base_hint(&self) -> &'static str {
        "基点 = 台阶面与轴线交点（轴线为 x 轴；d = 磨出的外圆直径）"
    }

    fn generate(&self, d: f64, b1: Option<f64>, _view: &str) -> Result<GenPart, String> {
        let row = row_for(d, b1)?;
        Ok(build_grind_od(d, row))
    }

    fn catalog_extra(&self) -> serde_json::Value {
        serde_json::json!({
            "tree_dir": "结构要素/砂轮越程槽",
            "d_label": "轴径 d（mm，自由输入）",
            "source": "磨外圆_GB-T6403.png + 磨外圆_GB-T6403.5-2008.dxf",
            "bands": GROOVE_BANDS
                .iter()
                .map(|band| serde_json::json!({
                    "label": band.label,
                    "lo": band.lo, "lo_incl": band.lo_incl,
                    "hi": band.hi, "hi_incl": band.hi_incl,
                    "rows": band.rows.iter().map(|row| serde_json::json!({
                        "b1": row.b1, "h": row.h, "r": row.r,
                    })).collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        })
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 外螺纹退刀槽：数据表（GB/T 3-1997 表 2）
// ══════════════════════════════════════════════════════════════════════════

/// 族 id（CLI / GUI / 树 / xdata 用）。
pub const FAMILY_THREAD_RELIEF: &str = "detail_thread_relief";

/// 斜壁标称角（°，与**轴向**夹角）。GB/T 3-1997 表 2 注 3：
/// 一般 30°，也允许 45° 或其他角度；本要素把 alpha 当**最小允许角**用。
pub const DEFAULT_ALPHA_DEG: f64 = 30.0;

/// 斜壁角的**取整容差**（°）。
///
/// 标准正文要求「过渡角 α 不应小于 30°」；表 2 的 g2(max)/g1(min) 是各自圆整的
/// 极限值，拿它们反算的角最小 28.30°（P=0.45）、最大 33.69°（P=0.35），与标称
/// 30° 最大差 1.70°。校验式取 `θ + 2° ≥ alpha`：默认 alpha=30° 时 23 行全部合法；
/// 覆盖值把斜壁做平（θ < 28°）时报错。
pub const WALL_ANGLE_TOL_DEG: f64 = 2.0;

/// 示意螺纹段长度（mm）：图 2 里螺纹是继续向右延伸的，本要素固定画 5mm
/// 示意段（不参与尺寸链；报告里已注明）。
pub const THREAD_TAIL: f64 = 5.0;

/// 表 2 的一行；**单键 = 螺距 P**（不依赖螺纹直径）。单位 mm。表头：
/// `螺距 P | g2 (max) | g1 (min) | dg | r≈`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ThreadReliefRow {
    /// 螺距 P。
    pub p: f64,
    /// 退刀槽总宽 g2（台肩面 → 斜壁与螺纹大径 d 的交点）；表头标 **max**。
    pub g2: f64,
    /// 槽底平段终点 g1（台肩面 → 斜壁起点，含圆角段）；表头标 **min**。
    pub g1: f64,
    /// 直径减量 Δ：`dg = d − Δ`（表 2 的「dg」列印的是 `d−Δ`）。
    pub dg_reduction: f64,
    /// 槽底圆角半径 r（表头标 ≈）。
    pub r: f64,
}

/// **GB/T 3-1997 表 2（外螺纹退刀槽）数据唯一来源**（改表只改这里）。
///
/// 出处：GB/T 3-1997《普通螺纹收尾、肩距、退刀槽和倒角》（等同 ISO 3508:1976 /
/// ISO 4755:1977），表 2；数值抄自 <https://www.164580.com/data/detail_148.html>
/// （用户 2026-09-19 核对与真标准一致）。单位 mm；23 行（P=0.25…6，**表 2 无 P=0.2**）。
///
/// 表头与正文注：`g2` 标 **max**、`g1` 标 **min**、`r≈`；正文「过渡角（α）不应小于 30°」；
/// `dg` 公差 h13（d>3）/ h12（d≤3）。→ 本要素把 `alpha` 当**最小允许角**用。
///
/// **斜壁角自检的现实**：用户给的恒等式 `g2 ≈ g1 + ((d−dg)/2)/tan30°` 在
/// g1/g2 同时按公式取整时精确；表 2 的 g1/g2 是各自独立圆整的**上下限值**，
/// 取表中 g2(max)/g1(min) 反算的角最小 **28.30°**（P=0.45，比标称 30° 低 1.70°）、
/// 最大 **33.69°**（P=0.35）→ 全表最大偏差 **0.188mm**（P=6），只有 11/23 行落在
/// 0.02 以内。画法按表值 `(g1, dg/2) → (g2, d/2)` 连直线（**不硬拧 30°**），
/// 自检测试因此按表值实际精度 0.2mm 立断言，斜壁角按 `alpha − 2°` 校验
/// （标准正文的 30° 下限在生产值上成立；表列极限值是圆整后的极值）。
pub(crate) const THREAD_RELIEF_ROWS: &[ThreadReliefRow] = &[
    ThreadReliefRow { p: 0.25, g2: 0.75, g1: 0.4, dg_reduction: 0.4, r: 0.12 },
    ThreadReliefRow { p: 0.3, g2: 0.9, g1: 0.5, dg_reduction: 0.5, r: 0.16 },
    ThreadReliefRow { p: 0.35, g2: 1.05, g1: 0.6, dg_reduction: 0.6, r: 0.16 },
    ThreadReliefRow { p: 0.4, g2: 1.2, g1: 0.6, dg_reduction: 0.7, r: 0.2 },
    ThreadReliefRow { p: 0.45, g2: 1.35, g1: 0.7, dg_reduction: 0.7, r: 0.2 },
    ThreadReliefRow { p: 0.5, g2: 1.5, g1: 0.8, dg_reduction: 0.8, r: 0.2 },
    ThreadReliefRow { p: 0.6, g2: 1.8, g1: 0.9, dg_reduction: 1.0, r: 0.4 },
    ThreadReliefRow { p: 0.7, g2: 2.1, g1: 1.1, dg_reduction: 1.1, r: 0.4 },
    ThreadReliefRow { p: 0.75, g2: 2.25, g1: 1.2, dg_reduction: 1.2, r: 0.4 },
    ThreadReliefRow { p: 0.8, g2: 2.4, g1: 1.3, dg_reduction: 1.3, r: 0.4 },
    ThreadReliefRow { p: 1.0, g2: 3.0, g1: 1.6, dg_reduction: 1.6, r: 0.6 },
    ThreadReliefRow { p: 1.25, g2: 3.75, g1: 2.0, dg_reduction: 2.0, r: 0.6 },
    ThreadReliefRow { p: 1.5, g2: 4.5, g1: 2.5, dg_reduction: 2.3, r: 0.8 },
    ThreadReliefRow { p: 1.75, g2: 5.25, g1: 3.0, dg_reduction: 2.6, r: 1.0 },
    ThreadReliefRow { p: 2.0, g2: 6.0, g1: 3.4, dg_reduction: 3.0, r: 1.0 },
    ThreadReliefRow { p: 2.5, g2: 7.5, g1: 4.4, dg_reduction: 3.6, r: 1.2 },
    ThreadReliefRow { p: 3.0, g2: 9.0, g1: 5.2, dg_reduction: 4.4, r: 1.6 },
    ThreadReliefRow { p: 3.5, g2: 10.5, g1: 6.2, dg_reduction: 5.0, r: 1.6 },
    ThreadReliefRow { p: 4.0, g2: 12.0, g1: 7.0, dg_reduction: 5.7, r: 2.0 },
    ThreadReliefRow { p: 4.5, g2: 13.5, g1: 8.0, dg_reduction: 6.4, r: 2.5 },
    ThreadReliefRow { p: 5.0, g2: 15.0, g1: 9.0, dg_reduction: 7.0, r: 2.5 },
    ThreadReliefRow { p: 5.5, g2: 17.5, g1: 11.0, dg_reduction: 7.7, r: 3.2 },
    ThreadReliefRow { p: 6.0, g2: 18.0, g1: 11.0, dg_reduction: 8.3, r: 3.2 },
];

impl ThreadReliefRow {
    /// 表行 + 螺纹大径 `d` → 实际退刀槽尺寸（`M` 段螺纹收尾无覆盖值时用）。
    /// 与 [`relief_dims`] 同一口径：`dg = d − dg_reduction`，斜壁实际角按表值反算
    /// （不硬拧 30°）。
    pub(crate) fn dims(&self, d: f64) -> ReliefDims {
        let dg = d - self.dg_reduction;
        let half = (d - dg) / 2.0;
        ReliefDims {
            p: self.p,
            d,
            dg,
            g1: self.g1,
            g2: self.g2,
            r: self.r,
            wall_angle_deg: (half / (self.g2 - self.g1)).atan().to_degrees(),
        }
    }
}

/// 螺距 P → 表 2 行（1e-9 容差；表里没有就报错并列出可用 P，**不插值/不外推**）。
pub(crate) fn thread_relief_row(p: f64) -> Result<&'static ThreadReliefRow, String> {
    if !p.is_finite() || p <= 0.0 {
        return Err(format!("外螺纹退刀槽：螺距 P 必须是正数（收到 {p}）"));
    }
    THREAD_RELIEF_ROWS
        .iter()
        .find(|row| (row.p - p).abs() < 1e-9)
        .ok_or_else(|| {
            let choices = THREAD_RELIEF_ROWS
                .iter()
                .map(|row| trim(row.p))
                .collect::<Vec<_>>()
                .join("、");
            format!(
                "外螺纹退刀槽：表 2 没有螺距 P={}（可用 P = {}；表 2 从 0.25 起，无 0.2）",
                trim(p),
                choices
            )
        })
}

/// 退刀槽实际尺寸（表值 + 可选覆盖），并附斜壁实际角供校验/台账。
///
/// `p = 0` 是**段级显式尺寸**的占位（段级 RL 可以不给螺距，见
/// [`relief_dims_explicit`]）；表 2 路径与独立要素路径的 `p` 都是真螺距。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ReliefDims {
    pub p: f64,
    pub d: f64,
    /// 退刀槽直径（表值为 `d − 减量`）。
    pub dg: f64,
    pub g1: f64,
    pub g2: f64,
    pub r: f64,
    /// 斜壁实际角（°，与轴线夹角；表值反算 ≈28.30°…33.69°）。
    pub wall_angle_deg: f64,
}

/// 由 `d` + 参数解出退刀槽尺寸并做几何校验（表 2 路径，`P` 必给）。
///
/// 参数：`P` 必给（表 2 单键）；`g1/g2/dg/r` 可选覆盖（给则覆盖表值）；
/// `alpha` 可选（默认 30°，必须 ≥30°，实际斜壁角 `θ + WALL_ANGLE_TOL_DEG ≥ alpha`）。
/// 其它键（如磨外圆的 `b1`）一律报错——不让笔误静默出图。
pub(crate) fn relief_dims(d: f64, params: &DetailParams) -> Result<ReliefDims, String> {
    const KNOWN: [&str; 6] = ["p", "g1", "g2", "dg", "r", "alpha"];
    let unknown: Vec<&str> = params
        .keys()
        .into_iter()
        .filter(|key| !KNOWN.contains(key))
        .collect();
    if !unknown.is_empty() {
        return Err(format!(
            "外螺纹退刀槽：不认识参数 {}（本族支持 P/g1/g2/dg/r/alpha）",
            unknown.join("、")
        ));
    }
    if !d.is_finite() || d <= 0.0 {
        return Err(format!("外螺纹退刀槽：d 必须是正数（收到 {d}）"));
    }
    let p = params.get("P").ok_or_else(|| {
        "外螺纹退刀槽：缺少螺距 P（写法 `XL detail_thread_relief <d> P <P>`）".to_string()
    })?;
    let row = thread_relief_row(p)?;
    let dg = params.get("dg").unwrap_or(d - row.dg_reduction);
    let g1 = params.get("g1").unwrap_or(row.g1);
    let g2 = params.get("g2").unwrap_or(row.g2);
    let r = params.get("r").unwrap_or(row.r);
    let alpha = params.get("alpha").unwrap_or(DEFAULT_ALPHA_DEG);
    check_relief("外螺纹退刀槽", "螺纹大径", d, p, dg, g1, g2, r, alpha)
}

/// **段级退刀槽**（轴生成器 `RL@L/@R` 的显式尺寸路径）：不给 P 时 g1/g2/dg/r 全给。
///
/// `dg` 是**绝对直径**（表 2 的 `dg` 列是 `d−Δ`，口径一致）；校验与表 2 路径共用
/// [`check_relief`]，所以 `dg < d`、`g2 > g1 > r`、圆角不超大径、斜壁角下限都照查。
pub(crate) fn relief_dims_explicit(
    d: f64,
    g1: f64,
    g2: f64,
    dg: f64,
    r: f64,
) -> Result<ReliefDims, String> {
    // p=0 = 段级显式尺寸占位（不再有螺距可查；调用方不需要 p）。
    check_relief("段级退刀槽", "本段大径", d, 0.0, dg, g1, g2, r, DEFAULT_ALPHA_DEG)
}

/// 表 2 / 显式覆盖共用的几何校验：尺寸、圆角、槽深、斜壁角。
/// `what` 只用于报错前缀（表 2 路径 =「外螺纹退刀槽」，段级 =「段级退刀槽」）；
/// `big` = 大径的称呼（表 2 路径 =「螺纹大径」，段级 =「本段大径」）。
fn check_relief(
    what: &str,
    big: &str,
    d: f64,
    p: f64,
    dg: f64,
    g1: f64,
    g2: f64,
    r: f64,
    alpha: f64,
) -> Result<ReliefDims, String> {
    if !d.is_finite() || d <= 0.0 {
        return Err(format!("{what}：d 必须是正数（收到 {d}）"));
    }
    for (name, value) in [("dg", dg), ("g1", g1), ("g2", g2), ("r", r)] {
        if !value.is_finite() || value <= 0.0 {
            return Err(format!("{what}：{name} 必须是正数（收到 {value}）"));
        }
    }
    if !alpha.is_finite() {
        return Err(format!("{what}：alpha 不是有限数"));
    }
    if alpha < DEFAULT_ALPHA_DEG {
        return Err(format!(
            "{what}：alpha 必须 ≥ {}°（收到 {}°）—— 斜壁不能比 30° 更平",
            trim(DEFAULT_ALPHA_DEG),
            trim(alpha)
        ));
    }
    if dg >= d {
        return Err(format!(
            "{what}：dg（{}）必须小于{big} d（{}）",
            trim(dg),
            trim(d)
        ));
    }
    if g2 <= g1 {
        return Err(format!(
            "{what}：g2（{}）必须大于 g1（{}）",
            trim(g2),
            trim(g1)
        ));
    }
    if g1 <= r {
        return Err(format!(
            "{what}：g1（{}）必须大于圆角 r（{}），否则圆角与斜壁打架",
            trim(g1),
            trim(r)
        ));
    }
    let half = (d - dg) / 2.0;
    if r > half + 1e-9 {
        return Err(format!(
            "{what}：圆角 r（{}）超过槽深 (d−dg)/2（{}），圆角伸到{big}之外",
            trim(r),
            trim(half)
        ));
    }
    let wall_angle_deg = (half / (g2 - g1)).atan().to_degrees();
    if wall_angle_deg + WALL_ANGLE_TOL_DEG < alpha {
        return Err(format!(
            "{what}：斜壁实际角 {:.2}° 小于 alpha {:.2}°（含表 2 取整容差 {:.0}°）—— 检查 g1/g2/dg 覆盖值",
            wall_angle_deg, alpha, WALL_ANGLE_TOL_DEG
        ));
    }
    Ok(ReliefDims {
        p,
        d,
        dg,
        g1,
        g2,
        r,
        wall_angle_deg,
    })
}

// ══════════════════════════════════════════════════════════════════════════
// 外螺纹退刀槽：画法（图 2）
// ══════════════════════════════════════════════════════════════════════════

/// 退刀槽图元（照图 2 口径）：上半 5 条 + 下半镜像 5 条，全部 `1轮廓实线层`。
///
/// 锚点 = 台肩面与轴线交点 `(0,0)`，轴向向右为 `+x`、`y` 为半径：
/// 台肩面竖线 `x=0`（`dg/2+r` → `d/2+r`）、R=r 圆角（心 `(r, dg/2+r)`，180°→270°）、
/// 槽底 `y=dg/2`（`x=r…g1`）、30° 斜壁 `(g1,dg/2)→(g2,d/2)`、螺纹外圆示意段
/// `y=d/2`（`x=g2…g2+THREAD_TAIL`）。
///
/// **台肩面只画到 `d/2+r`**（比螺纹大径高一个 r），与磨外圆要素的「台阶面到
/// d/2+r」同口径；**螺纹示意段固定 5mm**（图 2 里螺纹继续延伸，本要素不闭环）。
fn build_thread_relief(dims: &ReliefDims) -> GenPart {
    let entities = build_thread_relief_entities(dims);
    let bbox = entity_bbox(&entities);
    GenPart {
        entities,
        meta: PartMeta {
            code: "GB/T 3-1997".into(),
            name: "外螺纹退刀槽".into(),
            spec: format!(
                "d{} P{} g1 {} g2 {}",
                trim(dims.d),
                trim(dims.p),
                trim(dims.g1),
                trim(dims.g2)
            ),
            material: String::new(),
            weight: String::new(),
        },
        bbox,
    }
}

/// 独立要素 `build_thread_relief` 的 10 条图元（顺序不变）：
/// 上（台肩面/圆角/槽底/斜壁/螺纹示意段）→ 下（同构）。
fn build_thread_relief_entities(dims: &ReliefDims) -> Vec<EntityType> {
    let yb = dims.dg / 2.0; // 槽底半径
    let yt = dims.d / 2.0; // 螺纹大径半径
    let ys = yb + dims.r; // 台肩面下端（圆角切点）
    let ytop = yt + dims.r; // 台肩面上端
    let mut entities = Vec::with_capacity(10);

    // 上半侧：台肩面 / R=r 圆角 / 槽底 / 斜壁 / 螺纹外圆示意段
    entities.push(line([0.0, ys], [0.0, ytop], LAYER_MAIN));
    entities.push(arc([dims.r, ys], dims.r, 180.0, 270.0, LAYER_MAIN));
    entities.push(line([dims.r, yb], [dims.g1, yb], LAYER_MAIN));
    entities.push(line([dims.g1, yb], [dims.g2, yt], LAYER_MAIN));
    entities.push(line(
        [dims.g2, yt],
        [dims.g2 + THREAD_TAIL, yt],
        LAYER_MAIN,
    ));

    // 下半侧：对 y 镜像
    entities.push(line([0.0, -ys], [0.0, -ytop], LAYER_MAIN));
    entities.push(arc([dims.r, -ys], dims.r, 90.0, 180.0, LAYER_MAIN));
    entities.push(line([dims.r, -yb], [dims.g1, -yb], LAYER_MAIN));
    entities.push(line([dims.g1, -yb], [dims.g2, -yt], LAYER_MAIN));
    entities.push(line(
        [dims.g2, -yt],
        [dims.g2 + THREAD_TAIL, -yt],
        LAYER_MAIN,
    ));
    entities
}

/// 退刀槽**槽体**图元（轴生成器等复用；与 `build_thread_relief` 同一套画法/落层）。
///
/// 只去掉独立要素图自己的两条闭合线（台肩面竖线 `x=0`、螺纹示意段）——放进轴里
/// 那些线由轴生成器的端面/轮廓线负责；保留上半/下半的 R 圆角、槽底、斜壁。
/// 局部坐标与 `build_thread_relief` 完全一致：台肩面 = `x=0`、槽向 `+x` 展开、
/// 大径 `y = ±d/2`；图元顺序 = 上圆角/槽底/斜壁、下圆角/槽底/斜壁。
///
/// **不改 `build_thread_relief` 的输出**（独立要素预览/DXF 不受影响）。
pub(crate) fn relief_groove_entities(dims: &ReliefDims) -> Vec<EntityType> {
    let part = build_thread_relief_entities(dims);
    // 0/5 = 台肩面竖线；4/9 = 螺纹示意段（轴里由轮廓/端面线负责）。
    [1usize, 2, 3, 6, 7, 8]
        .iter()
        .map(|&index| part[index].clone())
        .collect()
}

/// 外螺纹退刀槽的要素定义（登记到 `ELEMENTS`）。
pub struct ThreadRelief;

/// 单例（`ELEMENTS` 里的引用）。
pub static THREAD_RELIEF: ThreadRelief = ThreadRelief;

impl DetailElement for ThreadRelief {
    fn family(&self) -> &'static str {
        FAMILY_THREAD_RELIEF
    }

    fn name(&self) -> &'static str {
        "外螺纹退刀槽"
    }

    fn code(&self) -> &'static str {
        "GB/T 3-1997"
    }

    fn views(&self) -> &'static [&'static str] {
        &["main"]
    }

    fn base_hint(&self) -> &'static str {
        "基点 = 台肩面与轴线交点（轴线为 x 轴；d = 螺纹公称直径）"
    }

    /// 历史 `b1` 槽位表达不了必给的 P —— 明确报错指路（不让默认值静默出图）。
    fn generate(&self, _d: f64, _b1: Option<f64>, _view: &str) -> Result<GenPart, String> {
        Err("外螺纹退刀槽需要螺距 P：请用 `XL detail_thread_relief <d> P <P> [g1 值 …]`"
            .to_string())
    }

    fn generate_params(
        &self,
        d: f64,
        params: &DetailParams,
        _view: &str,
    ) -> Result<GenPart, String> {
        Ok(build_thread_relief(&relief_dims(d, params)?))
    }

    fn catalog_extra(&self) -> serde_json::Value {
        serde_json::json!({
            "tree_dir": "结构要素/退刀槽",
            "d_label": "螺纹公称直径 d（mm，自由输入）",
            "default_d": 20,
            "source": "GB/T 3-1997（ISO 3508:1976 / ISO 4755:1977）表 2；抄自 164580.com/data/detail_148.html（用户 2026-09-19 核对）",
            "thread_tail": THREAD_TAIL,
            // GUI 自由参数面板：id/必给/顺序全由这份数据驱动（磨外圆没有 inputs，仍走 b1）
            "inputs": [
                { "key": "P", "label": "螺距 P（mm，必给；表 2 单键）", "required": true, "placeholder": "例如 1.5" },
                { "key": "g1", "label": "g1 覆盖（mm，留空 = 表值）" },
                { "key": "g2", "label": "g2 覆盖（mm，留空 = 表值）" },
                { "key": "dg", "label": "dg 覆盖（mm，留空 = d − 表值减量）" },
                { "key": "r", "label": "r 覆盖（mm，留空 = 表值）" },
                { "key": "alpha", "label": "斜壁最小角 α（°，默认 30，必须 ≥30）" },
            ],
            "pitches": THREAD_RELIEF_ROWS
                .iter()
                .map(|row| serde_json::json!({
                    "P": row.p,
                    "g2": row.g2,
                    "g1": row.g1,
                    "dg_reduction": row.dg_reduction,
                    "r": row.r,
                }))
                .collect::<Vec<_>>(),
            "sample": { "d": 20, "P": 1.5 },
        })
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 矩形花键（GB/T 1144 规格代号；几何在 `spline.rs`，独立要素与轴段特征共用）
// ══════════════════════════════════════════════════════════════════════════

/// 族 id。
pub const FAMILY_SPLINE_RECT: &str = "detail_spline_rect";

/// 把主参数 `d`（小径）+ 参数解成 [`crate::spline::RectSpline`] 与满齿段长。
///
/// * `params.spec()` = 规格代号（`6x23x26x6`）优先；同时给了 `d` 必须一致；
/// * 没 spec 时用数字参数 `n` / `big`（或 `D`）/ `b`，`de` 可选（查 GB/T 10952 表）；
/// * `len`（或 `l`）= 满齿段长 L；正视图不需要。
fn resolve_rect_spline(
    d: f64,
    params: &DetailParams,
) -> Result<(crate::spline::RectSpline, Option<f64>), String> {
    const KNOWN: [&str; 7] = ["n", "d", "big", "b", "de", "len", "l"];
    let unknown: Vec<&str> = params
        .keys()
        .into_iter()
        .filter(|key| !KNOWN.contains(key))
        .collect();
    if !unknown.is_empty() {
        return Err(format!(
            "矩形花键：不认识参数 {}（本族支持 规格代号 spec、L/len、de、N、D、B）",
            unknown.join("、")
        ));
    }
    let len = params.get("len").or_else(|| params.get("l"));
    let de = params.get("de");
    if let Some(spec) = params.spec() {
        let spline = crate::spline::RectSpline::from_code(spec, de, len.unwrap_or(0.0))?;
        if d.is_finite() && d > 0.0 && (d - spline.d).abs() > 1e-9 {
            return Err(format!(
                "矩形花键：规格 {spec} 的小径 d={} 与参数 d={} 不一致",
                trim(spline.d),
                trim(d)
            ));
        }
        if let Some(n) = params.get("n") {
            if n.fract().abs() > 1e-9 || n as u32 != spline.n {
                return Err(format!(
                    "矩形花键：规格 {spec} 的齿数 N={} 与参数 N={} 不一致",
                    spline.n,
                    trim(n)
                ));
            }
        }
        if let Some(big) = params.get("big").or_else(|| params.get("D")) {
            if (big - spline.big).abs() > 1e-9 {
                return Err(format!(
                    "矩形花键：规格 {spec} 的大径 D={} 与参数 D={} 不一致",
                    trim(spline.big),
                    trim(big)
                ));
            }
        }
        if let Some(b) = params.get("b") {
            if (b - spline.b).abs() > 1e-9 {
                return Err(format!(
                    "矩形花键：规格 {spec} 的键宽 B={} 与参数 B={} 不一致",
                    trim(spline.b),
                    trim(b)
                ));
            }
        }
        return Ok((spline, len));
    }
    let n_value = params.get("n").ok_or_else(|| {
        format!("矩形花键：缺齿数 N（写法 `spec 6x23x26x6` 或数字参数 `N6 D26 B6 d23`）")
    })?;
    if n_value.fract().abs() > 1e-9 || !(3.0..=100.0).contains(&n_value) {
        return Err(format!("矩形花键：齿数 N={} 必须取 3..=100 的整数", trim(n_value)));
    }
    let big = params
        .get("big")
        .or_else(|| params.get("D"))
        .ok_or_else(|| "矩形花键：缺大径 D（数字参数写法 `D26`）".to_string())?;
    let b = params
        .get("b")
        .ok_or_else(|| "矩形花键：缺键宽 B（数字参数写法 `B6`）".to_string())?;
    if !d.is_finite() || d <= 0.0 {
        return Err(format!("矩形花键：小径 d={} 必须是正数", trim(d)));
    }
    let de = match de {
        Some(de) => de,
        None => crate::spline::lookup_de(n_value as u32, d, big, b).ok_or_else(|| {
            "矩形花键：该 N/d/D/B 不在 GB/T 10952-2005 表 1/表 2 里，de 查不到 —— 请给 de".to_string()
        })?,
    };
    Ok((
        crate::spline::RectSpline::new(n_value as u32, d, big, b, de, len.unwrap_or(0.0))?,
        len,
    ))
}

/// 矩形花键的要素定义（登记到 `ELEMENTS`）。
pub struct SplineRect;

/// 单例（`ELEMENTS` 里的引用）。
pub static SPLINE_RECT: SplineRect = SplineRect;

impl DetailElement for SplineRect {
    fn family(&self) -> &'static str {
        FAMILY_SPLINE_RECT
    }

    fn name(&self) -> &'static str {
        "矩形花键"
    }

    fn code(&self) -> &'static str {
        crate::spline::CODE
    }

    fn views(&self) -> &'static [&'static str] {
        &["front", "side", "section"]
    }

    fn view_labels(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("front", "正视图（端视图）"),
            ("side", "常规侧视图"),
            ("section", "侧剖视图"),
        ]
    }

    fn base_hint(&self) -> &'static str {
        "基点 = 左端面与轴线交点（轴线为 x 轴；正视图 = 齿形中心）"
    }

    /// 历史 `b1` 槽位表达不了规格代号 —— 明确报错指路。
    fn generate(&self, _d: f64, _b1: Option<f64>, _view: &str) -> Result<GenPart, String> {
        Err("矩形花键请给规格代号：`XL detail_spline_rect 6x23x26x6 L30 [de 63] [view side|section|front]`"
            .to_string())
    }

    fn generate_params(
        &self,
        d: f64,
        params: &DetailParams,
        view: &str,
    ) -> Result<GenPart, String> {
        let (spline, len) = resolve_rect_spline(d, params)?;
        let (entities, spec_text) = match view {
            "front" => (spline.front_view(), spline.code()),
            "side" => {
                let len = len.ok_or_else(|| {
                    "矩形花键：常规侧视图需要 L（满齿段长，例 `L30` / `&len=30`）".to_string()
                })?;
                (spline.side_view(len), format!("{} L{}", spline.code(), trim(len)))
            }
            "section" => {
                let len = len.ok_or_else(|| {
                    "矩形花键：侧剖视图需要 L（满齿段长，例 `L30` / `&len=30`）".to_string()
                })?;
                (spline.section_view(len), format!("{} L{}", spline.code(), trim(len)))
            }
            other => return Err(format!("矩形花键：视图 {other} 尚未实现")),
        };
        let bbox = entity_bbox(&entities);
        Ok(GenPart {
            entities,
            meta: PartMeta {
                code: crate::spline::CODE.into(),
                name: format!("矩形花键（{}）", self
                    .view_labels()
                    .iter()
                    .find(|(key, _)| *key == view)
                    .map(|(_, label)| *label)
                    .unwrap_or(view)),
                spec: spec_text,
                material: String::new(),
                weight: String::new(),
            },
            bbox,
        })
    }

    fn parse_spec_token(&self, token: &str) -> Option<(f64, DetailParams)> {
        let (_, d, _, _) = crate::spline::parse_code(token).ok()?;
        let mut params = DetailParams::new();
        params.set_spec(token);
        Some((d, params))
    }

    fn catalog_extra(&self) -> serde_json::Value {
        serde_json::json!({
            "tree_dir": "结构要素/花键",
            "d_label": "小径 d（mm；由规格代号派生，自定义规格时才自由输入）",
            "default_d": 23,
            "spec_label": "规格代号 N×d×D×B（下拉选择或自定义输入）",
            "source": format!("{}（规格代号自带 N/d/D/B）；{} 表1/表2（de）；模板 矩形花键.dxf 逐图元反解", crate::spline::CODE, crate::spline::HOB_CODE),
            "inputs": [
                { "key": "len", "label": "L 满齿段长（mm，侧视/剖视必给）", "required": true, "placeholder": "例如 30" },
                { "key": "de", "label": "de 滚刀外径覆盖（mm，留空 = 按规格查表）", "placeholder": "例如 63" },
            ],
            "specs": crate::spline::SPLINE_SPECS
                .iter()
                .map(|s| serde_json::json!({
                    "code": s.code,
                    "label": format!("{}{}", s.series, s.code),
                    "n": s.n,
                    "d": s.d_minor,
                    "D": s.d_major,
                    "B": s.b,
                    "de": s.de,
                    "series": s.series,
                }))
                .collect::<Vec<_>>(),
            "sample": { "d": 23, "n": 6, "big": 26, "b": 6, "len": 30, "de": 63 },
        })
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 表 1（收尾 / 肩距）——下一轮「局部螺纹 + 收尾」用，先以数据存好
// ══════════════════════════════════════════════════════════════════════════

/// GB/T 3-1997 表 1（外螺纹收尾与肩距）一行；单位 mm。
///
/// 出处：GB/T 3-1997 表 1（ISO 3508/ISO 4755），抄自
/// <https://www.164580.com/data/detail_148.html>（用户 2026-09-19 核对）。
///
/// **订正记录**：网页 P=1.75 行的 `x一般` 印成 **1.3**，用户核对真标准为 **4.3**
/// （与参考比例 2.5P=4.375 及前后行 3.8 / 5.0 的序列一致）——本表已用 4.3。
///
/// 本表与表 2 都只在本文件维护（**表归一**：轴生成器 `shaft.rs` 引用这里，
/// 不再自带副本）；画法在轴生成器的局部螺纹里。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RunoutRow {
    /// 螺距 P（表 1 从 0.2 起，比表 2 多一行 0.2）。
    pub p: f64,
    /// 收尾 x（一般）≈2.5P。
    pub x_normal: f64,
    /// 收尾 x（短）≈1.25P。
    pub x_short: f64,
    /// 肩距 a（一般）≈3P。
    pub a_normal: f64,
    /// 肩距 a（长）= 4P。
    pub a_long: f64,
    /// 肩距 a（短）= 2P。
    pub a_short: f64,
}

/// GB/T 3-1997 表 1 数据（24 行，P=0.2…6）；**表归一后的唯一来源**。
pub(crate) const RUNOUT_ROWS: &[RunoutRow] = &[
    RunoutRow { p: 0.2, x_normal: 0.5, x_short: 0.25, a_normal: 0.6, a_long: 0.8, a_short: 0.4 },
    RunoutRow { p: 0.25, x_normal: 0.6, x_short: 0.3, a_normal: 0.75, a_long: 1.0, a_short: 0.5 },
    RunoutRow { p: 0.3, x_normal: 0.75, x_short: 0.4, a_normal: 0.9, a_long: 1.2, a_short: 0.6 },
    RunoutRow { p: 0.35, x_normal: 0.9, x_short: 0.45, a_normal: 1.05, a_long: 1.4, a_short: 0.7 },
    RunoutRow { p: 0.4, x_normal: 1.0, x_short: 0.5, a_normal: 1.2, a_long: 1.6, a_short: 0.8 },
    RunoutRow { p: 0.45, x_normal: 1.1, x_short: 0.6, a_normal: 1.35, a_long: 1.8, a_short: 0.9 },
    RunoutRow { p: 0.5, x_normal: 1.25, x_short: 0.7, a_normal: 1.5, a_long: 2.0, a_short: 1.0 },
    RunoutRow { p: 0.6, x_normal: 1.5, x_short: 0.75, a_normal: 1.8, a_long: 2.4, a_short: 1.2 },
    RunoutRow { p: 0.7, x_normal: 1.75, x_short: 0.9, a_normal: 2.1, a_long: 2.8, a_short: 1.4 },
    RunoutRow { p: 0.75, x_normal: 1.9, x_short: 1.0, a_normal: 2.25, a_long: 3.0, a_short: 1.5 },
    RunoutRow { p: 0.8, x_normal: 2.0, x_short: 1.0, a_normal: 2.4, a_long: 3.2, a_short: 1.6 },
    RunoutRow { p: 1.0, x_normal: 2.5, x_short: 1.25, a_normal: 3.0, a_long: 4.0, a_short: 2.0 },
    RunoutRow { p: 1.25, x_normal: 3.2, x_short: 1.6, a_normal: 4.0, a_long: 5.0, a_short: 2.5 },
    RunoutRow { p: 1.5, x_normal: 3.8, x_short: 1.9, a_normal: 4.5, a_long: 6.0, a_short: 3.0 },
    // ↓ 网页此处 x一般 印 1.3，真标准 4.3（见 `RunoutRow` 文档注释的订正记录）
    RunoutRow { p: 1.75, x_normal: 4.3, x_short: 2.2, a_normal: 5.3, a_long: 7.0, a_short: 3.5 },
    RunoutRow { p: 2.0, x_normal: 5.0, x_short: 2.5, a_normal: 6.0, a_long: 8.0, a_short: 4.0 },
    RunoutRow { p: 2.5, x_normal: 6.3, x_short: 3.2, a_normal: 7.5, a_long: 10.0, a_short: 5.0 },
    RunoutRow { p: 3.0, x_normal: 7.5, x_short: 3.8, a_normal: 9.0, a_long: 12.0, a_short: 6.0 },
    RunoutRow { p: 3.5, x_normal: 9.0, x_short: 4.5, a_normal: 10.5, a_long: 14.0, a_short: 7.0 },
    RunoutRow { p: 4.0, x_normal: 10.0, x_short: 5.0, a_normal: 12.0, a_long: 16.0, a_short: 8.0 },
    RunoutRow { p: 4.5, x_normal: 11.0, x_short: 5.5, a_normal: 13.5, a_long: 18.0, a_short: 9.0 },
    RunoutRow { p: 5.0, x_normal: 12.5, x_short: 6.3, a_normal: 15.0, a_long: 20.0, a_short: 10.0 },
    RunoutRow { p: 5.5, x_normal: 14.0, x_short: 7.0, a_normal: 16.5, a_long: 22.0, a_short: 11.0 },
    RunoutRow { p: 6.0, x_normal: 15.0, x_short: 7.5, a_normal: 18.0, a_long: 24.0, a_short: 12.0 },
];

/// 螺距 P → 表 1 行（1e-9 容差；找不到返回 `None`，便于调用方自定报错口径）。
pub(crate) fn runout_row(p: f64) -> Option<&'static RunoutRow> {
    RUNOUT_ROWS.iter().find(|row| (row.p - p).abs() < 1e-9)
}

/// 螺距 P → 表 1 行，查不到报错并列出可用 P（轴生成器局部螺纹走这里）。
///
/// 与 `detail_thread_relief` 的 `thread_relief_row` 同口径（1e-9 容差、不插值、
/// 不外推）；表 1 从 P=0.2 起。
pub(crate) fn runout_row_checked(p: f64) -> Result<&'static RunoutRow, String> {
    if !p.is_finite() || p <= 0.0 {
        return Err(format!("GB/T 3 表 1：螺距 P 必须是正数（收到 {}）", trim(p)));
    }
    runout_row(p).ok_or_else(|| {
        let choices = RUNOUT_ROWS
            .iter()
            .map(|row| trim(row.p))
            .collect::<Vec<_>>()
            .join("、");
        format!(
            "GB/T 3 表 1 没有螺距 P={}（可用 P = {}；不插值、不外推）",
            trim(p),
            choices
        )
    })
}

// ══════════════════════════════════════════════════════════════════════════
// 测试
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn has_line(part: &GenPart, a: [f64; 2], b: [f64; 2]) -> bool {
        part.entities.iter().any(|e| match e {
            EntityType::Line(l) => {
                let (p, q) = ([l.start.x, l.start.y], [l.end.x, l.end.y]);
                (near(p[0], a[0]) && near(p[1], a[1]) && near(q[0], b[0]) && near(q[1], b[1]))
                    || (near(p[0], b[0]) && near(p[1], b[1]) && near(q[0], a[0]) && near(q[1], a[1]))
            }
            _ => false,
        })
    }

    fn has_arc(part: &GenPart, c: [f64; 2], r: f64, start_deg: f64, end_deg: f64) -> bool {
        part.entities.iter().any(|e| match e {
            EntityType::Arc(a) => {
                near(a.center.x, c[0])
                    && near(a.center.y, c[1])
                    && near(a.radius, r)
                    && near(a.start_angle.to_degrees(), start_deg)
                    && near(a.end_angle.to_degrees(), end_deg)
            }
            _ => false,
        })
    }

    /// 各 d 档查表：d=100 → 第 4 档最后一行（b1=10/h=1.2/r=3，与模板一致）。
    #[test]
    fn bands_and_default_rows() {
        for (d, label, b1, h, r) in [
            (0.5, "d ≤ 10", 1.6, 0.2, 0.5),
            (10.0, "d ≤ 10", 1.6, 0.2, 0.5),
            (10.5, "10 < d ≤ 50", 3.0, 0.4, 1.0),
            (50.0, "10 < d ≤ 50", 3.0, 0.4, 1.0),
            (50.5, "50 < d < 100", 5.0, 0.6, 1.6),
            (99.999, "50 < d < 100", 5.0, 0.6, 1.6),
            (100.0, "d ≥ 100", 10.0, 1.2, 3.0),
            (250.0, "d ≥ 100", 10.0, 1.2, 3.0),
        ] {
            let band = band_of(d).expect("应能查档");
            let row = row_for(d, None).expect("应能取默认行");
            assert_eq!(band.label, label, "d={d} 所属档");
            assert!(
                near(row.b1, b1) && near(row.h, h) && near(row.r, r),
                "d={d} 默认行 {row:?}"
            );
        }
    }

    /// d ≤ 0 / NaN / ∞ 报错（表覆盖 d>0 全范围，不外推）。
    #[test]
    fn invalid_d_rejected() {
        for d in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let err = row_for(d, None).expect_err("非法 d 应报错");
            assert!(err.contains("d"), "{err}");
        }
    }

    /// 显式 b1 覆盖：在 d 所在档内匹配。
    #[test]
    fn explicit_b1_override() {
        assert_eq!(row_for(100.0, Some(8.0)).unwrap().b1, 8.0);
        assert_eq!(row_for(100.0, Some(10.0)).unwrap().b1, 10.0);
        assert_eq!(row_for(25.0, Some(2.0)).unwrap().b1, 2.0);
        assert_eq!(row_for(10.0, Some(0.6)).unwrap().b1, 0.6);
        // 覆盖行自带 h/r（不是只换 b1）
        let row = row_for(100.0, Some(8.0)).unwrap();
        assert!(near(row.h, 0.8) && near(row.r, 2.0));
    }

    /// b1 不在该档：报错并列出该档可选 b1。
    #[test]
    fn b1_mismatch_lists_choices() {
        let err = row_for(100.0, Some(4.0)).expect_err("b1=4 不在 ≥100 档");
        assert!(err.contains("8") && err.contains("10"), "应列出可选 b1：{err}");
        let err = row_for(25.0, Some(10.0)).expect_err("b1=10 不在 10<d≤50 档");
        assert!(err.contains("2") && err.contains("3"), "应列出可选 b1：{err}");
        // 别的档有 b1=4，但当前档没有 → 仍须报错
        assert!(row_for(25.0, Some(4.0)).is_err());
    }

    /// 模板 d=100 档：图元数 / 落层 / 坐标与 DXF 逐图元一致（不再有砂轮细线）。
    #[test]
    fn template_d100_matches_dxf() {
        let part = generate(FAMILY_GRIND_OD, 100.0, None, "main").unwrap();
        assert_eq!(part.entities.len(), 10, "10 条轮廓；砂轮细线已按用户定案去掉");
        let main = part
            .entities
            .iter()
            .filter(|e| e.common().layer == LAYER_MAIN)
            .count();
        assert_eq!(main, 10, "落层：轮廓 → 1轮廓实线层（没有 2细线层）");
        assert!(part
            .entities
            .iter()
            .all(|e| e.common().layer == LAYER_MAIN));
        // 图层风格：ByLayer
        assert!(part
            .entities
            .iter()
            .all(|e| e.common().color == ocs_plugin_api::host::acadrust::types::Color::ByLayer));

        // 上半（模板反解值）
        assert!(has_line(&part, [0.0, 0.0], [0.0, 53.0]), "台阶面 x=0 → y=53");
        assert!(has_arc(&part, [3.0, 51.8], 3.0, 180.0, 270.0), "上圆角心 (3,51.8)");
        assert!(has_line(&part, [3.0, 48.8], [8.8, 48.8]), "槽底 y=48.8");
        assert!(has_line(&part, [8.8, 48.8], [10.0, 50.0]), "45° (8.8,48.8)→(10,50)");
        assert!(has_line(&part, [10.0, 0.0], [10.0, 50.0]), "右端竖线 x=10");
        // 下半（镜像）
        assert!(has_line(&part, [0.0, 0.0], [0.0, -53.0]));
        assert!(has_arc(&part, [3.0, -51.8], 3.0, 90.0, 180.0));
        assert!(has_line(&part, [3.0, -48.8], [8.8, -48.8]));
        assert!(has_line(&part, [8.8, -48.8], [10.0, -50.0]));
        assert!(has_line(&part, [10.0, 0.0], [10.0, -50.0]));

        // 规格文本 / 元数据
        assert_eq!(part.meta.code, "GB/T 6403.5-2008");
        assert_eq!(part.meta.spec, "d100 b1 10");
        // 锚点 = 台阶面与轴线交点；包围盒只盖轮廓（砂轮细线端点不再有）
        assert_eq!(part.bbox, [0.0, -53.0, 10.0, 53.0]);
    }

    /// 其它 d：几何按公式走（不是写死 d=100 的那一组数）。
    #[test]
    fn geometry_generalizes_by_d() {
        // d=150（≥100 → b1=10/h=1.2/r=3）：外圆半径 75、台阶 78、圆心 (3,76.8)、槽底 73.8
        let part = generate(FAMILY_GRIND_OD, 150.0, None, "main").unwrap();
        assert!(has_line(&part, [0.0, 0.0], [0.0, 78.0]));
        assert!(has_arc(&part, [3.0, 76.8], 3.0, 180.0, 270.0));
        assert!(has_line(&part, [3.0, 73.8], [8.8, 73.8]));
        assert!(has_line(&part, [8.8, 73.8], [10.0, 75.0]));
        assert!(has_line(&part, [10.0, 0.0], [10.0, 75.0]));

        // d=8（≤10 → 最后一行 b1=1.6/h=0.2/r=0.5）：半径 4、台阶 4.5、圆心 (0.5,4.3)、槽底 3.8
        let part = generate(FAMILY_GRIND_OD, 8.0, None, "main").unwrap();
        assert!(has_line(&part, [0.0, 0.0], [0.0, 4.5]));
        assert!(has_arc(&part, [0.5, 4.3], 0.5, 180.0, 270.0));
        assert!(has_line(&part, [0.5, 3.8], [1.4, 3.8]));
        assert!(has_line(&part, [1.4, 3.8], [1.6, 4.0]));
        assert!(has_line(&part, [1.6, 0.0], [1.6, 4.0]));

        // 显式 b1 覆盖同样改几何：d=150 b1=8 → h=0.8/r=2、台阶 77、圆心 (2,76.2)、槽底 74.2
        let part = generate(FAMILY_GRIND_OD, 150.0, Some(8.0), "main").unwrap();
        assert!(has_line(&part, [0.0, 0.0], [0.0, 77.0]));
        assert!(has_arc(&part, [2.0, 76.2], 2.0, 180.0, 270.0));
        assert!(has_line(&part, [2.0, 74.2], [7.2, 74.2]));
        assert!(has_line(&part, [7.2, 74.2], [8.0, 75.0]));
    }

    /// 表自洽：b1 = r + 平段 + h（平段 > 0）；d>0 全部落在某个档。
    #[test]
    fn table_is_self_consistent() {
        for band in GROOVE_BANDS {
            for row in band.rows {
                assert!(
                    row.b1 > row.r + row.h + 1e-9,
                    "b1 = r + 平段 + h 要求平段 > 0：{row:?}"
                );
            }
        }
        for d in [
            0.001, 1.0, 9.999, 10.0, 10.001, 49.999, 50.0, 50.001, 99.999, 100.0, 1e6,
        ] {
            assert!(band_of(d).is_ok(), "d={d} 应落在某个档");
        }
    }

    /// 视图 / 目录 / 文件树 / 统一入口的接线。
    #[test]
    fn registry_catalog_and_tree_wiring() {
        // 只有主视图
        let err = generate(FAMILY_GRIND_OD, 100.0, None, "top").expect_err("只有主视图");
        assert!(err.contains("视图") && err.contains("main"), "{err}");
        assert_eq!(family_views(FAMILY_GRIND_OD), vec!["main"]);
        assert!(is_detail(FAMILY_GRIND_OD));
        assert!(!is_detail("hex_bolt_c"));
        assert!(try_generate("hex_bolt_c", 5.0, None, "main").is_none());

        // 目录：族条目 + 「结构要素」与「零件库」并列的根树
        let cat: serde_json::Value = serde_json::from_str(&crate::partgen::catalog_json()).unwrap();
        let family = &cat["families"][FAMILY_GRIND_OD];
        assert_eq!(family["kind"], "detail");
        assert_eq!(family["free_d"], true);
        assert_eq!(family["tree_dir"], "结构要素/砂轮越程槽");
        assert_eq!(family["views"][0]["id"], "main");
        assert_eq!(family["sizes"].as_array().unwrap().len(), 0);
        assert_eq!(family["bands"].as_array().unwrap().len(), 4);
        let roots = cat["tree"].as_array().unwrap();
        assert!(
            roots.iter().any(|n| n["name"] == "零件库"),
            "标准件树还在"
        );
        let root = roots
            .iter()
            .find(|n| n["name"] == "结构要素")
            .expect("结构要素根树");
        let leaf = &root["children"][0]["children"][0];
        assert_eq!(root["children"][0]["name"], "砂轮越程槽");
        assert_eq!(leaf["name"], "磨外圆 GB/T 6403.5-2008");
        assert_eq!(leaf["family"], FAMILY_GRIND_OD);

        // 统一入口 partgen::generate：detail 族用 l 槽位承载可选 b1（0 = 默认）
        let part = crate::partgen::generate(FAMILY_GRIND_OD, 100.0, 8.0, "main").unwrap();
        assert_eq!(part.meta.spec, "d100 b1 8");
        let part = crate::partgen::generate(FAMILY_GRIND_OD, 100.0, 0.0, "main").unwrap();
        assert_eq!(part.meta.spec, "d100 b1 10");
        assert!(crate::partgen::generate(FAMILY_GRIND_OD, 100.0, 0.0, "top").is_err());
    }

    /// 预览 SVG：结构要素不需要 l；b1 可带可不带。
    #[test]
    fn preview_svg_renders_detail_without_l() {
        let svg = preview_svg("family=detail_grind_od&d=100")
            .expect("是结构要素")
            .expect("d=100 应能出图");
        assert!(svg.contains("<svg") && svg.contains("d100 b1 10"), "{svg}");
        let svg = preview_svg("family=detail_grind_od&d=100&b1=8&view=main")
            .unwrap()
            .unwrap();
        assert!(svg.contains("d100 b1 8"));
        let err = preview_svg("family=detail_grind_od&d=100&b1=4")
            .unwrap()
            .unwrap_err();
        assert!(err.contains("可选 b1"), "{err}");
        assert!(preview_svg("family=hex_bolt_c&d=5&l=25").is_none());
    }

    // ── 外螺纹退刀槽（GB/T 3-1997 表 2）──────────────────────────────────

    /// 表 2：23 行、按 P 查表、demo 行数值；P=0.2 明确不在表里。
    #[test]
    fn thread_relief_table2_lookup() {
        assert_eq!(THREAD_RELIEF_ROWS.len(), 23);
        let row = thread_relief_row(1.5).expect("P=1.5 在表 2");
        assert!(
            near(row.g1, 2.5) && near(row.g2, 4.5) && near(row.dg_reduction, 2.3) && near(row.r, 0.8),
            "d=20/P=1.5 行 {row:?}"
        );
        let row = thread_relief_row(0.25).expect("P=0.25 在表 2");
        assert!(near(row.g1, 0.4) && near(row.g2, 0.75) && near(row.r, 0.12));
        let row = thread_relief_row(6.0).expect("P=6 在表 2");
        assert!(near(row.g1, 11.0) && near(row.g2, 18.0) && near(row.dg_reduction, 8.3));
        // 表 2 从 0.25 起（无 0.2 行）——报错要列出可用 P
        let err = thread_relief_row(0.2).expect_err("P=0.2 不在表 2");
        assert!(err.contains("0.25") && err.contains("0.2"), "{err}");
        assert!(thread_relief_row(1.3).is_err());
        assert!(thread_relief_row(0.0).is_err());
        assert!(thread_relief_row(f64::NAN).is_err());
    }

    /// 斜壁角自检（用户给的恒等式 `g2 ≈ g1 + ((d−dg)/2)/tan30°`）。
    ///
    /// 实测：表 2 的 g1/g2 是各自圆整的 min/max 极限值，恒等式**只有 11/23 行**
    /// 落在 0.02 内，最大偏差 0.188mm（P=6）——所以 0.02 容差版本不能当全表断言；
    /// 本测试按表值实际精度 0.2 立断言，并钉死最大偏差与最小斜壁角（表 2 正文
    /// 要求过渡角 α ≥30°；表列极限值圆整后最低 28.30°，即“ 30° 为下限”的含意）。
    #[test]
    fn thread_relief_wall_angle_identity() {
        let tan30 = DEFAULT_ALPHA_DEG.to_radians().tan();
        let mut max_err = 0.0_f64;
        let mut min_angle = f64::INFINITY;
        let mut within_0_02 = 0;
        for row in THREAD_RELIEF_ROWS {
            let ideal = row.g1 + (row.dg_reduction / 2.0) / tan30;
            let err = (row.g2 - ideal).abs();
            max_err = max_err.max(err);
            if err <= 0.02 {
                within_0_02 += 1;
            }
            let angle = ((row.dg_reduction / 2.0) / (row.g2 - row.g1)).atan().to_degrees();
            min_angle = min_angle.min(angle);
            assert!(
                angle + WALL_ANGLE_TOL_DEG >= DEFAULT_ALPHA_DEG,
                "P={} 斜壁实际角 {angle:.3}° 低于 alpha−容差",
                row.p
            );
        }
        assert_eq!(within_0_02, 11, "容差 0.02 只对 11 行成立（表值各自取整）");
        assert!((max_err - 0.188).abs() < 1e-3, "最大偏差 {max_err}（P=6）");
        assert!((min_angle - 28.301).abs() < 1e-3, "最小斜壁角 {min_angle}°（P=0.45）");
    }

    /// 示例 d=20、P=1.5：逐图元核对图 2 口径（dg=17.7 / g1=2.5 / g2=4.5 / r=0.8）。
    #[test]
    fn thread_relief_geometry_d20_p1_5() {
        let params = DetailParams::from_pairs([("P", 1.5)]);
        let part = generate_params(FAMILY_THREAD_RELIEF, 20.0, &params, "main").expect("应能出图");
        assert_eq!(part.entities.len(), 10, "上 5 + 下镜像 5");
        assert!(
            part.entities
                .iter()
                .all(|e| e.common().layer == LAYER_MAIN),
            "全部 1轮廓实线层"
        );
        // 上半：台肩面 x=0（圆角切点 9.65 → 大径+ r = 10.8）
        assert!(has_line(&part, [0.0, 9.65], [0.0, 10.8]), "台肩面");
        assert!(has_arc(&part, [0.8, 9.65], 0.8, 180.0, 270.0), "上圆角");
        assert!(has_line(&part, [0.8, 8.85], [2.5, 8.85]), "槽底");
        assert!(has_line(&part, [2.5, 8.85], [4.5, 10.0]), "30° 斜壁");
        assert!(has_line(&part, [4.5, 10.0], [9.5, 10.0]), "螺纹示意段 5mm");
        // 下半：镜像
        assert!(has_line(&part, [0.0, -9.65], [0.0, -10.8]));
        assert!(has_arc(&part, [0.8, -9.65], 0.8, 90.0, 180.0));
        assert!(has_line(&part, [0.8, -8.85], [2.5, -8.85]));
        assert!(has_line(&part, [2.5, -8.85], [4.5, -10.0]));
        assert!(has_line(&part, [4.5, -10.0], [9.5, -10.0]));
        // 元数据 / 包围盒（含台肩面高出大径的一个 r）
        assert_eq!(part.meta.code, "GB/T 3-1997");
        assert_eq!(part.meta.name, "外螺纹退刀槽");
        assert_eq!(part.meta.spec, "d20 P1.5 g1 2.5 g2 4.5");
        assert_eq!(part.bbox, [0.0, -10.8, 9.5, 10.8]);
        // 尺寸解算：dg=17.7，斜壁实际角 29.899°（表值取整，标称 30°）
        let dims = relief_dims(20.0, &params).unwrap();
        assert!(near(dims.dg, 17.7), "dg=17.7");
        assert!((dims.wall_angle_deg - 29.899).abs() < 1e-3, "{}", dims.wall_angle_deg);
    }

    /// 覆盖参数与非法几何：P 必给/不在表、d≤0、alpha<30、g1/g2/r/dg 打架。
    #[test]
    fn thread_relief_overrides_and_rejects_bad_geometry() {
        // P 必给
        let err = relief_dims(20.0, &DetailParams::new()).expect_err("缺 P");
        assert!(err.contains("P") && err.contains("detail_thread_relief"), "{err}");
        // d ≤ 0 / 非数
        assert!(relief_dims(0.0, &DetailParams::from_pairs([("P", 1.5)])).is_err());
        assert!(relief_dims(f64::NAN, &DetailParams::from_pairs([("P", 1.5)])).is_err());
        // alpha < 30 报错；alpha = 30 可以不写
        let err = relief_dims(20.0, &DetailParams::from_pairs([("P", 1.5), ("alpha", 25.0)]))
            .expect_err("alpha<30");
        assert!(err.contains("alpha") && err.contains("30"), "{err}");
        assert!(relief_dims(20.0, &DetailParams::from_pairs([("P", 1.5), ("alpha", 30.0)])).is_ok());
        // 覆盖 dg：槽更深（斜壁更陡），合法
        let dims = relief_dims(20.0, &DetailParams::from_pairs([("P", 1.5), ("dg", 16.0)])).unwrap();
        assert!(near(dims.dg, 16.0));
        assert!(near(dims.wall_angle_deg, 45.0), "{}", dims.wall_angle_deg);
        // 覆盖 g2 过大 → 斜壁太平（11.8°）→ 报错
        let err = relief_dims(20.0, &DetailParams::from_pairs([("P", 1.5), ("g2", 8.0)]))
            .expect_err("斜壁太浅");
        assert!(err.contains("斜壁") && err.contains("11.8"), "{err}");
        // g2 ≤ g1 / g1 ≤ r / r 超过槽深 / dg ≥ d
        assert!(relief_dims(20.0, &DetailParams::from_pairs([("P", 1.5), ("g2", 2.0)])).is_err());
        assert!(relief_dims(20.0, &DetailParams::from_pairs([("P", 1.5), ("g1", 0.5)])).is_err());
        assert!(relief_dims(20.0, &DetailParams::from_pairs([("P", 1.5), ("r", 1.2)])).is_err());
        assert!(relief_dims(20.0, &DetailParams::from_pairs([("P", 1.5), ("dg", 20.0)])).is_err());
        // 覆盖 g1/r 正常路径：几何跟着变
        let part = generate_params(
            FAMILY_THREAD_RELIEF,
            20.0,
            &DetailParams::from_pairs([("P", 1.5), ("g1", 3.0), ("r", 0.5)]),
            "main",
        )
        .unwrap();
        assert!(has_line(&part, [0.5, 8.85], [3.0, 8.85]), "新槽底");
        assert!(has_arc(&part, [0.5, 9.35], 0.5, 180.0, 270.0), "新圆角");
        // 历史 b1 槽位不能表达 P：明确报错（不静默用默认值）
        assert!(generate(FAMILY_THREAD_RELIEF, 20.0, None, "main").is_err());
        // 磨外圆的默认 generate_params 不认新参数
        let err = generate_params(FAMILY_GRIND_OD, 100.0, &DetailParams::from_pairs([("P", 1.5)]), "main")
            .expect_err("磨外圆不认 P");
        assert!(err.contains("不认识参数"), "{err}");
        // 退刀槽反过来不认 b1（防笔误静默出图）
        let err = relief_dims(20.0, &DetailParams::from_pairs([("P", 1.5), ("b1", 3.0)]))
            .expect_err("退刀槽不认 b1");
        assert!(err.contains("不认识参数") && err.contains("b1"), "{err}");
    }

    /// 目录 / 文件树 / 预览 SVG 的接线（与磨外圆并列但在同一棵「结构要素」下）。
    #[test]
    fn thread_relief_registry_catalog_and_preview() {
        assert!(is_detail(FAMILY_THREAD_RELIEF));
        assert_eq!(family_views(FAMILY_THREAD_RELIEF), vec!["main"]);
        let cat: serde_json::Value = serde_json::from_str(&crate::partgen::catalog_json()).unwrap();
        let family = &cat["families"][FAMILY_THREAD_RELIEF];
        assert_eq!(family["kind"], "detail");
        assert_eq!(family["tree_dir"], "结构要素/退刀槽");
        assert_eq!(family["free_d"], true);
        assert_eq!(family["inputs"].as_array().unwrap().len(), 6, "自由参数面板");
        assert_eq!(family["inputs"][0]["key"], "P");
        assert_eq!(family["inputs"][0]["required"], true);
        assert_eq!(family["pitches"].as_array().unwrap().len(), 23, "表 2 进目录");
        assert_eq!(family["sample"]["d"], 20.0);
        assert_eq!(family["sample"]["P"], 1.5);
        let roots = cat["tree"].as_array().unwrap();
        let root = roots
            .iter()
            .find(|n| n["name"] == "结构要素")
            .expect("结构要素根树");
        let dir = root["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["name"] == "退刀槽")
            .expect("结构要素/退刀槽");
        assert_eq!(dir["children"][0]["name"], "外螺纹退刀槽 GB/T 3-1997");
        assert_eq!(dir["children"][0]["family"], FAMILY_THREAD_RELIEF);
        // 预览：无 l，P 从 URL 参数进（大小写不敏感）；覆盖 g1 进几何
        let svg = preview_svg("family=detail_thread_relief&d=20&P=1.5")
            .expect("是结构要素")
            .expect("d=20 P=1.5 应能出图");
        assert!(svg.contains("<svg") && svg.contains("d20 P1.5 g1 2.5 g2 4.5"), "{svg}");
        let svg = preview_svg("family=detail_thread_relief&d=20&P=1.5&g1=3")
            .unwrap()
            .unwrap();
        assert!(svg.contains("g1 3"), "{svg}");
        let err = preview_svg("family=detail_thread_relief&d=20")
            .unwrap()
            .expect_err("缺 P");
        assert!(err.contains("P"), "{err}");
        let err = preview_svg("family=detail_thread_relief&d=20&P=1.3")
            .unwrap()
            .expect_err("P 不在表 2");
        assert!(err.contains("0.25"), "{err}");
        let err = preview_svg("family=detail_thread_relief&d=20&P=x")
            .unwrap()
            .expect_err("P 不是数字");
        assert!(err.contains("不是数字"), "{err}");
        let err = preview_svg("family=detail_thread_relief&d=20&P=1.5&b1=3")
            .unwrap()
            .expect_err("退刀槽不认 b1");
        assert!(err.contains("不认识参数"), "{err}");
        // 空值 / 无关的 `l` 不当参数（旧行为：忽略）；磨外圆 b1=8 仍照旧
        let svg = preview_svg("family=detail_grind_od&d=100&b1=&l=25")
            .expect("是结构要素")
            .expect("空值应忽略");
        assert!(svg.contains("d100 b1 10"), "{svg}");
    }

    // ── 矩形花键（GB/T 1144 规格代号 + GB/T 10952 de）─────────────────

    /// 族接线：视图 / 目录（规格代号下拉 + L/de 输入）/ 文件树 / 三视图预览。
    #[test]
    fn spline_rect_registry_catalog_and_preview() {
        assert!(is_detail(FAMILY_SPLINE_RECT));
        assert_eq!(family_views(FAMILY_SPLINE_RECT), vec!["front", "side", "section"]);
        // 规格代号入口（XL CLI 用）：解析出小径 d，并预设 spec
        let (d, params) = parse_spec_token(FAMILY_SPLINE_RECT, "6x23x26x6").expect("认规格代号");
        assert_eq!(d, 23.0);
        assert_eq!(params.spec(), Some("6x23x26x6"));
        assert!(parse_spec_token(FAMILY_SPLINE_RECT, "6x23x26").is_none());
        assert!(parse_spec_token(FAMILY_GRIND_OD, "6x23x26x6").is_none());

        let cat: serde_json::Value = serde_json::from_str(&crate::partgen::catalog_json()).unwrap();
        let family = &cat["families"][FAMILY_SPLINE_RECT];
        assert_eq!(family["kind"], "detail");
        assert_eq!(family["tree_dir"], "结构要素/花键");
        assert_eq!(family["free_d"], true);
        assert_eq!(family["code"], "GB/T 1144-2001");
        assert_eq!(family["specs"].as_array().unwrap().len(), 33, "轻 15 + 中 18");
        assert_eq!(family["inputs"].as_array().unwrap().len(), 2);
        assert_eq!(family["inputs"][0]["key"], "len");
        assert_eq!(family["inputs"][0]["required"], true);
        assert_eq!(family["inputs"][1]["key"], "de");
        // 视图按钮中文名被族覆盖
        let views: Vec<&str> = family["views"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect();
        assert_eq!(views, vec!["正视图（端视图）", "常规侧视图", "侧剖视图"]);
        assert_eq!(family["sample"]["d"], 23.0);
        let roots = cat["tree"].as_array().unwrap();
        let root = roots.iter().find(|n| n["name"] == "结构要素").expect("结构要素根树");
        let dir = root["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["name"] == "花键")
            .expect("结构要素/花键");
        assert_eq!(dir["children"][0]["name"], "矩形花键 GB/T 1144-2001");
        assert_eq!(dir["children"][0]["family"], FAMILY_SPLINE_RECT);

        // 预览：spec 可替代 d；L 走 `len`；de 可覆盖；三个视图都能出。
        let svg = preview_svg("family=detail_spline_rect&spec=6x23x26x6&len=30&view=side")
            .expect("是结构要素")
            .expect("侧视图应能出图");
        assert!(svg.contains("<svg") && svg.contains("6x23x26x6 L30"), "{svg}");
        // 表内规格不传 de 也能出图：自动查表结果与显式 de=63 完全一致（R=de/2、l 同式）。
        let svg_de = preview_svg("family=detail_spline_rect&spec=6x23x26x6&len=30&de=63&view=side")
            .unwrap()
            .unwrap();
        assert_eq!(svg, svg_de, "表内规格不传 de 应自动查表（等价于 de=63）");
        let svg = preview_svg("family=detail_spline_rect&spec=6x23x26x6&len=30&view=front")
            .unwrap()
            .unwrap();
        assert!(svg.contains("<svg"), "正视图：{svg}");
        let svg = preview_svg("family=detail_spline_rect&spec=6x23x26x6&len=30&de=71&view=section")
            .unwrap()
            .unwrap();
        assert!(svg.contains("6x23x26x6 L30"), "剖视图 de 覆盖：{svg}");
        let err = preview_svg("family=detail_spline_rect&spec=6x23x26x6&view=side")
            .unwrap()
            .expect_err("侧视图缺 L");
        assert!(err.contains("L"), "{err}");
        let err = preview_svg("family=detail_spline_rect&spec=6x11x14x3&len=20&view=side")
            .unwrap()
            .expect_err("表外规格缺 de");
        assert!(err.contains("de"), "{err}");
        let err = preview_svg("family=detail_spline_rect&spec=6x23x26x6&len=30&b1=3&view=side")
            .unwrap()
            .expect_err("花键不认 b1");
        assert!(err.contains("不认识参数"), "{err}");
        // 数字参数路线（XL CLI / MCP）：`d=23 N6 D26 B6 len30 de63`
        let svg = preview_svg(
            "family=detail_spline_rect&d=23&N=6&big=26&B=6&len=30&de=63&view=side",
        )
        .unwrap()
        .unwrap();
        assert!(svg.contains("6x23x26x6 L30"), "{svg}");
    }

    /// 三个视图生成：图元数 / 落层 / 侧剖视图的 HATCH 口径。
    #[test]
    fn spline_rect_views_generate() {
        let mut params = DetailParams::new();
        params.set_spec("6x23x26x6");
        params.insert("len", 30.0);
        let front = generate_params(FAMILY_SPLINE_RECT, 23.0, &params, "front").unwrap();
        assert_eq!(front.meta.code, "GB/T 1144-2001");
        assert_eq!(front.entities.len(), 26);
        assert_eq!(front.meta.spec, "6x23x26x6");
        let side = generate_params(FAMILY_SPLINE_RECT, 23.0, &params, "side").unwrap();
        assert_eq!(side.entities.len(), 7);
        assert_eq!(side.meta.spec, "6x23x26x6 L30");
        let section = generate_params(FAMILY_SPLINE_RECT, 23.0, &params, "section").unwrap();
        assert_eq!(section.entities.len(), 8, "7 线 + 1 HATCH");
        assert!(section
            .entities
            .iter()
            .any(|e| matches!(e, EntityType::Hatch(_))));
        // 老 b1 入口明确报错指路
        let err = SPLINE_RECT.generate(23.0, None, "front").unwrap_err();
        assert!(err.contains("规格代号"), "{err}");
        // spec 与 d 不一致报错
        let err = generate_params(FAMILY_SPLINE_RECT, 26.0, &params, "side").unwrap_err();
        assert!(err.contains("不一致"), "{err}");
    }

    /// 表 1（收尾/肩距）：已按数据存好，比例自检；P=1.75 的 `x一般` 用订正后的 4.3。
    #[test]
    fn runout_table1_ratios_are_self_consistent() {
        assert_eq!(RUNOUT_ROWS.len(), 24);
        assert!(near(RUNOUT_ROWS[0].p, 0.2), "表 1 从 P=0.2 起");
        for row in RUNOUT_ROWS {
            // 表值是圆整到 0.05/0.1 的：x 一般/短 与 a 一般 用 0.2P 容差
            // （实测最大偏差 0.25mm：x一般 P=3.5，a一般 P=1.25）
            let tol = 0.2 * row.p + 1e-9;
            assert!(
                (row.x_normal - 2.5 * row.p).abs() <= tol,
                "P={} x一般 {} vs 2.5P {}",
                row.p,
                row.x_normal,
                2.5 * row.p
            );
            assert!(
                (row.x_short - 1.25 * row.p).abs() <= tol,
                "P={} x短 {} vs 1.25P {}",
                row.p,
                row.x_short,
                1.25 * row.p
            );
            assert!(
                (row.a_normal - 3.0 * row.p).abs() <= tol,
                "P={} a一般 {} vs 3P {}",
                row.p,
                row.a_normal,
                3.0 * row.p
            );
            // a长 = 4P、a短 = 2P 是全表精确值
            assert!(near(row.a_long, 4.0 * row.p), "P={} a长 = 4P", row.p);
            assert!(near(row.a_short, 2.0 * row.p), "P={} a短 = 2P", row.p);
        }
        // 订正记录：网页 P=1.75 的 x一般 印成 1.3，真标准为 4.3
        let row = runout_row(1.75).expect("P=1.75 在表 1");
        assert!(near(row.x_normal, 4.3), "P=1.75 x一般 应为 4.3（非 1.3）");
        // 表 1 的 P 集合 ⊃ 表 2（多一行 0.2）
        assert_eq!(RUNOUT_ROWS.len(), THREAD_RELIEF_ROWS.len() + 1);
        assert!(runout_row(0.2).is_some() && thread_relief_row(0.2).is_err());
    }

    /// 示例几何 → `~/桌面/OCSM/review/relief_demo.csv`（列同 `shaft_demo.csv`）。
    ///
    /// 供人工/工具与 GB/T 3 图 2 逐图元叠合核对。
    #[test]
    fn thread_relief_demo_csv_dump() {
        let part = generate_params(
            FAMILY_THREAD_RELIEF,
            20.0,
            &DetailParams::from_pairs([("P", 1.5)]),
            "main",
        )
        .unwrap();
        let csv = entities_csv(&part.entities);
        let path = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            .join("桌面/OCSM/review");
        std::fs::create_dir_all(&path).expect("建 review 目录");
        let file = path.join("relief_demo.csv");
        std::fs::write(&file, &csv).expect("写 relief_demo.csv");
        assert!(file.is_file(), "demo CSV 已落盘：{}", file.display());
        assert!(csv.contains("LINE") && csv.contains("ARC"), "{csv}");
        assert_eq!(part.entities.len(), 10, "demo 图元数");
        assert!(
            csv.contains("ARC,0.000000,9.650000,0.800000,8.850000,1轮廓实线层,0.800000,9.650000,0.800000,180.0000,270.0000"),
            "上圆角一行：{csv}"
        );
    }

    /// 图元序列 → CSV（列：entity,x1,y1,x2,y2,layer,cx,cy,r,a0,a1）。
    /// 与 `shaft.rs::tests::entities_csv` 同格式（本模块只出 LINE/ARC）。
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
                other => panic!("demo dump 只支持 LINE/ARC，得到 {other:?}"),
            }
        }
        csv
    }
}
