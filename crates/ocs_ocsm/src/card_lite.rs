//! 智能卡片「**精简版**」共享引擎（NF / DIN / ANSI / 齿轮；用户 2026-09-27 点单）。
//!
//! 用户口径：与 GB 花键精简版同义 —— **只列基本参数 + 主要测量量**、**整列公差去掉**；
//! 一卡一方向；GUI 统一骨架（通用表单，字段清单来自各完整卡）；表达式反解/查表/公式
//! **全部复用既有完整卡**（NF / DIN / ANSI / 齿轮各自的 `values()` 与 `compute()`）。
//!
//! ## 表驱动（一行一卡）
//! 9 张精简卡 = [`LITE_CARDS`] 的 9 行：[`LiteCardSpec`]（块名/标题/完整卡 id/字段清单）。
//! 每张卡的字段 = [`LiteFieldSpec`] 清单（tag/label/全卡项 key/字宽）；**不复制任何计算**：
//! 取值先走完整卡预览（`guide_server::apply_card_preview`，卡 id 换成完整卡），
//! 再按 `key` 投影子集；齿轮「分度圆/齿顶圆/齿根圆」由同一 `GearParams` 补算。
//! 版面 = GB 精简版同一骨架（[`crate::spline_lite`] 的坐标/行高/线层/`OCSM_GB` 常量），
//! 只是行数随字段数变化、公差列不存在。
//!
//! ## 同源/断言
//! 预览项值与完整卡同一次计算逐项相等（单测）；计算书走同一 `preview_json`；
//! 文字几何一律用真字体 metrics（`spline_table::text_box_ttf`，cap-height 归一 ×1.57）。

use crate::spline_lite::{
    attdef_wf, label_y, row_top, text_ent, value_y, LAYER_OUTLINE, LAYER_THIN, ROW, TEXT_H,
    TITLE_WF, VALUE_WF, X_LABEL, X_LEFT, X_RIGHT, X_SEP, X_VALUE, Y_TITLE,
};
use ocs_plugin_api::host::acadrust::entities::{
    AttributeDefinition, AttributeEntity, Entity as _, EntityType, Insert,
};
use ocs_plugin_api::host::acadrust::types::{Color, LineWeight, Vector3};

/// 一行精简项（一卡一表；tag 进块 ATTDEF，key 指完整卡项 tag）。
#[derive(Debug, Clone, Copy)]
pub struct LiteFieldSpec {
    /// 精简块 ATTDEF tag（带体系前缀，避免与完整卡块混淆）。
    pub tag: &'static str,
    /// 完整卡项 tag（取值来源）；`""` = 由同一引擎补算（齿轮三圆）。
    pub key: &'static str,
    /// 卡面标签（精简口径；可短于完整卡标签）。
    pub label: &'static str,
    /// 实体级字宽（按真字体 metrics 预算，留 ≥3% 列内余量）。
    pub wf: f64,
    /// 值 ATTDEF 的实体级字宽（默认 [`VALUE_WF`]；长英文值如 ANSI「花键类型」需再压缩）。
    pub vwf: f64,
    /// 补算项的公式/口径（key=="" 时用）。
    pub formula: &'static str,
    /// 补算项的来源（key=="" 时用）。
    pub source: &'static str,
}

/// 体系族（取值/补算分派；不是每张卡一套代码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteFamily {
    NfInt,
    NfExt,
    DinInt,
    DinExt,
    AnsiCnInt,
    AnsiCnExt,
    AnsiEnInt,
    AnsiEnExt,
    Gear,
}

/// 一张精简卡（一行一卡定义）。
#[derive(Debug, Clone, Copy)]
pub struct LiteCardSpec {
    /// 稳定 id（= `CARD_TYPES` 的 id）。
    pub id: &'static str,
    /// 块名（一套=一卡一块；有 at 直插 / 无 at 待放置件都用它）。
    pub block: &'static str,
    /// 块内标题（表心标题，非卡名）。
    pub title: &'static str,
    /// 完整卡 id（取值/校验/计算书的唯一计算来源）。
    pub full_card: &'static str,
    pub family: LiteFamily,
    pub fields: &'static [LiteFieldSpec],
}

macro_rules! fld {
    ($tag:literal, $key:literal, $label:literal, $wf:literal) => {
        LiteFieldSpec {
            tag: $tag,
            key: $key,
            label: $label,
            wf: $wf,
            vwf: VALUE_WF,
            formula: "",
            source: "",
        }
    };
}

// ── NF 内花键（p18 拉削内花键；用户例：执行标准·定心方式·m·z·a·齿根样式·加工方法·大径·小径 + V/G）──
const NF_INT_FIELDS: &[LiteFieldSpec] = &[
    fld!("(NF简)执行标准", "执行标准", "执行标准", 1.0),
    fld!("(NF简)定心方式", "定心方式", "定心方式", 1.0),
    fld!("(NF简)模数", "模数", "模数 m", 1.0),
    fld!("(NF简)齿数", "齿数", "齿数 z", 1.0),
    fld!("(NF简)压力角", "压力角", "压力角 a", 1.0),
    fld!("(NF简)齿根样式", "齿根样式", "齿根样式", 1.0),
    fld!("(NF简)加工方法", "加工方法", "加工方法", 1.0),
    fld!("(NF简)大径", "大径Az", "大径 Az", 1.0),
    fld!("(NF简)小径", "小径D", "小径 D", 1.0),
    fld!("(NF简)量棒直径", "量棒直径V", "量棒直径 V", 0.90),
    fld!("(NF简)跨棒距", "跨棒距G", "跨棒距 G", 1.0),
];

// ── NF 外花键（p20–p22 滚齿外花键；末两项 K/W）──
const NF_EXT_FIELDS: &[LiteFieldSpec] = &[
    fld!("(NF简)执行标准", "执行标准", "执行标准", 1.0),
    fld!("(NF简)定心方式", "定心方式", "定心方式", 1.0),
    fld!("(NF简)模数", "模数", "模数 m", 1.0),
    fld!("(NF简)齿数", "齿数", "齿数 z", 1.0),
    fld!("(NF简)压力角", "压力角", "压力角 a", 1.0),
    fld!("(NF简)齿根样式", "齿根样式", "齿根样式", 1.0),
    fld!("(NF简)加工方法", "加工方法", "加工方法", 1.0),
    fld!("(NF简)大径", "大径Dee", "大径 Dee", 1.0),
    fld!("(NF简)小径", "小径Die", "小径 Die", 1.0),
    fld!("(NF简)跨测齿数", "跨测齿数K", "跨测齿数 K", 0.90),
    fld!("(NF简)公法线", "公法线W", "公法线 W", 1.0),
];

// ── DIN 5480（Bild 6；标记 + z/m/α + 关键直径/量圆；按 DIN 自身口径，不照搬 GB）──
const DIN_INT_FIELDS: &[LiteFieldSpec] = &[
    fld!("(DIN简)标记", "N标记", "标记 N", 1.0),
    fld!("(DIN简)齿数", "N齿数", "齿数 z", 1.0),
    fld!("(DIN简)模数", "N模数", "模数 m", 1.0),
    fld!("(DIN简)压力角", "N压力角", "压力角 α", 1.0),
    fld!("(DIN简)齿根圆", "N齿根圆", "齿根圆 d_f2", 0.85),
    fld!("(DIN简)齿根成形圆", "N齿根成形圆", "齿根成形圆 d_Ff2", 0.60),
    fld!("(DIN简)齿顶圆", "N齿顶圆", "齿顶圆 d_a2", 0.80),
    fld!("(DIN简)量圆", "N量圆", "量圆 D_M", 1.0),
    fld!("(DIN简)量距max", "N量距max", "量圆距 M2_max", 0.65),
    fld!("(DIN简)量距min", "N量距min", "量圆距 M2_min", 0.65),
];

const DIN_EXT_FIELDS: &[LiteFieldSpec] = &[
    fld!("(DIN简)标记", "W标记", "标记 W", 1.0),
    fld!("(DIN简)齿数", "W齿数", "齿数 z", 1.0),
    fld!("(DIN简)模数", "W模数", "模数 m", 1.0),
    fld!("(DIN简)压力角", "W压力角", "压力角 α", 1.0),
    fld!("(DIN简)齿顶圆", "W齿顶圆", "齿顶圆 d_a1", 0.85),
    fld!("(DIN简)齿根成形圆", "W齿根成形圆", "齿根成形圆 d_Ff1", 0.60),
    fld!("(DIN简)齿根圆", "W齿根圆", "齿根圆 d_f1", 0.85),
    fld!("(DIN简)量圆", "W量圆", "量圆 D_M", 1.0),
    fld!("(DIN简)量距max", "W量距max", "量圆距 M1_max", 0.65),
    fld!("(DIN简)量距min", "W量距min", "量圆距 M1_min", 0.65),
];

// ── ANSI B92.1（内/外 × 中/英 四张：基本参数 + 主要测量量）──
const ANSI_CN_INT_FIELDS: &[LiteFieldSpec] = &[
    LiteFieldSpec { tag: "(ANSI简)花键类型", key: "花键类型", label: "花键类型", wf: 1.0, vwf: 0.60, formula: "", source: "" },
    fld!("(ANSI简)齿数", "齿数", "齿数 z", 1.0),
    fld!("(ANSI简)径节", "径节", "径节 P", 1.0),
    fld!("(ANSI简)压力角", "压力角", "压力角 α", 1.0),
    fld!("(ANSI简)基圆直径", "基圆直径", "基圆直径 Db", 0.80),
    fld!("(ANSI简)节圆直径", "节圆直径", "节圆直径 D", 0.90),
    fld!("(ANSI简)大径", "大径", "大径", 1.0),
    fld!("(ANSI简)小径", "小径", "小径", 1.0),
    fld!("(ANSI简)跨棒距", "跨棒距", "跨棒距 M", 1.0),
    fld!("(ANSI简)量棒直径", "量棒直径", "量棒直径 Dp", 0.80),
];

const ANSI_CN_EXT_FIELDS: &[LiteFieldSpec] = &[
    LiteFieldSpec { tag: "(ANSI简)花键类型", key: "花键类型", label: "花键类型", wf: 1.0, vwf: 0.60, formula: "", source: "" },
    fld!("(ANSI简)齿数", "齿数", "齿数 z", 1.0),
    fld!("(ANSI简)径节", "径节", "径节 P", 1.0),
    fld!("(ANSI简)压力角", "压力角", "压力角 α", 1.0),
    fld!("(ANSI简)基圆直径", "基圆直径", "基圆直径 Db", 0.80),
    fld!("(ANSI简)节圆直径", "节圆直径", "节圆直径 D", 0.90),
    fld!("(ANSI简)大径", "大径", "大径", 1.0),
    fld!("(ANSI简)小径", "小径", "小径", 1.0),
    fld!("(ANSI简)公法线", "公法线长度", "公法线 W", 1.0),
    fld!("(ANSI简)跨测齿数", "跨测齿数", "跨测齿数 K", 0.90),
];

const ANSI_EN_INT_FIELDS: &[LiteFieldSpec] = &[
    LiteFieldSpec { tag: "(ANSI简)花键类型", key: "花键类型", label: "SPLINE TYPE", wf: 0.75, vwf: 0.45, formula: "", source: "" },
    fld!("(ANSI简)齿数", "齿数", "TEETH z", 1.0),
    fld!("(ANSI简)径节", "径节", "PITCH P", 1.0),
    fld!("(ANSI简)压力角", "压力角", "ALPHA", 1.0),
    fld!("(ANSI简)基圆直径", "基圆直径", "BASE DIA. Db", 0.75),
    fld!("(ANSI简)节圆直径", "节圆直径", "PITCH DIA. D", 0.75),
    fld!("(ANSI简)大径", "大径", "MAJOR DIA.", 0.80),
    fld!("(ANSI简)小径", "小径", "MINOR DIA.", 0.80),
    fld!("(ANSI简)跨棒距", "跨棒距", "PIN DIST. M", 0.80),
    fld!("(ANSI简)量棒直径", "量棒直径", "PIN DIA. Dp", 0.80),
];

const ANSI_EN_EXT_FIELDS: &[LiteFieldSpec] = &[
    LiteFieldSpec { tag: "(ANSI简)花键类型", key: "花键类型", label: "SPLINE TYPE", wf: 0.75, vwf: 0.45, formula: "", source: "" },
    fld!("(ANSI简)齿数", "齿数", "TEETH z", 1.0),
    fld!("(ANSI简)径节", "径节", "PITCH P", 1.0),
    fld!("(ANSI简)压力角", "压力角", "ALPHA", 1.0),
    fld!("(ANSI简)基圆直径", "基圆直径", "BASE DIA. Db", 0.75),
    fld!("(ANSI简)节圆直径", "节圆直径", "PITCH DIA. D", 0.75),
    fld!("(ANSI简)大径", "大径", "MAJOR DIA.", 0.80),
    fld!("(ANSI简)小径", "小径", "MINOR DIA.", 0.80),
    fld!("(ANSI简)公法线", "公法线长度", "BASE TANG. W", 0.65),
    fld!("(ANSI简)跨测齿数", "跨测齿数", "SPAN TEETH K", 0.65),
];

// ── 齿轮（GB/T 10095；模数·齿数·压力角·变位·三圆 + 公法线/跨齿数）──
const GEAR_FIELDS: &[LiteFieldSpec] = &[
    fld!("(齿轮简)模数", "法向模数", "模数 m", 1.0),
    fld!("(齿轮简)齿数", "齿数", "齿数 z", 1.0),
    fld!("(齿轮简)压力角", "齿形角", "压力角 α", 1.0),
    fld!("(齿轮简)变位", "径向变位系数", "变位系数 x", 0.90),
    LiteFieldSpec {
        tag: "(齿轮简)分度圆",
        key: "",
        label: "分度圆 d",
        wf: 1.0,
        vwf: VALUE_WF,
        formula: "d = m·z（与齿轮卡同一 `GearParams`）",
        source: "齿轮引擎 `GearParams::d()`",
    },
    LiteFieldSpec {
        tag: "(齿轮简)齿顶圆",
        key: "",
        label: "齿顶圆 da",
        wf: 1.0,
        vwf: VALUE_WF,
        formula: "da = d + 2m(ha* + x)（外齿；内齿引擎口径）",
        source: "齿轮引擎 `GearParams::da()`",
    },
    LiteFieldSpec {
        tag: "(齿轮简)齿根圆",
        key: "",
        label: "齿根圆 df",
        wf: 1.0,
        vwf: VALUE_WF,
        formula: "df = d − 2m(ha* + c* − x)（外齿；内齿引擎口径）",
        source: "齿轮引擎 `GearParams::df()`",
    },
    fld!("(齿轮简)公法线", "公法线", "公法线 W", 1.0),
    fld!("(齿轮简)跨齿数", "公法线K", "跨齿数 K", 1.0),
];

/// 9 张精简卡（顺序 = GUI 下拉「精简版」分组顺序；GB 精简两卡见 `spline_lite.rs`）。
pub const LITE_CARDS: &[LiteCardSpec] = &[
    LiteCardSpec {
        id: "NF花键精简表_内",
        block: "OCSM_LITE_NF_INT",
        title: "NF 内花键参数表（精简）",
        full_card: "NF内花键参数表",
        family: LiteFamily::NfInt,
        fields: NF_INT_FIELDS,
    },
    LiteCardSpec {
        id: "NF花键精简表_外",
        block: "OCSM_LITE_NF_EXT",
        title: "NF 外花键参数表（精简）",
        full_card: "NF外花键参数表",
        family: LiteFamily::NfExt,
        fields: NF_EXT_FIELDS,
    },
    LiteCardSpec {
        id: "DIN花键精简表_内",
        block: "OCSM_LITE_DIN_INT",
        title: "DIN 内花键参数表（精简）",
        full_card: "DIN花键参数表",
        family: LiteFamily::DinInt,
        fields: DIN_INT_FIELDS,
    },
    LiteCardSpec {
        id: "DIN花键精简表_外",
        block: "OCSM_LITE_DIN_EXT",
        title: "DIN 外花键参数表（精简）",
        full_card: "DIN花键参数表_外",
        family: LiteFamily::DinExt,
        fields: DIN_EXT_FIELDS,
    },
    LiteCardSpec {
        id: "ANSI花键精简表_内_中文",
        block: "OCSM_LITE_ANSI_INT_CN",
        title: "ANSI 内花键参数表（精简）",
        full_card: "ANSI花键参数表_中文",
        family: LiteFamily::AnsiCnInt,
        fields: ANSI_CN_INT_FIELDS,
    },
    LiteCardSpec {
        id: "ANSI花键精简表_外_中文",
        block: "OCSM_LITE_ANSI_EXT_CN",
        title: "ANSI 外花键参数表（精简）",
        full_card: "ANSI花键参数表_外_中文",
        family: LiteFamily::AnsiCnExt,
        fields: ANSI_CN_EXT_FIELDS,
    },
    LiteCardSpec {
        id: "ANSI花键精简表_内_英文",
        block: "OCSM_LITE_ANSI_INT_EN",
        title: "ANSI INTERNAL SPLINE (LITE)",
        full_card: "ANSI花键参数表_英文",
        family: LiteFamily::AnsiEnInt,
        fields: ANSI_EN_INT_FIELDS,
    },
    LiteCardSpec {
        id: "ANSI花键精简表_外_英文",
        block: "OCSM_LITE_ANSI_EXT_EN",
        title: "ANSI EXTERNAL SPLINE (LITE)",
        full_card: "ANSI花键参数表_外_英文",
        family: LiteFamily::AnsiEnExt,
        fields: ANSI_EN_EXT_FIELDS,
    },
    LiteCardSpec {
        id: "齿轮精简表",
        block: "OCSM_LITE_GEAR",
        title: "齿轮参数表（精简）",
        full_card: "齿轮参数表",
        family: LiteFamily::Gear,
        fields: GEAR_FIELDS,
    },
];

/// 按卡 id 找精简卡定义。
pub fn by_id(id: &str) -> Option<&'static LiteCardSpec> {
    LITE_CARDS.iter().find(|c| c.id == id)
}

// ── 块几何（与 GB 精简版同一骨架；行数随字段数）─────────────────────────────

/// 数据行顶部 y（第 i 行 1 起；标题行在 0..−ROW）。
fn data_bottom(card: &LiteCardSpec) -> f64 {
    -((card.fields.len() as f64) + 1.0) * ROW
}

/// 精简卡块成员：4 框 + N 行分隔 + 1 分栏 = N+5 线；标题 + N 标签 = N+1 文字；N ATTDEF。
/// **无公差列图元/文字/属性**。
pub fn block_entities(card: &LiteCardSpec) -> Vec<EntityType> {
    let n = card.fields.len();
    let bottom = data_bottom(card);
    let mut out = Vec::with_capacity(n + 5 + n + 1 + n);
    let mut line = |a: [f64; 2], b: [f64; 2], layer: &str| out.push(crate::partgen_kit::line(a, b, layer));
    line([X_LEFT, 0.0], [X_LEFT, bottom], LAYER_OUTLINE);
    line([X_RIGHT, 0.0], [X_RIGHT, bottom], LAYER_OUTLINE);
    line([X_LEFT, 0.0], [X_RIGHT, 0.0], LAYER_OUTLINE);
    line([X_LEFT, bottom], [X_RIGHT, bottom], LAYER_OUTLINE);
    for i in 1..=n {
        line([X_LEFT, row_top(i)], [X_RIGHT, row_top(i)], LAYER_THIN);
    }
    line([X_SEP, -ROW], [X_SEP, bottom], LAYER_THIN);
    // 标题左对齐在标签列（自定卡无模板；不与列线/标签相叠）。
    out.push(text_ent(card.title, X_LABEL, Y_TITLE, TEXT_H, TITLE_WF));
    for (i, f) in card.fields.iter().enumerate() {
        out.push(text_ent(f.label, X_LABEL, label_y(i + 1), TEXT_H, f.wf));
    }
    for (i, f) in card.fields.iter().enumerate() {
        out.push(EntityType::AttributeDefinition(attdef_wf(
            f.tag,
            X_VALUE,
            value_y(i + 1),
            f.vwf,
        )));
    }
    out
}

/// N 个 ATTDEF（顺序 = `fields`）。
pub fn attdefs(card: &LiteCardSpec) -> Vec<AttributeDefinition> {
    card.fields
        .iter()
        .enumerate()
        .map(|(i, f)| attdef_wf(f.tag, X_VALUE, value_y(i + 1), f.vwf))
        .collect()
}

// ── 取值（完整卡同源投影）────────────────────────────────────────────────────

/// 完整卡预览 JSON（把请求的 card 换成完整卡 id → 走既有 renderer）。
fn full_preview(card: &LiteCardSpec, model: &serde_json::Value) -> Result<serde_json::Value, String> {
    let mut v = model.clone();
    v["card"] = serde_json::Value::String(card.full_card.to_string());
    let bytes = serde_json::to_vec(&v)
        .map_err(|e| {
            crate::i18n::t_fmt(
                "cmd.cardlite.err.serialize",
                &[("id", card.id), ("e", &e.to_string())],
            )
        })?;
    let s = crate::guide_server::apply_card_preview(&bytes)
        .map_err(|e| crate::i18n::t_fmt("cmd.cardlite.err.preview", &[("id", card.id), ("e", &e)]))?;
    serde_json::from_str(&s).map_err(|e| {
        crate::i18n::t_fmt(
            "cmd.cardlite.err.preview_parse",
            &[("id", card.id), ("e", &e.to_string())],
        )
    })
}

/// 完整卡项 tag → 值。
fn full_map(full: &serde_json::Value) -> std::collections::HashMap<String, String> {
    full["items"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|it| {
                    Some((
                        it["tag"].as_str()?.to_string(),
                        it["value"].as_str().unwrap_or("").to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 齿轮补算项（分度圆/齿顶圆/齿根圆；同一 `GearParams`）。
pub fn gear_extras_from_params(p: &crate::gear::GearParams) -> Vec<(String, String)> {
    let fmt = crate::gear_table::fmt_mm;
    vec![
        ("(齿轮简)分度圆".to_string(), fmt(p.d())),
        ("(齿轮简)齿顶圆".to_string(), fmt(p.da())),
        ("(齿轮简)齿根圆".to_string(), fmt(p.df())),
    ]
}

fn gear_extras(model: &serde_json::Value) -> Result<Vec<(String, String)>, String> {
    let m: crate::gear_table::GearTableModel = serde_json::from_value(model.clone())
        .map_err(|e| {
            crate::i18n::t_fmt("cmd.cardlite.err.gear_model", &[("e", &e.to_string())])
        })?;
    let spec = m.spec()?;
    Ok(gear_extras_from_params(&spec.params))
}

/// CLI 路径：按卡族解析 args（复用完整卡 `Spec::parse`）→（完整卡值, 补算项, at, rot）。
pub fn cli_values(
    card: &LiteCardSpec,
    args: &str,
) -> Result<(Vec<(String, String)>, Vec<(String, String)>, Option<[f64; 2]>, f64), String> {
    use crate::card_lite::LiteFamily;
    match card.family {
        LiteFamily::NfInt => {
            let s = crate::nf_table::NfTableSpec::parse(args)?;
            Ok((crate::nf_table::values(&s)?, Vec::new(), s.at, s.rot))
        }
        LiteFamily::NfExt => {
            let s = crate::nf_ext_table::NfExtTableSpec::parse(args)?;
            Ok((crate::nf_ext_table::values(&s)?, Vec::new(), s.at, s.rot))
        }
        LiteFamily::DinInt | LiteFamily::DinExt => {
            let hub = card.family == LiteFamily::DinInt;
            let s = crate::din_table::DinTableSpec::parse(args, hub)?;
            Ok((crate::din_table::values(&s, hub)?, Vec::new(), s.at, s.rot))
        }
        LiteFamily::AnsiCnInt | LiteFamily::AnsiCnExt => {
            let side = if card.family == LiteFamily::AnsiCnInt { "内" } else { "外" };
            let args = crate::inject_default_side(args, side);
            let s = crate::ansi_table::AnsiTableSpec::parse(crate::ansi_table::AnsiLang::Cn, &args)?;
            let want_int = card.family == LiteFamily::AnsiCnInt;
            if (s.side == crate::spline_tol::SplineSide::Internal) != want_int {
                return Err(crate::i18n::t_fmt(
                    "cmd.cardlite.err.dir_mismatch",
                    &[
                        ("id", card.id),
                        (
                            "side",
                            &crate::i18n::t(if want_int {
                                "cmd.cardexpr.dir.int"
                            } else {
                                "cmd.cardexpr.dir.ext"
                            }),
                        ),
                    ],
                ));
            }
            Ok((crate::ansi_table::values(&s)?, Vec::new(), s.at, s.rot))
        }
        LiteFamily::AnsiEnInt | LiteFamily::AnsiEnExt => {
            let side = if card.family == LiteFamily::AnsiEnInt { "内" } else { "外" };
            let args = crate::inject_default_side(args, side);
            let s = crate::ansi_table::AnsiTableSpec::parse(crate::ansi_table::AnsiLang::En, &args)?;
            let want_int = card.family == LiteFamily::AnsiEnInt;
            if (s.side == crate::spline_tol::SplineSide::Internal) != want_int {
                return Err(crate::i18n::t_fmt(
                    "cmd.cardlite.err.dir_mismatch",
                    &[
                        ("id", card.id),
                        (
                            "side",
                            &crate::i18n::t(if want_int {
                                "cmd.cardexpr.dir.int"
                            } else {
                                "cmd.cardexpr.dir.ext"
                            }),
                        ),
                    ],
                ));
            }
            Ok((crate::ansi_table::values(&s)?, Vec::new(), s.at, s.rot))
        }
        LiteFamily::Gear => {
            let s = crate::gear_table::GearTableSpec::parse(args)?;
            let extras = gear_extras_from_params(&s.params);
            Ok((crate::gear_table::values(&s)?, extras, s.at, s.rot))
        }
    }
}

/// 取一张卡的补算项（其余体系无补算）。
fn extras_for(card: &LiteCardSpec, model: &serde_json::Value) -> Result<Vec<(String, String)>, String> {
    match card.family {
        LiteFamily::Gear => gear_extras(model),
        _ => Ok(Vec::new()),
    }
}

/// 精简项：只挑字段表里的项；完整卡没有的 key = 表中错误（补算项除外）。
fn project(
    card: &LiteCardSpec,
    full: &std::collections::HashMap<String, String>,
    extras: &[(String, String)],
) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::with_capacity(card.fields.len());
    for f in card.fields {
        let v = if f.key.is_empty() {
            extras
                .iter()
                .find(|(t, _)| t == f.tag)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| {
                    crate::i18n::t_fmt(
                        "cmd.cardlite.err.extra_missing",
                        &[("id", card.id), ("tag", &f.tag)],
                    )
                })?
        } else {
            full.get(f.key)
                .cloned()
                .ok_or_else(|| {
                    crate::i18n::t_fmt(
                        "cmd.cardlite.err.full_item_missing",
                        &[("id", card.id), ("key", f.key)],
                    )
                })?
        };
        out.push((f.tag.to_string(), v));
    }
    Ok(out)
}

/// 精简卡预览（GUI `/api/card_preview` 与计算书同一条路径）。
pub fn preview_json(card: &LiteCardSpec, model: &serde_json::Value) -> Result<serde_json::Value, String> {
    let full = full_preview(card, model)?;
    let map = full_map(&full);
    let extras = extras_for(card, model)?;
    let values = project(card, &map, &extras)?;
    // 每项的完整卡口径（公式/来源/单位同源；补算项用字段表自带的）。
    let full_items = full["items"].as_array().cloned().unwrap_or_default();
    let mut items = Vec::with_capacity(card.fields.len());
    let mut missing = Vec::new();
    for (f, (tag, value)) in card.fields.iter().zip(&values) {
        let src = full_items.iter().find(|it| it["tag"].as_str() == Some(f.key) && !f.key.is_empty());
        let (unit, formula, source) = match src {
            Some(it) => (
                it["unit"].as_str().unwrap_or("").to_string(),
                it["formula"].as_str().unwrap_or("").to_string(),
                it["source"].as_str().unwrap_or("").to_string(),
            ),
            None => (String::new(), f.formula.to_string(), f.source.to_string()),
        };
        if value == "—" {
            missing.push(f.label.to_string());
        }
        items.push(serde_json::json!({
            "tag": tag,
            "label": f.label,
            "unit": unit,
            "value": value,
            "formula": formula,
            "source": source,
            "missing": value == "—",
        }));
    }
    // 读数照完整卡（基本参数/测量量），去掉纯公差/偏差行（精简口径不留公差）。
    let readout: Vec<serde_json::Value> = full["readout"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|r| {
            let k = r["k"].as_str().unwrap_or("");
            !(k.contains("公差") || k.contains("上差") || k.contains("下差")
                || k.contains("上/下差") || k.contains("偏差"))
        })
        .collect();
    Ok(serde_json::json!({
        "ok": true,
        "card": card.id,
        "renderer": "card_lite",
        "title": format!("{}（{}）", card.title, card.id),
        "expr": full.get("expr").cloned().unwrap_or(serde_json::Value::Null),
        "fields": full.get("fields").cloned().unwrap_or(serde_json::Value::Null),
        "readout": readout,
        "items": items,
        "missing": missing,
        "missing_note": "精简版：只列基本参数 + 主要测量量，不含任何公差（上/下偏差）列；\
                         完整明细请用对应的完整卡。",
    }))
}

/// 预览 JSON → ATTDEF 值（tag→value）。
pub fn values_from_preview(card: &LiteCardSpec, preview: &serde_json::Value) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::with_capacity(card.fields.len());
    for f in card.fields {
        let v = preview["items"]
            .as_array()
            .and_then(|arr| arr.iter().find(|it| it["tag"].as_str() == Some(f.tag)))
            .and_then(|it| it["value"].as_str())
            .ok_or_else(|| {
                crate::i18n::t_fmt(
                    "cmd.cardlite.err.preview_missing",
                    &[("id", card.id), ("tag", &f.tag)],
                )
            })?;
        out.push((f.tag.to_string(), v.to_string()));
    }
    Ok(out)
}

/// 完整卡取值表（tag→value）→ 精简 ATTDEF 值（CLI 路径复用；同一 `project`）。
pub fn values_from_full(
    card: &LiteCardSpec,
    full_values: &[(String, String)],
    extras: &[(String, String)],
) -> Result<Vec<(String, String)>, String> {
    let map: std::collections::HashMap<String, String> = full_values.iter().cloned().collect();
    project(card, &map, extras)
}

/// 建 INSERT（与 GB 精简卡同一条构造路径：几何一次、值一次）。
pub fn build_insert(
    card: &LiteCardSpec,
    values: &[(String, String)],
    at: [f64; 2],
    rot_deg: f64,
) -> Result<Insert, String> {
    let mut ins = Insert::new(card.block, Vector3::new(at[0], at[1], 0.0));
    ins.rotation = rot_deg.to_radians();
    {
        let c = &mut ins.common;
        c.layer = crate::partgen::LAYER_MAIN.to_string();
        c.color = Color::ByLayer;
        c.linetype = "ByLayer".to_string();
        c.line_weight = LineWeight::ByLayer;
    }
    for ad in attdefs(card).iter() {
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

/// 待放置件要带的 N 个 ATTRIB。
pub fn pending_attrs(
    card: &LiteCardSpec,
    values: &[(String, String)],
) -> Vec<(AttributeDefinition, String)> {
    attdefs(card)
        .into_iter()
        .map(|ad| {
            let v = values
                .iter()
                .find(|(tag, _)| tag == &ad.tag)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            (ad, v)
        })
        .collect()
}

/// 待放置件的 `OCSM_PART` 元数据（薄台账）。
pub fn part_meta_json(card: &LiteCardSpec) -> String {
    serde_json::json!({
        "family": "card_lite",
        "card": card.id,
        "full_card": card.full_card,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spline_table::text_box_ttf;

    /// 9 卡表驱动完整性：id/块名唯一、每卡 tag 唯一、key 非空（齿轮三圆除外）。
    #[test]
    fn lite_cards_table_driven_and_unique() {
        assert_eq!(LITE_CARDS.len(), 9, "NF/DIN/ANSI×4/齿轮 = 9 张");
        let mut ids = std::collections::HashSet::new();
        let mut blocks = std::collections::HashSet::new();
        for c in LITE_CARDS {
            assert!(ids.insert(c.id), "卡 id 重复：{}", c.id);
            assert!(blocks.insert(c.block), "块名重复：{}", c.block);
            assert!(!c.fields.is_empty(), "{} 字段为空", c.id);
            assert!(
                crate::card::card_type_by_token(c.full_card).is_some(),
                "{} 完整卡 {} 不在 CARD_TYPES",
                c.id,
                c.full_card
            );
            let mut tags = std::collections::HashSet::new();
            for f in c.fields {
                assert!(tags.insert(f.tag), "{} tag 重复：{}", c.id, f.tag);
                assert!(!f.label.is_empty() && f.wf > 0.0, "{} 字段 {} 缺 label/wf", c.id, f.tag);
                if f.key.is_empty() {
                    assert_eq!(c.family, LiteFamily::Gear, "只有齿轮有补算项：{}", c.id);
                    assert!(!f.formula.is_empty(), "补算项 {} 缺公式", f.tag);
                }
            }
        }
        for id in ["齿轮精简表", "NF花键精简表_内", "ANSI花键精简表_内_英文"] {
            assert!(by_id(id).is_some(), "by_id 找不到 {id}");
        }
        assert!(by_id("不存在").is_none());
    }

    /// 负断言：卡面无任何公差列（文字/tag）；图元数 = 字段数 + 5 线 / +1 文字。
    #[test]
    fn lite_blocks_have_no_tolerance_column() {
        for c in LITE_CARDS {
            let ents = block_entities(c);
            let n = c.fields.len();
            let lines = ents
                .iter()
                .filter(|e| matches!(e, EntityType::Line(_)))
                .count();
            let texts: Vec<String> = ents
                .iter()
                .filter_map(|e| match e {
                    EntityType::Text(t) => Some(t.value.clone()),
                    EntityType::MText(m) => Some(m.value.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(lines, n + 5, "{} 线数（4 框 + {n} 分隔 + 1 分栏）", c.id);
            assert_eq!(texts.len(), n + 1, "{} 文字数（标题 + {n} 标签）", c.id);
            assert_eq!(attdefs(c).len(), n, "{} ATTDEF 数", c.id);
            for bad in ["公差", "偏差", "上差", "下差"] {
                assert!(!texts.iter().any(|t| t.contains(bad)), "{} 文字含「{bad}」", c.id);
                assert!(
                    !c.fields.iter().any(|f| f.label.contains(bad) || f.tag.contains(bad)),
                    "{} 字段含「{bad}」",
                    c.id
                );
            }
            // 块底 = 字段行数 + 标题行。
            let min_y = ents
                .iter()
                .filter_map(|e| match e {
                    EntityType::Line(l) => Some(l.start.y.min(l.end.y)),
                    _ => None,
                })
                .fold(f64::INFINITY, f64::min);
            assert!(
                (min_y - data_bottom(c)).abs() < 1e-9,
                "{} 块底 {} 应为 {}",
                c.id,
                min_y,
                data_bottom(c)
            );
        }
    }

    /// 真字体 metrics：标题/标签均在格内（标签列 ≥3% 边距）、标签×值不相交、不出块框。
    #[test]
    fn lite_texts_stay_in_columns_and_do_not_overlap() {
        let cell = X_SEP - X_LABEL;
        for c in LITE_CARDS {
            let title = text_box_ttf([X_LABEL, Y_TITLE], TEXT_H, TITLE_WF, c.title);
            assert!(
                title[0] >= X_LEFT && title[2] <= X_RIGHT - 0.02,
                "{} 标题越表：{title:?}",
                c.id
            );
            let mut boxes: Vec<(String, [f64; 4])> = vec![(format!("title:{}", c.title), title)];
            for (i, f) in c.fields.iter().enumerate() {
                let row = i + 1;
                let lab = text_box_ttf([X_LABEL, label_y(row)], TEXT_H, f.wf, f.label);
                assert!(
                    lab[0] >= X_LEFT && lab[2] <= X_SEP - 0.02,
                    "{} 标签「{}」越列：{lab:?}",
                    c.id,
                    f.label
                );
                let margin = (X_SEP - lab[2]) / cell;
                assert!(
                    margin >= 0.03,
                    "{} 标签「{}」余量 {:.1}% < 3%：{lab:?}",
                    c.id,
                    f.label,
                    margin * 100.0
                );
                // 值取最宽样式（三圆/直径 + 小数）。
                let val = "12345.678";
                let vb = text_box_ttf([X_VALUE, value_y(row)], TEXT_H, f.vwf, val);
                assert!(
                    vb[0] >= X_SEP && vb[2] <= X_RIGHT - 0.02,
                    "{} 值「{val}」越值列：{vb:?}",
                    c.id
                );
                assert!(lab[2] < vb[0], "{} 标签×值相叠：{}", c.id, f.label);
                let top = row_top(row);
                let bottom = top - ROW;
                assert!(lab[1] >= bottom && lab[3] <= top && vb[1] >= bottom && vb[3] <= top);
                boxes.push((format!("label:{}", f.label), lab));
                boxes.push((format!("value:{val}"), vb));
            }
            for i in 0..boxes.len() {
                for j in (i + 1)..boxes.len() {
                    let (a, b) = (&boxes[i].1, &boxes[j].1);
                    let hit = a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
                    assert!(!hit, "{} 文本框相交：{} × {}", c.id, boxes[i].0, boxes[j].0);
                }
            }
        }
    }

    /// ANSI「花键类型」长值（英文全称最长）按**实体级值字宽**再压缩后仍留 ≥3% 余量。
    /// OCS 真字体扫描（扩体系批）发现的唯一越值列项，锁在单测里。
    #[test]
    fn ansi_type_values_fit_value_column() {
        for c in LITE_CARDS {
            let Some(f) = c.fields.iter().find(|f| f.tag == "(ANSI简)花键类型") else {
                continue;
            };
            // 中文卡只出中文类型串、英文卡只出英文串（同一 `type_text()` 语种分派）。
            let cn = matches!(c.family, LiteFamily::AnsiCnInt | LiteFamily::AnsiCnExt);
            let vals: &[&str] = if cn {
                &["30°圆齿根齿侧配合"]
            } else {
                &["FLAT ROOT MAJOR DIA FIT", "FILLET ROOT SIDE FIT"]
            };
            for val in vals {
                let vb = text_box_ttf([X_VALUE, value_y(1)], TEXT_H, f.vwf, val);
                assert!(
                    vb[0] >= X_SEP && vb[2] <= X_RIGHT - 0.02,
                    "{} 值「{val}」越值列：{vb:?}",
                    c.id
                );
                let margin = (X_RIGHT - vb[2]) / (X_RIGHT - X_VALUE);
                assert!(
                    margin >= 0.03,
                    "{} 值「{val}」余量 {:.1}% < 3%：{vb:?}",
                    c.id,
                    margin * 100.0
                );
                let lab = text_box_ttf([X_LABEL, label_y(1)], TEXT_H, f.wf, f.label);
                assert!(lab[2] < vb[0], "{} 标签×值相叠：{}", c.id, f.label);
            }
        }
    }

    /// 代表性模型（每族一例；与完整卡现有测试/集成用例同参数）。
    fn sample_models() -> Vec<(&'static str, serde_json::Value)> {
        vec![
            (
                "NF花键精简表_内",
                serde_json::json!({"card":"NF花键精简表_内","expr":null,"a":300.0,"m":7.5,
                    "z":38,"centering":null,"root":null,"fit":null,"at":null,"rot":0.0}),
            ),
            (
                "NF花键精简表_外",
                serde_json::json!({"card":"NF花键精简表_外","expr":null,"a":210.0,"m":7.5,
                    "z":28,"centering":null,"root":null,"fit":null,"at":null,"rot":0.0}),
            ),
            (
                "DIN花键精简表_内",
                serde_json::json!({"card":"DIN花键精简表_内","side":"int","expr":null,
                    "m":3.0,"z":38,"d_b":120.0,"hub":"9H","at":null,"rot":0.0}),
            ),
            (
                "DIN花键精简表_外",
                serde_json::json!({"card":"DIN花键精简表_外","side":"ext","expr":null,
                    "m":3.0,"z":38,"d_b":120.0,"shaft":"8f","at":null,"rot":0.0}),
            ),
            (
                "ANSI花键精简表_内_中文",
                serde_json::json!({"card":"ANSI花键精简表_内_中文","side":"int","p":16.0,
                    "z":20,"profile":null,"at":null,"rot":0.0}),
            ),
            (
                "ANSI花键精简表_外_英文",
                serde_json::json!({"card":"ANSI花键精简表_外_英文","side":"ext","p":16.0,
                    "z":20,"profile":null,"at":null,"rot":0.0}),
            ),
            (
                "齿轮精简表",
                serde_json::json!({"card":"齿轮精简表",
                    "expr":"GEAR EX M2 Z40 ALPHA20 X0 DA84 DF75 BETA0 H30",
                    "mate_z":null,"dwg":null,"grade":null,"center":null,"at":null,"rot":0.0}),
            ),
        ]
    }

    /// 同源：精简项值 = 完整卡同一 `compute()` 的对应项；齿轮三圆由 `GearParams` 补算。
    #[test]
    fn lite_values_equal_full_card_items() {
        for (id, model) in sample_models() {
            let card = by_id(id).unwrap();
            let full = full_preview(card, &model).unwrap();
            let map = full_map(&full);
            let extras = extras_for(card, &model).unwrap();
            let p = preview_json(card, &model).unwrap();
            let items = p["items"].as_array().unwrap();
            assert_eq!(items.len(), card.fields.len(), "{id} 项数");
            for f in card.fields {
                let it = items.iter().find(|it| it["tag"] == f.tag).unwrap();
                let v = it["value"].as_str().unwrap();
                if f.key.is_empty() {
                    let want = extras.iter().find(|(t, _)| t == f.tag).unwrap().1.clone();
                    assert_eq!(v, want, "{id} 补算项 {}", f.tag);
                } else {
                    assert_eq!(v, map[f.key], "{id} 项 {} 应等于完整卡 {}", f.tag, f.key);
                }
                // 卡面文字/标签不得含公差字样。
                assert!(!it["label"].as_str().unwrap().contains("公差"));
            }
            // 精简卡报告同源：值集合与完整卡项交集一致。
            let vals = values_from_preview(card, &p).unwrap();
            assert_eq!(vals.len(), card.fields.len());
        }
    }

    /// 表外不外推：ANSI 例（p16 z20）跨棒距在完整卡为「—」→ 精简卡同「—」。
    #[test]
    fn lite_missing_stays_dash() {
        let card = by_id("ANSI花键精简表_内_中文").unwrap();
        let model = serde_json::json!({"card":card.id,"side":"int","p":16.0,"z":20,
            "profile":null,"at":null,"rot":0.0});
        let p = preview_json(card, &model).unwrap();
        let it = p["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|it| it["tag"] == "(ANSI简)跨棒距")
            .unwrap()
            .clone();
        assert_eq!(it["value"], "—");
        assert_eq!(it["missing"], true);
        assert!(p["missing"].as_array().unwrap().iter().any(|m| m == "跨棒距 M"));
    }

    /// 建 INSERT：N 个 ATTRIB、值一致。
    #[test]
    fn lite_insert_has_n_attributes() {
        let (id, model) = sample_models().into_iter().find(|(i, _)| *i == "NF花键精简表_内").unwrap();
        let card = by_id(id).unwrap();
        let p = preview_json(card, &model).unwrap();
        let vals = values_from_preview(card, &p).unwrap();
        let ins = build_insert(card, &vals, [1.0, 2.0], 30.0).unwrap();
        assert_eq!(ins.block_name, card.block);
        assert_eq!(ins.attributes.len(), card.fields.len());
        for (tag, v) in &vals {
            let a = ins.attributes.iter().find(|a| a.tag == *tag).unwrap();
            assert_eq!(a.value.as_str(), v);
        }
        assert!((ins.rotation - 30f64.to_radians()).abs() < 1e-12);
    }
}
