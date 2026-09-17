//! **结构要素**（轴上的工艺结构轮廓）参数化生成。
//!
//! 第一期：GB/T 6403.5-2008 砂轮越程槽 —— **磨外圆**（族 id `detail_grind_od`）。
//!
//! ## 数据出处（唯一数据源 = `GROOVE_BANDS`，改表只改这一处）
//! - 尺寸表：用户提供的参数表 `磨外圆_GB-T6403.png`（列：b1 / h / r / d）；
//! - 几何模板：`磨外圆_GB-T6403.5-2008.dxf`（1:1，逐图元反解核对；
//!   d=100 → 最后一行 b1=10 / h=1.2 / r=3）。
//!
//! ## 画法（模板 `1轮廓实线层` + `2细线层` 逐图元反解）
//! 轴为 x 轴（x 向右、y = 半径），**锚点 = 台阶面与轴线交点 (0,0)**；上半侧 + 下半侧镜像：
//! - 台阶面竖线 `x=0`：`y = 0 … d/2 + r`（模板 53 = 50 + 3）；
//! - 圆角弧 `R=r`：圆心 `(r, d/2−h+r)`（模板 `(3, 51.8)`），与台阶面/槽底相切；
//! - 槽底 `y = d/2−h`：`x = r … b1−h`（模板 48.8：x 3…8.8）；
//! - 45° 斜线：`(b1−h, d/2−h) → (b1, d/2)`（模板 `(8.8,48.8) → (10,50)`）；
//! - 右端竖线 `x=b1`：`y = 0 … ±d/2`（与"磨出的外圆"闭合）；
//! - 砂轮细实线 1 条：从 `(b1, d/2)` 45° 上扬 `WHEEL_TAIL`（模板到 `(25,65)`，即 15）。
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
use crate::partgen_kit::{arc, line, trim, LAYER_MAIN, LAYER_THIN};

// ══════════════════════════════════════════════════════════════════════════
// 通用框架（一族 = 一个 DetailElement；新增要素只动本文件）
// ══════════════════════════════════════════════════════════════════════════

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
    /// 目录 JSON 的补充字段（`free_d` / `bands` 等，GUI 自由输入表单用）。
    fn catalog_extra(&self) -> serde_json::Value;
}

/// 已登记的要素（新增要素往这里加一项）。
pub static ELEMENTS: &[&dyn DetailElement] = &[&GRIND_OD];

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

/// 生成（找不到族报错）。
pub fn generate(family: &str, d: f64, b1: Option<f64>, view: &str) -> Result<GenPart, String> {
    let element = find(family).ok_or_else(|| format!("结构要素族 {family} 尚未实现"))?;
    if !element.views().contains(&view) {
        return Err(format!(
            "{} 只有视图 {}（收到 {view}）",
            element.name(),
            element.views().join("/")
        ));
    }
    element.generate(d, b1, view)
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

/// `/api/part_svg` 的**结构要素**分支（`family=…&d=…[&b1=…]&view=main`）。
/// 返回 `None` = 不是结构要素，调用方走标准件分支。
pub fn preview_svg(query: &str) -> Option<Result<String, String>> {
    let get = |key: &str| {
        query
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.to_string())
    };
    let family = get("family")?;
    let element = find(&family)?;
    Some((|| {
        let d: f64 = get("d")
            .ok_or("缺少参数 d")?
            .parse()
            .map_err(|_| "d 不是数字".to_string())?;
        let b1 = match get("b1").as_deref() {
            Some("") | None => None,
            Some(text) => Some(text.parse::<f64>().map_err(|_| "b1 不是数字".to_string())?),
        };
        let view = get("view").unwrap_or_else(|| "main".to_string());
        let part = generate(element.family(), d, b1, &view)?;
        Ok(crate::partgen::to_svg(
            &part,
            &format!("{} {}", part.meta.code, part.meta.spec),
            520.0,
            260.0,
        ))
    })())
}

/// 图元包围盒（`[xmin, ymin, xmax, ymax]`）。圆弧按**落在弧上的**四个象限点精确取。
fn entity_bbox(entities: &[EntityType]) -> [f64; 4] {
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

/// 砂轮细实线尾长（模板实测：从 `(b1, d/2)` 45° 上扬到 `(b1+15, d/2+15)`）。
pub const WHEEL_TAIL: f64 = 15.0;

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
/// `1轮廓实线层` 10 条（上半 5 + 下半镜像 5）+ `2细线层` 砂轮细线 1 条。
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

    // 砂轮细实线（模板 (10,50) → (25,65)）
    entities.push(line(
        [b1, rg],
        [b1 + WHEEL_TAIL, rg + WHEEL_TAIL],
        LAYER_THIN,
    ));

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
/// 保留上半/下半的 R 圆角、槽底、45° 斜坡 + 砂轮细实线，落层与原来一致。
///
/// 局部坐标与 `build_grind_od` 完全一致：台阶面 = `x=0`、槽向 `+x` 展开、
/// 外圆 `y = ±d/2`；返回 `(图元, 数据行)`，图元顺序 = 上圆角/槽底/斜坡、
/// 下圆角/槽底/斜坡、砂轮细线。
///
/// **不改 `build_grind_od` 的输出**（独立要素预览/DXF 不受影响）。
pub fn groove_entities(
    d: f64,
    b1: Option<f64>,
) -> Result<(Vec<EntityType>, &'static GrooveRow), String> {
    let row = row_for(d, b1)?;
    let part = build_grind_od(d, row);
    // build_grind_od 的固定顺序：上（台阶面/圆角/槽底/斜坡/右端）→ 下（同构）→ 砂轮细线；
    // 丢弃 0/5（台阶面）与 4/9（右端闭合竖线）。
    let keep = [1usize, 2, 3, 6, 7, 8, 10];
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
// 测试
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn line_layer(e: &EntityType) -> Option<&str> {
        match e {
            EntityType::Line(_) => Some(e.common().layer.as_str()),
            _ => None,
        }
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

    /// 模板 d=100 档：图元数 / 落层 / 坐标与 DXF 逐图元一致。
    #[test]
    fn template_d100_matches_dxf() {
        let part = generate(FAMILY_GRIND_OD, 100.0, None, "main").unwrap();
        assert_eq!(part.entities.len(), 11, "10 轮廓 + 1 砂轮细线");
        let main = part
            .entities
            .iter()
            .filter(|e| e.common().layer == LAYER_MAIN)
            .count();
        let thin = part
            .entities
            .iter()
            .filter(|e| e.common().layer == LAYER_THIN)
            .count();
        assert_eq!((main, thin), (10, 1), "落层：轮廓 → 1轮廓实线层，细线 → 2细线层");
        assert!(part
            .entities
            .iter()
            .all(|e| e.common().layer == LAYER_MAIN || e.common().layer == LAYER_THIN));
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
        // 砂轮细线（模板 (10,50) → (25,65)）
        assert!(has_line(&part, [10.0, 50.0], [25.0, 65.0]));
        assert_eq!(line_layer(&part.entities[10]).unwrap_or(""), LAYER_THIN);

        // 规格文本 / 元数据
        assert_eq!(part.meta.code, "GB/T 6403.5-2008");
        assert_eq!(part.meta.spec, "d100 b1 10");
        // 锚点 = 台阶面与轴线交点；包围盒盖住全部图元（含细线端点）
        assert_eq!(part.bbox, [0.0, -53.0, 25.0, 65.0]);
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
        assert!(has_line(&part, [10.0, 75.0], [25.0, 90.0]));

        // d=8（≤10 → 最后一行 b1=1.6/h=0.2/r=0.5）：半径 4、台阶 4.5、圆心 (0.5,4.3)、槽底 3.8
        let part = generate(FAMILY_GRIND_OD, 8.0, None, "main").unwrap();
        assert!(has_line(&part, [0.0, 0.0], [0.0, 4.5]));
        assert!(has_arc(&part, [0.5, 4.3], 0.5, 180.0, 270.0));
        assert!(has_line(&part, [0.5, 3.8], [1.4, 3.8]));
        assert!(has_line(&part, [1.4, 3.8], [1.6, 4.0]));
        assert!(has_line(&part, [1.6, 0.0], [1.6, 4.0]));
        assert!(has_line(&part, [1.6, 4.0], [16.6, 19.0]));

        // 显式 b1 覆盖同样改几何：d=150 b1=8 → h=0.8/r=2、台阶 77、圆心 (2,76.2)、槽底 74.2
        let part = generate(FAMILY_GRIND_OD, 150.0, Some(8.0), "main").unwrap();
        assert!(has_line(&part, [0.0, 0.0], [0.0, 77.0]));
        assert!(has_arc(&part, [2.0, 76.2], 2.0, 180.0, 270.0));
        assert!(has_line(&part, [2.0, 74.2], [7.2, 74.2]));
        assert!(has_line(&part, [7.2, 74.2], [8.0, 75.0]));
        assert!(has_line(&part, [8.0, 75.0], [23.0, 90.0]));
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
}
