//! 「智能卡片」（`OCSMCARD`）通用卡片生成器：**卡片类型表驱动**。
//!
//! 用户 2026-09-25 定名：`OCSMCARD` / 中文「智能卡片」。以后生成的各类表格、
//! 甚至铭牌都靠它。本期已放七种卡：GB 花键 / 齿轮 / ANSI 中 / ANSI 英 /
//! NF 内 / NF 外 / DIN；加新卡片 = 本表加一行 +
//! 一个渲染器（`CardRenderer` 分支）。体系/字段/来源同样表驱动：
//! `CardTypeSpec.systems` 指向后端选项表（`spline_gui::SPLINE_SYSTEMS`）的体系 id，
//! GUI 与命令解析都只认表里的清单。

/// 卡片渲染器（一个卡类型 = 一个渲染器；七张卡共用本枚举）。
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
    /// NF 外花键参数表（NF E22-141）：照模板原版；A/m/z 查表 → 13 行表（18 属性）。
    NfExtTable,
    /// DIN 花键参数表（DIN 5480-1 Bild 6）：13 行 × Nabe/Welle 两栏（26 属性）。
    DinTable,
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
            CardRenderer::NfExtTable => "nf_ext_table",
            CardRenderer::DinTable => "din_table",
        }
    }
}

/// 一种卡片的**表单字段**（表驱动 GUI 骨架；每个卡在自身模块里给清单）。
///
/// 字段只描述「控件 + 标签 + 取值来源（title）」；布局/分区/按钮位置由页面统一骨架提供，
/// 以后加卡 = 在 `CARD_TYPES` 加一行 + 一个渲染器 + 一份字段清单。
#[derive(Debug, Clone, Copy)]
pub struct CardFieldSpec {
    /// 模型 JSON 键（DOM id = `f_` + key）。
    pub key: &'static str,
    /// 显示名。
    pub label: &'static str,
    /// 控件类型：`number` / `text` / `textarea` / `select`。
    pub kind: &'static str,
    pub placeholder: &'static str,
    /// 初始值（空 = 不预填，交后端默认）。
    pub default: &'static str,
    /// 口径/来源（进原生 `title=`，不做常显）。
    pub title: &'static str,
    /// `select` 的静态选项 `(value, label)`。
    pub options: &'static [(&'static str, &'static str)],
    /// `select` 的动态选项来源键（`ansi_profiles` / `nf_centering` / `nf_roots`；空 = 无）。
    pub options_from: &'static str,
    /// `number` 控件属性（0.0 = 不设）。
    pub min: f64,
    pub step: f64,
    /// 必填（空值由页面拦住并指路；后端仍会校验）。
    pub required: bool,
}

/// 一张卡的表单骨架（字段清单 + 提示）。
#[derive(Debug, Clone, Copy)]
pub struct CardFormSpec {
    pub fields: &'static [CardFieldSpec],
    /// 常显提示（操作引导）。
    pub note: &'static str,
    /// 缺项/口径说明（进常显区的原生 `title=`）。
    pub missing_note: &'static str,
}

impl CardFormSpec {
    pub fn fields_json(&self) -> serde_json::Value {
        serde_json::Value::Array(
            self.fields
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "key": f.key,
                        "label": f.label,
                        "kind": f.kind,
                        "placeholder": f.placeholder,
                        "default": f.default,
                        "title": f.title,
                        "options": f.options.iter().map(|(v, l)| serde_json::json!({"value": v, "label": l})).collect::<Vec<_>>(),
                        "options_from": f.options_from,
                        "min": f.min,
                        "step": f.step,
                        "required": f.required,
                    })
                })
                .collect(),
        )
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
    /// 表单字段清单（`None` = 花键卡：保持专用面板，作为统一外观的基准）。
    pub form: Option<&'static CardFormSpec>,
}

/// 卡片类型表（顺序 = GUI 下拉/命令扫描顺序）。
pub const CARD_TYPES: &[CardTypeSpec] = &[
    CardTypeSpec {
        id: "花键参数表",
        aliases: &["spline", "splinetable", "花键", "gb", "GB花键参数表", "GB花键"],
        label: "GB 花键参数表",
        summary: "GB/T 3478 渐开线花键：内/外 + 九字段齿形表达式 → 21 项参数表",
        systems: &["gb3478"],
        renderer: CardRenderer::SplineTable,
        // 花键卡保持专用面板（统一外观的基准；以后新卡沿用通用骨架）。
        form: None,
    },
    CardTypeSpec {
        id: "齿轮参数表",
        aliases: &["gear", "geartable", "齿轮"],
        label: "齿轮参数表（GB/T 10095）",
        summary: "齿轮（内/外）：九字段齿形表达式反解 ha*/c* + 公法线跨距 → 19 项参数表（GB/T 10095 公差未收，如实标缺）",
        systems: &[],
        renderer: CardRenderer::GearTable,
        form: Some(&crate::gear_table::FORM),
    },
    CardTypeSpec {
        id: "ANSI花键参数表_中文",
        aliases: &["ansicn", "ansi中文", "ANSI花键参数表CN"],
        label: "ANSI 花键参数表（纯中文）",
        summary: "ANSI B92.1 花键：内/外 + P/z → 17 项参数表（中文版；公差/量棒/公法线未收，如实标缺）",
        systems: &[],
        renderer: CardRenderer::AnsiTableCn,
        form: Some(&crate::ansi_table::FORM),
    },
    CardTypeSpec {
        id: "ANSI花键参数表_英文",
        aliases: &["ansien", "ansi英文", "ANSI花键参数表EN"],
        label: "ANSI 花键参数表（纯英文）",
        summary: "ANSI B92.1 花键：内/外 + P/z → 17 项参数表（英文版；与中文版同构，仅文本语种替换）",
        systems: &[],
        renderer: CardRenderer::AnsiTableEn,
        form: Some(&crate::ansi_table::FORM),
    },
    CardTypeSpec {
        id: "NF内花键参数表",
        aliases: &["nf", "nfint", "NF内花键", "NF花键参数表"],
        label: "NF E22-141 内花键参数表",
        summary: "NF E22-141 内花键（拉削，外径定心）：A/m/z 查 p18 表 → 13 行镜像表（Az=A 或 A+0.3m、D=A−2m、V/G 取 p23–p25、ri 取 p22；公差 = p28 R7/H7 + p29 E 偏差，四配合）",
        systems: &[],
        renderer: CardRenderer::NfTable,
        form: Some(&crate::nf_table::FORM),
    },
    CardTypeSpec {
        id: "NF外花键参数表",
        aliases: &["nfext", "nfout", "NF外花键", "NF外花键参数表"],
        label: "NF E22-141 外花键参数表",
        summary: "NF E22-141 外花键（滚齿，齿面定心）：A/m/z 查 p20–p22 表 → 13 行模板原版表（Dee=A−0.2m、Die=A−2.4m/2.694m、K/W 取 p23–p25；公差 = ISO h12/H7 + p29 外花键 E 偏差，四配合）",
        systems: &[],
        renderer: CardRenderer::NfExtTable,
        form: Some(&crate::nf_ext_table::FORM),
    },
    CardTypeSpec {
        id: "DIN花键参数表",
        aliases: &["din", "din5480", "DIN花键", "din花键参数表"],
        label: "DIN 5480 花键参数表",
        summary: "DIN 5480-1 Bild 6：13 行 × Nabe/Welle 两栏（z/m/α/三直径/e-s 三极限/D_M/两 M 极限）；Table 7 上段偏差 2026-09-26 OCR 入库（c4/c5 有实锚），公差表只到 6–9 级锚点，缺口如实标「—」",
        systems: &[],
        renderer: CardRenderer::DinTable,
        form: Some(&crate::din_table::FORM),
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
                    // 表驱动 GUI：字段清单 + 提示（花键卡为 null，页面用专用面板）。
                    "form": c.form.map(|f| serde_json::json!({
                        "fields": f.fields_json(),
                        "note": f.note,
                        "missing_note": f.missing_note,
                    })),
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
         * GB 花键参数表：`OCSMCARD 花键参数表 [std GB] 内 6H <九字段表达式> [dp 4.5] [root 平|圆] [at x,y] [rot 度]`\n\
         * 齿轮参数表：`OCSMCARD 齿轮参数表 <九字段表达式> [mate z₂] [dwg 图号] [grade 精度等级] [center a] [at x,y] [rot 度]`\n\
         * ANSI 花键参数表：`OCSMCARD ANSI花键参数表_中文 内 <九字段表达式> [profile ANSI30R] [at x,y]`（英文版换 `_英文`；旧写法 `内 P16 Z20` 兼容）\n\
         * NF 内花键参数表：`OCSMCARD NF内花键参数表 <九字段表达式> [中心 外径|齿面] [根 平|圆] [配合 松动|滑动|固定|压] [at x,y]`（旧写法 `A300 M7.5 Z38` 兼容）\n\
         * NF 外花键参数表：`OCSMCARD NF外花键参数表 <九字段表达式> [中心 齿面|外径] [根 平|圆] [配合 松动|滑动|固定|压] [at x,y]`（旧写法 `A300 M7.5 Z38` 兼容）\n\
         * DIN 花键参数表：`OCSMCARD DIN花键参数表 <九字段表达式> [N9H] [W8f] [ae …] [as …] [tactn …] [teffn …] [tactw …] [teffw …] [at x,y]`（旧写法 `M3 Z38 B120` 兼容）"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表驱动完整性：卡类型的体系 id 必须能在选项表里解析（加新体系/新卡片时先拦住）。
    #[test]
    fn card_types_resolve_systems_and_tokens() {
        assert_eq!(CARD_TYPES.len(), 7, "本期七张卡：GB 花键 / 齿轮 / ANSI 中 / ANSI 英 / NF 内 / NF 外 / DIN");
        let c = &CARD_TYPES[0];
        assert_eq!(c.id, "花键参数表", "id 保持不变（深链 ?card= 与 CLI 旧写法兼容）");
        assert_eq!(c.label, "GB 花键参数表", "下拉显示名补 GB 前缀");
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
        assert_eq!(CARD_TYPES[5].id, "NF外花键参数表");
        assert_eq!(CARD_TYPES[5].renderer, CardRenderer::NfExtTable);
        assert!(CARD_TYPES[5].systems.is_empty());
        assert_eq!(CARD_TYPES[5].label, "NF E22-141 外花键参数表");
        // ⑤ 下拉闭合态：每张卡的 label 都带标准号/体系（用户截图要求）
        for c in CARD_TYPES {
            assert!(
                c.label.contains("GB") || c.label.contains("ANSI") || c.label.contains("NF") || c.label.contains("DIN"),
                "卡名应带标准号：{}",
                c.label
            );
        }
        assert_eq!(CARD_TYPES[6].id, "DIN花键参数表");
        assert_eq!(CARD_TYPES[6].renderer, CardRenderer::DinTable);
        assert!(CARD_TYPES[6].systems.is_empty());
        assert_eq!(CardRenderer::DinTable.id(), "din_table");
        // 表驱动 GUI：花键卡无 form（专用面板基准）；其余五卡都有字段清单。
        assert!(CARD_TYPES[0].form.is_none(), "花键卡保持专用面板");
        for c in &CARD_TYPES[1..] {
            let f = c.form.unwrap_or_else(|| panic!("{} 缺 form 字段清单", c.id));
            assert!(!f.fields.is_empty(), "{} form 为空", c.id);
            assert!(
                f.fields.iter().all(|x| !x.kind.is_empty() && !x.label.is_empty()),
                "{} form 字段缺 kind/label",
                c.id
            );
        }
        assert_eq!(CARD_TYPES[1].form.unwrap().fields.len(), 5, "齿轮 5 字段");
        assert_eq!(CARD_TYPES[2].form.unwrap().fields.len(), 5, "ANSI 5 字段（含 expr）");
        assert_eq!(CARD_TYPES[3].form.unwrap().fields.len(), 5, "ANSI 5 字段（含 expr）");
        assert_eq!(CARD_TYPES[4].form.unwrap().fields.len(), 7, "NF 内 7 字段（含 expr/配合）");
        assert_eq!(CARD_TYPES[5].form.unwrap().fields.len(), 7, "NF 外 7 字段（含 expr/配合）");
        assert_eq!(CARD_TYPES[6].form.unwrap().fields.len(), 13, "DIN 13 字段（含 expr）");
        // 五张卡都有表达式输入框（齿轮已有；其余四卡新增；花键卡保留专用面板）。
        for c in &CARD_TYPES[1..] {
            let f = c.form.unwrap();
            let e = f.fields.iter().find(|x| x.key == "expr");
            let e = e.unwrap_or_else(|| panic!("{} 缺 expr 字段", c.id));
            assert_eq!(e.kind, "textarea", "{} expr 应为 textarea", c.id);
        }
        assert!(
            CARD_TYPES[4].form.unwrap().fields.iter().any(|x| x.key == "fit"),
            "NF 配合类别字段"
        );
        assert!(
            CARD_TYPES[5].form.unwrap().fields.iter().any(|x| x.key == "centering"),
            "NF 外定心字段"
        );
        // 记号/别名（大小写不敏感）
        assert!(card_type_by_token("花键参数表").is_some());
        assert!(card_type_by_token("SPLINE").is_some());
        assert!(card_type_by_token("splinetable").is_some());
        assert!(card_type_by_token("花键").is_some());
        assert!(card_type_by_token("gb").is_some(), "别名 gb");
        assert!(card_type_by_token("GB花键参数表").is_some());
        assert!(card_type_by_token("GB花键").is_some());
        assert!(card_type_by_token("齿轮").is_some());
        assert!(card_type_by_token("GEAR").is_some());
        assert!(card_type_by_token("ansicn").is_some());
        assert!(card_type_by_token("ANSI花键参数表_英文").is_some());
        assert!(card_type_by_token("ansien").is_some());
        assert!(card_type_by_token("NF内花键参数表").is_some());
        assert!(card_type_by_token("nf").is_some());
        assert!(card_type_by_token("NFINT").is_some());
        assert!(card_type_by_token("NF内花键").is_some());
        assert!(card_type_by_token("NF外花键参数表").is_some());
        assert!(card_type_by_token("NF外花键").is_some());
        assert!(card_type_by_token("nfext").is_some());
        assert!(card_type_by_token("NFout").is_some());
        assert!(card_type_by_token("DIN花键参数表").is_some());
        assert!(card_type_by_token("din").is_some());
        assert!(card_type_by_token("DIN5480").is_some());
        assert!(card_type_by_token("din花键").is_some());
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
        assert_eq!(j[5]["id"], "NF外花键参数表");
        assert_eq!(j[5]["renderer"], "nf_ext_table");
        assert_eq!(j[6]["id"], "DIN花键参数表");
        assert_eq!(j[6]["renderer"], "din_table");
        // form 下发：花键 null；其余含 fields/note/missing_note
        assert!(j[0]["form"].is_null());
        assert_eq!(j[1]["form"]["fields"].as_array().unwrap().len(), 5);
        assert!(j[4]["form"]["fields"].as_array().unwrap().iter().any(|f| f["key"] == "fit"));
        assert!(j[4]["form"]["missing_note"].as_str().unwrap().contains("p29"));
        assert_eq!(j[5]["form"]["fields"].as_array().unwrap().len(), 7);
        assert_eq!(j[6]["form"]["fields"].as_array().unwrap().len(), 13);
        assert!(j[6]["summary"].as_str().unwrap().contains("Bild 6"));
        assert!(j[5]["summary"].as_str().unwrap().contains("NF E22-141"));
        assert!(j[0]["summary"].as_str().unwrap().contains("GB/T 3478"));
        assert!(usage_line().contains("OCSMCARD 花键参数表"));
        assert!(usage_line().contains("OCSMCARD 齿轮参数表"));
        assert!(usage_line().contains("ANSI花键参数表_中文"));
        assert!(usage_line().contains("OCSMCARD NF内花键参数表"));
        assert!(usage_line().contains("OCSMCARD NF外花键参数表"));
        assert!(usage_line().contains("OCSMCARD DIN花键参数表"));
    }
}
