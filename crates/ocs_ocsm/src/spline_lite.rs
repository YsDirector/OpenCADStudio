//! 智能卡片「GB 花键参数表·**精简版**」（`OCSMCARD`；用户 2026-09-27 点单）。
//!
//! 用户口径：**不留精度、只有基本信息**——卡面只列「执行标准 / 模数 / 齿数 / 齿形角 /
//! 齿根样式 / 大径 / 小径」+ 主要测量量（内：量棒 Dp、跨棒距 Md；外：跨测齿数 Kn、
//! 公法线 Wn），**不含任何公差列（不显示上/下偏差）**。
//!
//! ## 同源
//! 取值与 GB 花键卡**同一份** `spline_tol::compute()`：本模块只挑字段、不另算。
//! 表达式反解复用 `spline_table`/`card_expr` 的唯一解析器（`SplineTableModel::resolved`）。
//!
//! ## 版面（自定，无用户模板；风格照 GB 卡）
//! 沿用 GB 模板框架坐标与行高（`6.898720166384379`）、线层（`1轮廓实线层`/`2细线层`）、
//! 文字层与 `OCSM_GB` 样式、标签列/值列分栏；**把公差整列去掉**（值列直接到右框 x=0，
//! 不再放 x≈−9.5 的上/下偏差 ATTDEF）。10 行 = 1 标题 + 9 数据行；14 线 + 10 文字 + 9 属性。
//! 值列 ATTDEF 实体级字宽 0.7（与 GB 卡 [`crate::spline_table::VALUE_WIDTH_FACTOR`] 同口径）。
//!
//! ## 扩展点（表驱动）
//! 一卡一方向；以后扩到 NF/DIN/ANSI/齿轮 = 给该体系加一份 [`LiteFieldSpec`] 表 +
//! `CARD_TYPES` 一行 + `CardRenderer` 分派一个分支；GUI 统一骨架/计算书/表达式反解直接复用。

use crate::card::{CardFieldSpec, CardFormSpec};
use crate::spline_tol::{RootForm, SplineSide, SplineTable};
use ocs_plugin_api::host::acadrust::entities::{
    AttributeDefinition, AttributeEntity, Entity as _, EntityType, Insert, Text,
};
use ocs_plugin_api::host::acadrust::types::{Color, LineWeight, Vector3};

/// 内花键精简卡块名。
pub const BLOCK_INT: &str = "OCSM_SPLITE_GB_INT";
/// 外花键精简卡块名。
pub const BLOCK_EXT: &str = "OCSM_SPLITE_GB_EXT";

/// 按方向取块名。
pub fn block_name(side: SplineSide) -> &'static str {
    match side {
        SplineSide::Internal => BLOCK_INT,
        SplineSide::External => BLOCK_EXT,
    }
}

// ── 版面常量（坐标照 GB 模板；右框直接收在值列，公差列不存在）───────────────
/// 行高（GB 模板实测）。
const ROW: f64 = 6.898_720_166_384_379;
/// 数据行数（标题外）。
const DATA_ROWS: usize = 9;
/// 左框 x。
const X_LEFT: f64 = -62.693_742_864_145;
/// 标签/值分栏线 x。
const X_SEP: f64 = -34.865_056_129_880_27;
/// 右框 x（= 原值列右缘；公差列已整列去掉）。
const X_RIGHT: f64 = 0.0;
/// 标签插入 x（照模板）。
const X_LABEL: f64 = -61.709_962_722_953_35;
/// 值插入 x（分栏线右侧留 1.47）。
const X_VALUE: f64 = -33.4;
/// 字高（GB 模板同款）。
const TEXT_H: f64 = 3.6;
/// 标签字宽（模板同款）。
const LABEL_WF: f64 = 1.0;
/// 值 ATTDEF 实体级字宽（与 GB 卡 0.7 同口径，不动全局 `OCSM_GB`）。
const VALUE_WF: f64 = 0.7;
/// 标题插入 y（模板）。
const Y_TITLE: f64 = -5.200_708_803_331_338;

const LAYER_OUTLINE: &str = "1轮廓实线层";
const LAYER_THIN: &str = "2细线层";
const LAYER_TEXT: &str = "6文字层";

/// 一行数据（表驱动；tag 同时是 ATTDEF 键与取值映射键）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct LiteFieldSpec {
    /// ATTDEF tag（`(简)` 前缀，避免与 GB 卡块混淆）。
    pub tag: &'static str,
    /// 卡面标签（含符号）。
    pub label: &'static str,
    /// 单位（进 GUI 预览；卡面不加单位行）。
    pub unit: &'static str,
    /// 公式/口径（GUI 行 title、计算书）。
    pub formula: &'static str,
    /// 来源。
    pub source: &'static str,
}

const INTERNAL_FIELDS: &[LiteFieldSpec] = &[
    LiteFieldSpec {
        tag: "(简)执行标准",
        label: "执行标准",
        unit: "",
        formula: "GB/T 3478.1-2008 渐开线花键（精简卡口径：只列基本参数与主要测量量，不含公差列）",
        source: "GB/T 3478.1-2008",
    },
    LiteFieldSpec {
        tag: "(简)模数",
        label: "模数 m",
        unit: "mm",
        formula: "m = 九字段齿形表达式反解（与 GB 花键卡同一 `spline_tol::compute()`）",
        source: "GB/T 3478.1",
    },
    LiteFieldSpec {
        tag: "(简)齿数",
        label: "齿数 z",
        unit: "",
        formula: "z = 九字段齿形表达式反解",
        source: "GB/T 3478.1",
    },
    LiteFieldSpec {
        tag: "(简)齿形角",
        label: "齿形角 αD",
        unit: "°",
        formula: "αD = 表达式反解（30/37.5/45）",
        source: "GB/T 3478.1 表 3",
    },
    LiteFieldSpec {
        tag: "(简)齿根样式",
        label: "齿根样式",
        unit: "",
        formula: "由表达式 DA/DF 反解或手选；平齿根（仅 30°）/ 圆齿根",
        source: "GB/T 3478.1 §5",
    },
    LiteFieldSpec {
        tag: "(简)大径",
        label: "大径 Dei",
        unit: "mm",
        formula: "Dei = m(z+1.5) 30°平 / m(z+1.8) 30°圆 / m(z+1.4) 37.5° / m(z+1.2) 45°",
        source: "GB/T 3478.1 表 3",
    },
    LiteFieldSpec {
        tag: "(简)小径",
        label: "小径 Dii",
        unit: "mm",
        formula: "Dii = D_Femax(H/h) + 2CF（CF = 0.1m）",
        source: "GB/T 3478.1 表 3 注 2/注 3",
    },
    LiteFieldSpec {
        tag: "(简)量棒直径",
        label: "量棒直径 Dp",
        unit: "mm",
        formula: "D'_Ri = Db[tanαci − tan(αci − E_max/D + invαci − invαD)]，按 GB/T 3478.9 R40 取最接近较大值",
        source: "GB/T 3478.6 式(1)、GB/T 3478.9 表 1",
    },
    LiteFieldSpec {
        tag: "(简)测量跨棒距",
        label: "测量跨棒距 Md",
        unit: "mm",
        formula: "偶齿 M = Db/cosαi ∓ Dp；奇齿再乘 cos(90°/z)",
        source: "GB/T 3478.6 式(2)~(5)",
    },
];

const EXTERNAL_FIELDS: &[LiteFieldSpec] = &[
    LiteFieldSpec {
        tag: "(简)执行标准",
        label: "执行标准",
        unit: "",
        formula: "GB/T 3478.1-2008 渐开线花键（精简卡口径：只列基本参数与主要测量量，不含公差列）",
        source: "GB/T 3478.1-2008",
    },
    LiteFieldSpec {
        tag: "(简)模数",
        label: "模数 m",
        unit: "mm",
        formula: "m = 九字段齿形表达式反解（与 GB 花键卡同一 `spline_tol::compute()`）",
        source: "GB/T 3478.1",
    },
    LiteFieldSpec {
        tag: "(简)齿数",
        label: "齿数 z",
        unit: "",
        formula: "z = 九字段齿形表达式反解",
        source: "GB/T 3478.1",
    },
    LiteFieldSpec {
        tag: "(简)齿形角",
        label: "齿形角 αD",
        unit: "°",
        formula: "αD = 表达式反解（30/37.5/45）",
        source: "GB/T 3478.1 表 3",
    },
    LiteFieldSpec {
        tag: "(简)齿根样式",
        label: "齿根样式",
        unit: "",
        formula: "由表达式 DA/DF 反解或手选；平齿根（仅 30°）/ 圆齿根",
        source: "GB/T 3478.1 §5",
    },
    LiteFieldSpec {
        tag: "(简)大径",
        label: "大径 Dee",
        unit: "mm",
        formula: "Dee = m(z+1) 30° / m(z+0.9) 37.5° / m(z+0.8) 45°",
        source: "GB/T 3478.1 表 3",
    },
    LiteFieldSpec {
        tag: "(简)小径",
        label: "小径 Die",
        unit: "mm",
        formula: "Die = m(z−1.5) 30°平 / m(z−1.8) 30°圆 / m(z−1.4) 37.5° / m(z−1.2) 45°",
        source: "GB/T 3478.1 表 3",
    },
    LiteFieldSpec {
        tag: "(简)跨测齿数",
        label: "跨测齿数 Kn",
        unit: "",
        formula: "K = z/6 + 0.5 取整数",
        source: "GB/T 3478.6 式(11) 注",
    },
    LiteFieldSpec {
        tag: "(简)公法线长度",
        label: "公法线长度 Wn",
        unit: "mm",
        formula: "W_min = cosαD[(K−0.5)πm + D·invαD + esv − (T+λ)]；W = (W_min+W_max)/2",
        source: "GB/T 3478.6 式(11)(12)",
    },
];

/// 本方向的字段表（一卡一方向；扩体系 = 加表 + 分派）。
pub(crate) fn fields(side: SplineSide) -> &'static [LiteFieldSpec] {
    match side {
        SplineSide::Internal => INTERNAL_FIELDS,
        SplineSide::External => EXTERNAL_FIELDS,
    }
}

/// 精简内卡表单（统一骨架；字段全走 `card_types[].form` 下发，页面不写卡专属 HTML）。
pub const FORM_INT: CardFormSpec = CardFormSpec {
    fields: &[
        CardFieldSpec {
            key: "expr",
            label: "齿形表达式（九字段；可从轴/齿轮生成器 GUI 复制）",
            kind: "textarea",
            placeholder: "SPLINE IN M3 Z20 ALPHA30 X0 DA65.4 DF57.3436 BETA0 H30",
            default: "SPLINE IN M3 Z20 ALPHA30 X0 DA65.4 DF57.3436 BETA0 H30",
            title: "表达式反解 m/z/αD/x/Da/Df；与 GB 花键卡/轴生成器同一解析器（card_expr）",
            options: &[],
            options_from: "",
            min: 0.0,
            step: 0.0,
            required: true,
        },
        CardFieldSpec {
            key: "grade",
            label: "公差等级（只影响 Md 计算；卡面不显示公差）",
            kind: "number",
            placeholder: "6",
            default: "6",
            title: "4/5/6/7 级；精简卡不含公差列，等级只影响测量量的计算口径",
            options: &[],
            options_from: "",
            min: 4.0,
            step: 1.0,
            required: true,
        },
        CardFieldSpec {
            key: "fit",
            label: "配合类别（只影响 Md 计算；卡面不显示公差）",
            kind: "select",
            placeholder: "",
            default: "H",
            title: "内花键基孔制 H；精简卡不含公差列",
            options: &[("H", "H")],
            options_from: "",
            min: 0.0,
            step: 0.0,
            required: true,
        },
        CardFieldSpec {
            key: "root",
            label: "齿根样式",
            kind: "select",
            placeholder: "",
            default: "auto",
            title: "平/圆；auto = 由表达式 DA/DF 反解，再退到按 αD 默认",
            options: &[("auto", "自动（按表达式反解）"), ("flat", "平齿根"), ("fillet", "圆齿根")],
            options_from: "",
            min: 0.0,
            step: 0.0,
            required: false,
        },
        CardFieldSpec {
            key: "dp",
            label: "量棒直径 Dp（留空 = 标准 R40 自动选）",
            kind: "number",
            placeholder: "留空自动",
            default: "",
            title: "手填必须是 GB/T 3478.9 系列值；留空按标准 R40 规则自动选",
            options: &[],
            options_from: "",
            min: 0.0,
            step: 0.01,
            required: false,
        },
    ],
    note: "精简版：只列基本参数 + 量棒/跨棒距，不含任何公差列；需要公差请用「GB 花键参数表（内）」。",
    missing_note: "精简卡面固定不含公差（上/下偏差）；等级/配合只用于 Md 计算（点项标题可看口径）。",
};

/// 精简外卡表单。
pub const FORM_EXT: CardFormSpec = CardFormSpec {
    fields: &[
        CardFieldSpec {
            key: "expr",
            label: "齿形表达式（九字段；可从轴/齿轮生成器 GUI 复制）",
            kind: "textarea",
            placeholder: "SPLINE EX M3 Z20 ALPHA30 X0 DA66 DF52.5 BETA0 H30",
            default: "SPLINE EX M2 Z20 ALPHA30 X0 DA42 DF37 BETA0 H30",
            title: "表达式反解 m/z/αD/x/Da/Df；与 GB 花键卡/轴生成器同一解析器（card_expr）",
            options: &[],
            options_from: "",
            min: 0.0,
            step: 0.0,
            required: true,
        },
        CardFieldSpec {
            key: "grade",
            label: "公差等级（只影响 Wn 计算；卡面不显示公差）",
            kind: "number",
            placeholder: "6",
            default: "6",
            title: "4/5/6/7 级；精简卡不含公差列，等级只影响测量量的计算口径",
            options: &[],
            options_from: "",
            min: 4.0,
            step: 1.0,
            required: true,
        },
        CardFieldSpec {
            key: "fit",
            label: "配合类别（只影响 Wn 计算；卡面不显示公差）",
            kind: "select",
            placeholder: "",
            default: "h",
            title: "外花键基本偏差 h/js/k/d/e/f（与内花键 H 相配）；精简卡不含公差列",
            options: &[
                ("h", "h"),
                ("js", "js"),
                ("k", "k"),
                ("d", "d"),
                ("e", "e"),
                ("f", "f"),
            ],
            options_from: "",
            min: 0.0,
            step: 0.0,
            required: true,
        },
        CardFieldSpec {
            key: "root",
            label: "齿根样式",
            kind: "select",
            placeholder: "",
            default: "auto",
            title: "平/圆；auto = 由表达式 DA/DF 反解，再退到按 αD 默认",
            options: &[("auto", "自动（按表达式反解）"), ("flat", "平齿根"), ("fillet", "圆齿根")],
            options_from: "",
            min: 0.0,
            step: 0.0,
            required: false,
        },
    ],
    note: "精简版：只列基本参数 + 公法线，不含任何公差列；需要公差请用「GB 花键参数表（外）」。",
    missing_note: "精简卡面固定不含公差（上/下偏差）；等级/配合只用于 Wn 计算（点项标题可看口径）。",
};

// ── 块几何（照 GB 模板风格 + 去掉公差整列）───────────────────────────────

fn text_ent(value: &str, x: f64, y: f64, h: f64, wf: f64) -> EntityType {
    let mut t = Text::with_value(value, Vector3::new(x, y, 0.0));
    t.height = h;
    t.width_factor = wf;
    t.style = "OCSM_GB".into();
    t.common.layer = LAYER_TEXT.to_string();
    t.common.color = Color::ByLayer;
    t.common.linetype = "ByLayer".into();
    EntityType::Text(t)
}

fn attdef(tag: &str, x: f64, y: f64) -> AttributeDefinition {
    let mut ad = AttributeDefinition::new(tag.to_string(), String::new(), " ".to_string());
    ad.insertion_point = Vector3::new(x, y, 0.0);
    ad.alignment_point = ad.insertion_point;
    ad.height = TEXT_H;
    ad.width_factor = VALUE_WF;
    ad.text_style = "OCSM_GB".into();
    ad.flags.preset = true;
    ad.common.layer = LAYER_TEXT.to_string();
    ad
}

/// 数据行 i（1 起）顶部 y。
fn row_top(i: usize) -> f64 {
    -(i as f64) * ROW
}

/// 标签基线 y（照模板 row1 = −12.74995）。
fn label_y(i: usize) -> f64 {
    row_top(i) - 5.852_141_401_252_116
}

/// 值基线 y（照模板 row1 = −12.19744）。
fn value_y(i: usize) -> f64 {
    label_y(i) + 0.552_51
}

/// 标题（内/外）。
fn title_text(side: SplineSide) -> &'static str {
    match side {
        SplineSide::Internal => "内 花 键 参 数 表（精简）",
        SplineSide::External => "外 花 键 参 数 表（精简）",
    }
}

fn title_x(side: SplineSide) -> f64 {
    match side {
        SplineSide::Internal => -51.288_982_774_652_794,
        SplineSide::External => -51.420_189_287_103_995,
    }
}

/// 精简卡块成员：左框/右框/上下框 + 9 行分隔 + 1 分栏线 = 14 线；
/// 标题 + 9 标签 = 10 文字；9 个 ATTDEF。**无公差列图元/文字/属性**。
pub fn block_entities(side: SplineSide) -> Vec<EntityType> {
    // 标题行 + DATA_ROWS 数据行 = 10 行；底部 = −10·ROW。
    let bottom = -((DATA_ROWS as f64) + 1.0) * ROW;
    let mut out = Vec::with_capacity(14 + 10 + DATA_ROWS);
    let mut line = |a: [f64; 2], b: [f64; 2], layer: &str| out.push(crate::partgen_kit::line(a, b, layer));
    line([X_LEFT, 0.0], [X_LEFT, bottom], LAYER_OUTLINE);
    line([X_RIGHT, 0.0], [X_RIGHT, bottom], LAYER_OUTLINE);
    line([X_LEFT, 0.0], [X_RIGHT, 0.0], LAYER_OUTLINE);
    line([X_LEFT, bottom], [X_RIGHT, bottom], LAYER_OUTLINE);
    for i in 1..=DATA_ROWS {
        line([X_LEFT, row_top(i)], [X_RIGHT, row_top(i)], LAYER_THIN);
    }
    line([X_SEP, -ROW], [X_SEP, bottom], LAYER_THIN);
    out.push(text_ent(title_text(side), title_x(side), Y_TITLE, TEXT_H, LABEL_WF));
    for (i, f) in fields(side).iter().enumerate() {
        let row = i + 1;
        out.push(text_ent(f.label, X_LABEL, label_y(row), TEXT_H, LABEL_WF));
    }
    for (i, f) in fields(side).iter().enumerate() {
        out.push(EntityType::AttributeDefinition(attdef(f.tag, X_VALUE, value_y(i + 1))));
    }
    out
}

/// 9 个 ATTDEF（顺序 = `fields`）。
pub fn attdefs(side: SplineSide) -> Vec<AttributeDefinition> {
    fields(side)
        .iter()
        .enumerate()
        .map(|(i, f)| attdef(f.tag, X_VALUE, value_y(i + 1)))
        .collect()
}

// ── 取值（与 GB 卡同一份 `spline_tol::compute()`；本模块不另算）────────────

fn root_label(root: RootForm) -> &'static str {
    match root {
        RootForm::Flat => "平齿根",
        RootForm::Fillet => "圆齿根",
    }
}

/// 9 个 tag 的值（顺序与 `attdefs()` 一致）。只读 `SplineTable` 既有字段。
pub fn values(side: SplineSide, table: &SplineTable) -> Result<Vec<(String, String)>, String> {
    let s = crate::spline_table::fmt_mm;
    let vals: Vec<(&'static str, String)> = match (side, table) {
        (SplineSide::Internal, SplineTable::Internal(t)) => vec![
            ("(简)执行标准", "GB/T 3478.1-2008".to_string()),
            ("(简)模数", s(t.m)),
            ("(简)齿数", t.z.to_string()),
            ("(简)齿形角", crate::spline_table::fmt_deg(t.alpha_deg)),
            ("(简)齿根样式", root_label(t.root).to_string()),
            ("(简)大径", s(t.major_dia)),
            ("(简)小径", s(t.minor_dia)),
            ("(简)量棒直径", s(t.dp)),
            ("(简)测量跨棒距", s(t.md)),
        ],
        (SplineSide::External, SplineTable::External(t)) => vec![
            ("(简)执行标准", "GB/T 3478.1-2008".to_string()),
            ("(简)模数", s(t.m)),
            ("(简)齿数", t.z.to_string()),
            ("(简)齿形角", crate::spline_table::fmt_deg(t.alpha_deg)),
            ("(简)齿根样式", root_label(t.root).to_string()),
            ("(简)大径", s(t.major_dia)),
            ("(简)小径", s(t.minor_dia)),
            ("(简)跨测齿数", t.kn.to_string()),
            ("(简)公法线长度", s(t.wn)),
        ],
        _ => return Err("GB 花键精简卡：方向与计算结果不一致".into()),
    };
    // 顺序护栏：值顺序必须与字段表（ATTDEF 顺序）一致。
    for (f, (tag, _)) in fields(side).iter().zip(&vals) {
        debug_assert_eq!(f.tag, *tag, "精简卡字段表与取值顺序不一致");
    }
    Ok(vals
        .into_iter()
        .map(|(tag, v)| (tag.to_string(), v))
        .collect())
}

/// 预览 9 项（label/单位/公式/来源来自字段表，值同一份 `values()`）。
pub fn items_json(side: SplineSide, table: &SplineTable) -> Result<Vec<serde_json::Value>, String> {
    let vals = values(side, table)?;
    let mut out = Vec::with_capacity(vals.len());
    for (f, (tag, value)) in fields(side).iter().zip(vals) {
        debug_assert_eq!(f.tag, tag);
        out.push(serde_json::json!({
            "tag": tag,
            "label": f.label,
            "unit": f.unit,
            "value": value,
            "formula": f.formula,
            "source": f.source,
        }));
    }
    Ok(out)
}

/// 求值用的 `INSERT`（与 GB 卡同一条构造路径：几何一次、值一次）。
pub fn build_insert(
    side: SplineSide,
    table: &SplineTable,
    at: [f64; 2],
    rot_deg: f64,
) -> Result<Insert, String> {
    let values = values(side, table)?;
    let mut ins = Insert::new(block_name(side), Vector3::new(at[0], at[1], 0.0));
    ins.rotation = rot_deg.to_radians();
    {
        let c = &mut ins.common;
        c.layer = crate::partgen::LAYER_MAIN.to_string();
        c.color = Color::ByLayer;
        c.linetype = "ByLayer".to_string();
        c.line_weight = LineWeight::ByLayer;
    }
    for ad in attdefs(side).iter() {
        let val = values
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

/// 待放置件要带的 9 个 ATTRIB。
pub fn pending_attrs(
    side: SplineSide,
    table: &SplineTable,
) -> Result<Vec<(AttributeDefinition, String)>, String> {
    let values = values(side, table)?;
    let mut out = Vec::with_capacity(values.len());
    for ad in attdefs(side) {
        let v = values
            .iter()
            .find(|(tag, _)| tag == &ad.tag)
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        out.push((ad, v));
    }
    Ok(out)
}

/// GUI 预览 JSON（与 `/api/card_preview` 同形；表达式反解复用统一解析器）。
pub fn preview_json(model: &crate::spline_gui::SplineTableModel) -> Result<serde_json::Value, String> {
    let (input, gear) = model.resolved()?;
    let table = crate::spline_tol::compute(&input)?;
    let items = items_json(input.side, &table)?;
    let side = input.side;
    let (card, side_label) = match side {
        SplineSide::Internal => ("GB花键精简表_内", "内花键"),
        SplineSide::External => ("GB花键精简表_外", "外花键"),
    };
    let expr_echo = model
        .expr
        .trim()
        .to_string();
    let root = crate::spline_gui::root_id(input.root);
    let readout = serde_json::json!([
        {"k": "模数 m", "v": crate::spline_table::fmt_mm(input.m)},
        {"k": "齿数 z", "v": input.z.to_string()},
        {"k": "齿形角 αD", "v": crate::spline_table::fmt_deg(input.alpha.deg())},
        {"k": "齿根样式", "v": root_label(input.root)},
        {"k": "方向", "v": side_label},
    ]);
    let fit_code = match side {
        SplineSide::Internal => "H",
        SplineSide::External => input.ext_dev.code(),
    };
    let fields = serde_json::json!({
        "grade": input.grade,
        "fit": fit_code,
        "root": root,
    });
    Ok(serde_json::json!({
        "ok": true,
        "card": card,
        "renderer": "spline_lite",
        "title": format!("GB/T 3478 {side_label}参数表（精简版）"),
        "expr": expr_echo,
        "fields": fields,
        "x": gear.x,
        "side": crate::spline_gui::side_id(side),
        "side_label": side_label,
        "readout": readout,
        "items": items,
        "missing": [],
        "missing_note": "精简版卡片：固定只列基本参数与主要测量量，不含任何公差（上/下偏差）列；\
                         等级/配合只用于测量量计算。需要公差明细请用「GB 花键参数表」。",
    }))
}

/// 待放置件的 `OCSM_PART` 元数据（薄台账；卡类型/方向/等级配合/Dp）。
pub fn part_meta_json(
    side: SplineSide,
    input: &crate::spline_tol::SplineInput,
    dp: Option<f64>,
) -> String {
    serde_json::json!({
        "family": "spline_lite",
        "card": match side {
            SplineSide::Internal => "GB花键精简表_内",
            SplineSide::External => "GB花键精简表_外",
        },
        "side": crate::spline_gui::side_id(side),
        "grade_fit": input.grade_fit_label(),
        "dp": dp,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spline_tol::{ExtDev, PressureAngle, SplineInput};

    fn make_input(
        side: SplineSide,
        m: f64,
        z: u32,
        alpha: PressureAngle,
        root: RootForm,
    ) -> SplineInput {
        SplineInput {
            m,
            z,
            alpha,
            root,
            side,
            grade: 6,
            ext_dev: if side == SplineSide::Internal {
                ExtDev::H
            } else {
                ExtDev::H
            },
            fit_length: None,
            dp: None,
        }
    }

    fn text_values(ents: &[EntityType]) -> Vec<String> {
        ents.iter()
            .filter_map(|e| match e {
                EntityType::Text(t) => Some(t.value.clone()),
                EntityType::MText(m) => Some(m.value.clone()),
                _ => None,
            })
            .collect()
    }

    /// 负断言 + 图元数：9 行基本项，**卡面无任何公差类字段/文字**。
    #[test]
    fn lite_block_has_no_tolerance_column() {
        for side in [SplineSide::Internal, SplineSide::External] {
            let ents = block_entities(side);
            let lines = ents
                .iter()
                .filter(|e| matches!(e, EntityType::Line(_)))
                .count();
            let texts = text_values(&ents);
            let atts = attdefs(side);
            assert_eq!(lines, 14, "{side:?} 线数（4 框 + 9 分隔 + 1 分栏）");
            assert_eq!(texts.len(), 10, "{side:?} 文字数（标题 + 9 标签）");
            assert_eq!(atts.len(), DATA_ROWS, "{side:?} ATTDEF 数");
            // 块底部必须到第 10 行（标题 + 9 数据行），否则最后一行会出框。
            let min_y = ents
                .iter()
                .filter_map(|e| match e {
                    EntityType::Line(l) => Some(l.start.y.min(l.end.y)),
                    _ => None,
                })
                .fold(f64::INFINITY, f64::min);
            assert!(
                (min_y + ((DATA_ROWS as f64) + 1.0) * ROW).abs() < 1e-9,
                "{side:?} 块底应在 −10·ROW（含标题行），实 {min_y}"
            );
            for bad in ["公差", "偏差", "上差", "下差"] {
                assert!(
                    !texts.iter().any(|t| t.contains(bad)),
                    "{side:?} 卡面文字不应出现「{bad}」：{texts:?}"
                );
                assert!(
                    !atts.iter().any(|a| a.tag.contains(bad)),
                    "{side:?} ATTDEF tag 不应出现「{bad}」"
                );
            }
        }
    }

    /// 与既有 GB 卡同一批 `compute()` 值：精简项逐项等于 GB 卡对应项。
    #[test]
    fn lite_values_equal_gb_card_items_same_compute() {
        for (side, input) in [
            (
                SplineSide::Internal,
                make_input(SplineSide::Internal, 3.0, 20, PressureAngle::A30, RootForm::Flat),
            ),
            (
                SplineSide::External,
                make_input(SplineSide::External, 3.0, 20, PressureAngle::A30, RootForm::Flat),
            ),
        ] {
            let table = crate::spline_tol::compute(&input).unwrap();
            let gb: std::collections::HashMap<String, String> =
                crate::spline_table::values(side, &table).unwrap().into_iter().collect();
            let lite: std::collections::HashMap<String, String> =
                values(side, &table).unwrap().into_iter().collect();
            let pairs: &[(&str, &str)] = &[
                ("(简)模数", "模数"),
                ("(简)齿数", "齿数"),
                ("(简)齿形角", "齿形角"),
                ("(简)大径", "大径"),
                ("(简)小径", "小径"),
            ];
            for (lite_tag, gb_key) in pairs {
                let gb_tag = format!(
                    "({}){gb_key}",
                    if side == SplineSide::Internal { "内" } else { "外" }
                );
                assert_eq!(
                    lite[*lite_tag], gb[gb_tag.as_str()],
                    "{side:?} 精简 {lite_tag} 应等于 GB 卡 {gb_tag}"
                );
            }
            match side {
                SplineSide::Internal => {
                    assert_eq!(lite["(简)量棒直径"], gb["(内)量棒直径"]);
                    assert_eq!(lite["(简)测量跨棒距"], gb["(内)测量跨棒距"]);
                }
                SplineSide::External => {
                    assert_eq!(lite["(简)跨测齿数"], gb["(外)跨测齿数"]);
                    assert_eq!(lite["(简)公法线长度"], gb["(外)公法线长度"]);
                }
            }
            assert_eq!(lite["(简)执行标准"], "GB/T 3478.1-2008");
            assert_eq!(lite["(简)齿根样式"], "平齿根");
        }
    }

    /// 几何：标签不出左框/标签栏，值不出右框；标签与值不叠、行与行不叠。
    #[test]
    fn lite_texts_stay_in_columns_and_do_not_overlap() {
        let text_box = crate::spline_table::text_box;
        for side in [SplineSide::Internal, SplineSide::External] {
            let fields = fields(side);
            // 取一组大值（m=10、z=100）压最宽文本。
            let input = make_input(side, 10.0, 100, PressureAngle::A30, RootForm::Flat);
            let table = crate::spline_tol::compute(&input).unwrap();
            let vals = values(side, &table).unwrap();
            for (i, f) in fields.iter().enumerate() {
                let row = i + 1;
                let lab = text_box([X_LABEL, label_y(row)], TEXT_H, LABEL_WF, f.label);
                assert!(
                    lab[0] >= X_LEFT && lab[2] <= X_SEP,
                    "{side:?} 标签「{}」应在本列内：{lab:?}",
                    f.label
                );
                let val = &vals[i].1;
                let vb = text_box([X_VALUE, value_y(row)], TEXT_H, VALUE_WF, val);
                assert!(
                    vb[0] >= X_SEP && vb[2] <= X_RIGHT,
                    "{side:?} 值「{val}」应在本列内（不越右框、不进公差列）：{vb:?}"
                );
                assert!(
                    lab[2] <= vb[0],
                    "{side:?} 标签「{}」与值「{val}」不得横向相叠",
                    f.label
                );
                let top = row_top(row);
                let bottom = top - ROW;
                assert!(
                    lab[1] >= bottom && lab[3] <= top && vb[1] >= bottom && vb[3] <= top,
                    "{side:?} 第 {row} 行文字应在本行带内"
                );
            }
        }
    }

    /// 表达式反解 + 预览 JSON：9 项、回填 fields（root/grade/fit）、无公差项。
    #[test]
    fn lite_preview_reverse_solves_expression() {
        let model = crate::spline_gui::SplineTableModel {
            side: "int".into(),
            grade: 6,
            fit: "H".into(),
            expr: "SPLINE IN M3 Z20 ALPHA30 X0 DA65.4 DF57.3436 BETA0 H30".into(),
            root: Some("auto".into()),
            dp: None,
            at: None,
            rot: 0.0,
        };
        let v = preview_json(&model).unwrap();
        assert_eq!(v["renderer"], "spline_lite");
        assert_eq!(v["card"], "GB花键精简表_内");
        let items = v["items"].as_array().unwrap();
        assert_eq!(items.len(), DATA_ROWS);
        let get = |label: &str| {
            items
                .iter()
                .find(|it| it["label"] == label)
                .map(|it| it["value"].as_str().unwrap().to_string())
                .unwrap()
        };
        assert_eq!(get("模数 m"), "3");
        assert_eq!(get("齿数 z"), "20");
        assert_eq!(get("齿形角 αD"), "30°");
        assert_eq!(v["fields"]["root"], "fillet", "DA65.4 → 圆齿根反解");
        assert_eq!(v["fields"]["grade"], 6);
        assert_eq!(v["fields"]["fit"], "H");
        assert!(!items.iter().any(|it| {
            let t = it["tag"].as_str().unwrap();
            let l = it["label"].as_str().unwrap();
            t.contains("公差") || l.contains("公差") || t.contains("偏差")
        }));
    }
}

