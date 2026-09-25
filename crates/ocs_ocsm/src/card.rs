//! 「智能卡片」（`OCSMCARD`）通用卡片生成器：**卡片类型表驱动**。
//!
//! 用户 2026-09-25 定名：`OCSMCARD` / 中文「智能卡片」。以后生成的各类表格、
//! 甚至铭牌都靠它。本期只放「花键参数表」一种卡片；加新卡片 = 本表加一行 +
//! 一个渲染器（`CardRenderer` 分支）。体系/字段/来源同样表驱动：
//! `CardTypeSpec.systems` 指向后端选项表（`spline_gui::SPLINE_SYSTEMS`）的体系 id，
//! GUI 与命令解析都只认表里的清单。

/// 卡片渲染器（一个卡类型 = 一个渲染器；本期：花键参数表 GB + 齿轮参数表 + ANSI 中/英）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardRenderer {
    /// 花键参数表（GB/T 3478）：九字段表达式反解 → `spline_tol::compute()` → 21 属性表格块。
    SplineTable,
    /// 齿轮参数表：九字段表达式反解 → `gear::GearParams` → 19 属性表格块。
    GearTable,
    /// ANSI 花键参数表（**纯中文**）：ANSI B92.1 P/z → 17 属性表格块。
    AnsiTableCn,
    /// ANSI 花键参数表（**纯英文**）：同构，文本语种替换。
    AnsiTableEn,
    /// NF 内花键参数表（NF E22-141）：A/m/z 查表 → 13 行镜像表（18 属性）。
    NfTable,
}

impl CardRenderer {
    /// 稳定 id（下发给 GUI；页面按它切面板）。
    pub fn id(self) -> &'static str {
        match self {
            CardRenderer::SplineTable => "spline_table",
            CardRenderer::GearTable => "gear_table",
            CardRenderer::AnsiTableCn => "ansi_table_cn",
            CardRenderer::AnsiTableEn => "ansi_table_en",
            CardRenderer::NfTable => "nf_table",
        }
    }
}

/// 一种卡片类型。
#[derive(Debug, Clone, Copy)]
pub struct CardTypeSpec {
    /// 稳定 id / 命令里的卡类型记号（如 `花键参数表`）。
    pub id: &'static str,
    /// 命令别名（大小写不敏感）。
    pub aliases: &'static [&'static str],
    /// 界面名。
    pub label: &'static str,
    /// 一句话说明（命令目录/title）。
    pub summary: &'static str,
    /// 可用体系 id（指向选项表；命令 `std <体系>` 与 GUI 体系下拉只收这里的）。
    pub systems: &'static [&'static str],
    /// 渲染器。
    pub renderer: CardRenderer,
}

/// 卡片类型表（顺序 = GUI 下拉/命令扫描顺序）。
pub const CARD_TYPES: &[CardTypeSpec] = &[
    CardTypeSpec {
        id: "花键参数表",
        aliases: &["spline", "splinetable", "花键"],
        label: "花键参数表",
        summary: "GB/T 3478 渐开线花键：内/外 + 九字段齿形表达式 → 21 项参数表",
        systems: &["gb3478"],
        renderer: CardRenderer::SplineTable,
    },
    CardTypeSpec {
        id: "齿轮参数表",
        aliases: &["gear", "geartable", "齿轮"],
        label: "齿轮参数表",
        summary: "齿轮（内/外）：九字段齿形表达式反解 ha*/c* + 公法线跨距 → 19 项参数表（GB/T 10095 公差未收，如实标缺）",
        systems: &[],
        renderer: CardRenderer::GearTable,
    },
    CardTypeSpec {
        id: "ANSI花键参数表_中文",
        aliases: &["ansicn", "ansi中文", "ANSI花键参数表CN"],
        label: "ANSI 花键参数表（纯中文）",
        summary: "ANSI B92.1 花键：内/外 + P/z → 17 项参数表（中文版；公差/量棒/公法线未收，如实标缺）",
        systems: &[],
        renderer: CardRenderer::AnsiTableCn,
    },
    CardTypeSpec {
        id: "ANSI花键参数表_英文",
        aliases: &["ansien", "ansi英文", "ANSI花键参数表EN"],
        label: "ANSI 花键参数表（纯英文）",
        summary: "ANSI B92.1 花键：内/外 + P/z → 17 项参数表（英文版；与中文版同构，仅文本语种替换）",
        systems: &[],
        renderer: CardRenderer::AnsiTableEn,
    },
    CardTypeSpec {
        id: "NF内花键参数表",
        aliases: &["nf", "nfint", "NF内花键", "NF花键参数表"],
        label: "NF 内花键参数表",
        summary: "NF E22-141 内花键（拉削，外径定心）：A/m/z 查 p18 表 → 13 行镜像表（Az=A 或 A+0.3m、D=A−2m、V/G 取 p23–p25、ri 取 p22；偏差列义未辨，如实标缺）",
        systems: &[],
        renderer: CardRenderer::NfTable,
    },
];

/// 卡类型 token → 表项（大小写不敏感；别名可命中）。
pub fn card_type_by_token(text: &str) -> Option<&'static CardTypeSpec> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    CARD_TYPES.iter().find(|c| {
        c.id.eq_ignore_ascii_case(t) || c.aliases.iter().any(|a| a.eq_ignore_ascii_case(t))
    })
}

/// 卡类型 → 可用体系（label/standard 取自选项表；随 `/api/spline_options` 下发）。
pub fn card_types_json() -> serde_json::Value {
    serde_json::Value::Array(
        CARD_TYPES
            .iter()
            .map(|c| {
                let systems: Vec<serde_json::Value> = c
                    .systems
                    .iter()
                    .map(|id| {
                        let s = crate::spline_gui::system_by_id(id);
                        serde_json::json!({
                            "id": id,
                            "label": s.map(|s| s.label).unwrap_or(id),
                            "standard": s.map(|s| s.standard).unwrap_or(""),
                        })
                    })
                    .collect();
                serde_json::json!({
                    "id": c.id,
                    "aliases": c.aliases,
                    "label": c.label,
                    "summary": c.summary,
                    "systems": systems,
                    "renderer": c.renderer.id(),
                })
            })
            .collect(),
    )
}

/// 命令用法（报错指路；卡类型清单从表里来）。
pub fn usage_line() -> String {
    let types = CARD_TYPES
        .iter()
        .map(|c| c.id)
        .collect::<Vec<_>>()
        .join(" / ");
    format!(
        "OCSMCARD 用法：`OCSMCARD <卡类型> …`（本期卡类型：{types}；不带参数 = 开图形界面）。\n\
         * 花键参数表：`OCSMCARD 花键参数表 [std GB] 内 6H <九字段表达式> [dp 4.5] [root 平|圆] [at x,y] [rot 度]`\n\
         * 齿轮参数表：`OCSMCARD 齿轮参数表 <九字段表达式> [mate z₂] [dwg 图号] [grade 精度等级] [center a] [at x,y] [rot 度]`\n\
         * ANSI 花键参数表：`OCSMCARD ANSI花键参数表_中文 内 P16 Z20 [profile ANSI30R] [at x,y] [rot 度]`（英文版换 `_英文`）\n\
         * NF 内花键参数表：`OCSMCARD NF内花键参数表 A300 M7.5 [Z38] [中心 外径|齿面] [根 平|圆] [at x,y] [rot 度]`"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表驱动完整性：卡类型的体系 id 必须能在选项表里解析（加新体系/新卡片时先拦住）。
    #[test]
    fn card_types_resolve_systems_and_tokens() {
        assert_eq!(CARD_TYPES.len(), 5, "本期五张卡：GB 花键 / 齿轮 / ANSI 中 / ANSI 英 / NF 内花键");
        let c = &CARD_TYPES[0];
        assert_eq!(c.id, "花键参数表");
        assert_eq!(c.renderer, CardRenderer::SplineTable);
        for id in c.systems {
            assert!(
                crate::spline_gui::system_by_id(id).is_some(),
                "卡类型「{}」的体系 {id} 不在选项表",
                c.id
            );
        }
        assert_eq!(CARD_TYPES[1].id, "齿轮参数表");
        assert_eq!(CARD_TYPES[1].renderer, CardRenderer::GearTable);
        assert!(CARD_TYPES[1].systems.is_empty());
        assert_eq!(CARD_TYPES[2].id, "ANSI花键参数表_中文");
        assert_eq!(CARD_TYPES[2].renderer, CardRenderer::AnsiTableCn);
        assert_eq!(CARD_TYPES[3].id, "ANSI花键参数表_英文");
        assert_eq!(CARD_TYPES[3].renderer, CardRenderer::AnsiTableEn);
        assert_eq!(CARD_TYPES[4].id, "NF内花键参数表");
        assert_eq!(CARD_TYPES[4].renderer, CardRenderer::NfTable);
        assert!(CARD_TYPES[4].systems.is_empty());
        // 记号/别名（大小写不敏感）
        assert!(card_type_by_token("花键参数表").is_some());
        assert!(card_type_by_token("SPLINE").is_some());
        assert!(card_type_by_token("splinetable").is_some());
        assert!(card_type_by_token("花键").is_some());
        assert!(card_type_by_token("齿轮").is_some());
        assert!(card_type_by_token("GEAR").is_some());
        assert!(card_type_by_token("ansicn").is_some());
        assert!(card_type_by_token("ANSI花键参数表_英文").is_some());
        assert!(card_type_by_token("ansien").is_some());
        assert!(card_type_by_token("NF内花键参数表").is_some());
        assert!(card_type_by_token("nf").is_some());
        assert!(card_type_by_token("NFINT").is_some());
        assert!(card_type_by_token("NF内花键").is_some());
        assert!(card_type_by_token("铭牌").is_none(), "本期没有铭牌卡");
        assert!(card_type_by_token("  ").is_none());
        // 下发的 JSON 形状（GUI 只渲染）
        let j = card_types_json();
        assert_eq!(j[0]["id"], "花键参数表");
        assert_eq!(j[0]["systems"][0]["id"], "gb3478");
        assert_eq!(j[0]["renderer"], "spline_table");
        assert_eq!(j[1]["renderer"], "gear_table");
        assert_eq!(j[2]["renderer"], "ansi_table_cn");
        assert_eq!(j[3]["renderer"], "ansi_table_en");
        assert_eq!(j[4]["id"], "NF内花键参数表");
        assert_eq!(j[4]["renderer"], "nf_table");
        assert!(j[4]["summary"].as_str().unwrap().contains("NF E22-141"));
        assert!(j[0]["summary"].as_str().unwrap().contains("GB/T 3478"));
        assert!(usage_line().contains("OCSMCARD 花键参数表"));
        assert!(usage_line().contains("OCSMCARD 齿轮参数表"));
        assert!(usage_line().contains("ANSI花键参数表_中文"));
        assert!(usage_line().contains("OCSMCARD NF内花键参数表"));
    }
}
