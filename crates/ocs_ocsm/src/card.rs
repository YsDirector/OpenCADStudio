//! 「智能卡片」（`OCSMCARD`）通用卡片生成器：**卡片类型表驱动**。
//!
//! 用户 2026-09-25 定名：`OCSMCARD` / 中文「智能卡片」。以后生成的各类表格、
//! 甚至铭牌都靠它。本期只放「花键参数表」一种卡片；加新卡片 = 本表加一行 +
//! 一个渲染器（`CardRenderer` 分支）。体系/字段/来源同样表驱动：
//! `CardTypeSpec.systems` 指向后端选项表（`spline_gui::SPLINE_SYSTEMS`）的体系 id，
//! GUI 与命令解析都只认表里的清单。

/// 卡片渲染器（一个卡类型 = 一个渲染器；本期只有花键参数表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardRenderer {
    /// 花键参数表：九字段表达式反解 → `spline_tol::compute()` → 21 属性表格块。
    SplineTable,
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
pub const CARD_TYPES: &[CardTypeSpec] = &[CardTypeSpec {
    id: "花键参数表",
    aliases: &["spline", "splinetable", "花键"],
    label: "花键参数表",
    summary: "GB/T 3478 渐开线花键：内/外 + 九字段齿形表达式 → 21 项参数表",
    systems: &["gb3478"],
    renderer: CardRenderer::SplineTable,
}];

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
                    "renderer": match c.renderer {
                        CardRenderer::SplineTable => "spline_table",
                    },
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
        "OCSMCARD 用法：`OCSMCARD <卡类型> …`（本期卡类型：{types}；不带参数 = 开图形界面）。\
         花键参数表：`OCSMCARD 花键参数表 [std GB] 内 6H <九字段表达式> [dp 4.5] [root 平|圆] [at x,y] [rot 度]`；\
         外花键把 `内 6H` 换成 `外 5f`。"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表驱动完整性：卡类型的体系 id 必须能在选项表里解析（加新体系/新卡片时先拦住）。
    #[test]
    fn card_types_resolve_systems_and_tokens() {
        assert_eq!(CARD_TYPES.len(), 1, "本期只有「花键参数表」");
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
        // 记号/别名（大小写不敏感）
        assert!(card_type_by_token("花键参数表").is_some());
        assert!(card_type_by_token("SPLINE").is_some());
        assert!(card_type_by_token("splinetable").is_some());
        assert!(card_type_by_token("花键").is_some());
        assert!(card_type_by_token("铭牌").is_none(), "本期没有铭牌卡");
        assert!(card_type_by_token("  ").is_none());
        // 下发的 JSON 形状（GUI 只渲染）
        let j = card_types_json();
        assert_eq!(j[0]["id"], "花键参数表");
        assert_eq!(j[0]["systems"][0]["id"], "gb3478");
        assert!(j[0]["summary"].as_str().unwrap().contains("GB/T 3478"));
        assert!(usage_line().contains("OCSMCARD 花键参数表"));
    }
}
