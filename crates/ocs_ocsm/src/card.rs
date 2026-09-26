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
    /// GB 花键参数表**精简版**（用户 2026-09-27）：只列基本参数 + 主要测量量，
    /// **不含任何公差列**；取值与 GB 卡同一份 `spline_tol::compute()`。
    SplineLite,
    /// NF / DIN / ANSI / 齿轮**精简版**（用户 2026-09-27，共享引擎 `card_lite`）：
    /// 一行一卡定义；取值/校验/表达式反解全部投影自对应完整卡（同一份计算）。
    CardLite,
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
            CardRenderer::SplineLite => "spline_lite",
            CardRenderer::CardLite => "card_lite",
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
    /// 下拉分组（`None` = 平铺；如「精简版」）。GUI 仅渲染，不写死卡名。
    pub group: Option<&'static str>,
    /// 一句话说明（命令目录/title）。
    pub summary: &'static str,
    /// 可用体系 id（指向选项表；命令 `std <体系>` 与 GUI 体系下拉只收这里的）。
    pub systems: &'static [&'static str],
    /// 固定方向（`int`/`ext`；`None` = 无方向/双向卡）。**一卡一方向**：
    /// GUI 面板不放方向选择；旧 CLI 显式 `内/外` 仍可覆盖（兼容），表达式 KIND 仍按本项校验。
    pub direction: Option<&'static str>,
    /// 渲染器。
    pub renderer: CardRenderer,
    /// 表单字段清单（`None` = 花键卡：保持专用面板，作为统一外观的基准）。
    pub form: Option<&'static CardFormSpec>,
}

/// 卡片类型表（顺序 = GUI 下拉/命令扫描顺序）。
///
/// 一卡一方向（用户 2026-09-26）：内外切换 = 选卡；旧 id 指向默认方向那张：
/// `花键参数表`/`ANSI花键参数表_中文`/`_英文` = 内，`DIN花键参数表` = 内；
/// `ANSI` 拆 4 张（内/外 × 中/英），`DIN` 拆 2 张，`NF` 已内外两张。
pub const CARD_TYPES: &[CardTypeSpec] = &[
    CardTypeSpec {
        id: "花键参数表",
        aliases: &["spline", "splinetable", "花键", "gb", "GB花键参数表", "GB花键", "花键参数表_内", "GB花键_内"],
        label: "GB 花键参数表（内）",
        group: None,
        summary: "GB/T 3478 内花键：九字段齿形表达式 → 21 项参数表（一卡一方向；外键见表下方「（外）」卡）",
        systems: &["gb3478"],
        direction: Some("int"),
        renderer: CardRenderer::SplineTable,
        // 花键卡保持专用面板（统一外观的基准；以后新卡沿用通用骨架）。
        form: None,
    },
    CardTypeSpec {
        id: "花键参数表_外",
        aliases: &["splineext", "GB花键参数表_外", "GB花键_外", "GB花键外"],
        label: "GB 花键参数表（外）",
        group: None,
        summary: "GB/T 3478 外花键：九字段齿形表达式 → 21 项参数表（一卡一方向，不再面板选内外）",
        systems: &["gb3478"],
        direction: Some("ext"),
        renderer: CardRenderer::SplineTable,
        form: None,
    },
    CardTypeSpec {
        id: "齿轮参数表",
        aliases: &["gear", "geartable", "齿轮"],
        label: "齿轮参数表（GB/T 10095）",
        group: None,
        summary: "齿轮（内/外）：九字段齿形表达式反解 ha*/c* + 公法线跨距 → 19 项参数表（GB/T 10095 公差未收，如实标缺）",
        systems: &[],
        direction: None,
        renderer: CardRenderer::GearTable,
        form: Some(&crate::gear_table::FORM),
    },
    CardTypeSpec {
        id: "ANSI花键参数表_中文",
        aliases: &["ansicn", "ansi中文", "ANSI花键参数表CN", "ANSI内中文"],
        label: "ANSI 花键参数表（内·纯中文）",
        group: None,
        summary: "ANSI B92.1 内花键 + P/z → 17 项参数表（纯中文；公差/量棒/公法线未收，如实标缺）",
        systems: &[],
        direction: Some("int"),
        renderer: CardRenderer::AnsiTableCn,
        form: Some(&crate::ansi_table::FORM),
    },
    CardTypeSpec {
        id: "ANSI花键参数表_外_中文",
        aliases: &["ansicnext", "ansi外中文", "ANSI花键参数表外CN"],
        label: "ANSI 花键参数表（外·纯中文）",
        group: None,
        summary: "ANSI B92.1 外花键 + P/z → 17 项参数表（纯中文；公差/量棒/公法线未收，如实标缺）",
        systems: &[],
        direction: Some("ext"),
        renderer: CardRenderer::AnsiTableCn,
        form: Some(&crate::ansi_table::FORM),
    },
    CardTypeSpec {
        id: "ANSI花键参数表_英文",
        aliases: &["ansien", "ansi英文", "ANSI花键参数表EN", "ANSI内英文"],
        label: "ANSI 花键参数表（内·纯英文）",
        group: None,
        summary: "ANSI B92.1 内花键 + P/z → 17 项参数表（纯英文；与中文版同构，仅文本语种替换）",
        systems: &[],
        direction: Some("int"),
        renderer: CardRenderer::AnsiTableEn,
        form: Some(&crate::ansi_table::FORM),
    },
    CardTypeSpec {
        id: "ANSI花键参数表_外_英文",
        aliases: &["ansienext", "ansi外英文", "ANSI花键参数表外EN"],
        label: "ANSI 花键参数表（外·纯英文）",
        group: None,
        summary: "ANSI B92.1 外花键 + P/z → 17 项参数表（纯英文；与中文版同构，仅文本语种替换）",
        systems: &[],
        direction: Some("ext"),
        renderer: CardRenderer::AnsiTableEn,
        form: Some(&crate::ansi_table::FORM),
    },
    CardTypeSpec {
        id: "NF内花键参数表",
        aliases: &["nf", "nfint", "NF内花键", "NF花键参数表"],
        label: "NF E22-141 内花键参数表",
        group: None,
        summary: "NF E22-141 内花键（拉削，外径定心）：A/m/z 查 p18 表 → 13 行镜像表（Az=A 或 A+0.3m、D=A−2m、V/G 取 p23–p25、ri 取 p22；公差 = p28 R7/H7 + p29 E 偏差，四配合）",
        systems: &[],
        direction: Some("int"),
        renderer: CardRenderer::NfTable,
        form: Some(&crate::nf_table::FORM),
    },
    CardTypeSpec {
        id: "NF外花键参数表",
        aliases: &["nfext", "nfout", "NF外花键", "NF外花键参数表"],
        label: "NF E22-141 外花键参数表",
        group: None,
        summary: "NF E22-141 外花键（滚齿，齿面定心）：A/m/z 查 p20–p22 表 → 13 行模板原版表（Dee=A−0.2m、Die=A−2.4m/2.694m、K/W 取 p23–p25；公差 = ISO h12/H7 + p29 外花键 E 偏差，四配合）",
        systems: &[],
        direction: Some("ext"),
        renderer: CardRenderer::NfExtTable,
        form: Some(&crate::nf_ext_table::FORM),
    },
    CardTypeSpec {
        id: "DIN花键参数表",
        aliases: &["din", "din5480", "DIN花键", "din花键参数表", "DIN内花键"],
        label: "DIN 5480 内花键参数表",
        group: None,
        summary: "DIN 5480-1 Bild 6 内花键（Nabe）单栏 13 行（z/m/α/三直径/e 三极限/D_M/两 M2 极限）；Table 7 上段偏差有实锚，公差表只到 6–9 级锚点，缺口如实标「—」",
        systems: &[],
        direction: Some("int"),
        renderer: CardRenderer::DinTable,
        form: Some(&crate::din_table::FORM_INT),
    },
    CardTypeSpec {
        id: "DIN花键参数表_外",
        aliases: &["dinext", "DIN外花键", "din5480外"],
        label: "DIN 5480 外花键参数表",
        group: None,
        summary: "DIN 5480-1 Bild 6 外花键（Welle）单栏 13 行（z/m/α/三直径/s 三极限/D_M/两 M1 极限）；与内卡同源，缺口同样如实标「—」",
        systems: &[],
        direction: Some("ext"),
        renderer: CardRenderer::DinTable,
        form: Some(&crate::din_table::FORM_EXT),
    },
    CardTypeSpec {
        id: "GB花键精简表_内",
        aliases: &["gb精简", "精简内", "精简卡", "GB精简花键_内", "花键精简表_内", "splinelite"],
        label: "GB 花键参数表（内·精简版）",
        group: Some("精简版"),
        summary: "GB/T 3478 内花键精简卡：只列基本参数（标准/模数/齿数/压力角/齿根样式/大径/小径）+ 量棒 Dp / 跨棒距 Md，不含任何公差列",
        systems: &["gb3478"],
        direction: Some("int"),
        renderer: CardRenderer::SplineLite,
        form: Some(&crate::spline_lite::FORM_INT),
    },
    CardTypeSpec {
        id: "GB花键精简表_外",
        aliases: &["gb精简外", "精简外", "GB精简花键_外", "花键精简表_外", "splineliteext"],
        label: "GB 花键参数表（外·精简版）",
        group: Some("精简版"),
        summary: "GB/T 3478 外花键精简卡：只列基本参数（标准/模数/齿数/压力角/齿根样式/大径/小径）+ 跨测齿数 Kn / 公法线 Wn，不含任何公差列",
        systems: &["gb3478"],
        direction: Some("ext"),
        renderer: CardRenderer::SplineLite,
        form: Some(&crate::spline_lite::FORM_EXT),
    },
    // ── 精简版扩体系（用户 2026-09-27；一行一卡，取值/校验投影自对应完整卡）──
    CardTypeSpec {
        id: "NF花键精简表_内",
        aliases: &["nf精简", "nf精简内", "NF精简花键_内", "nf精简表_内", "nflite"],
        label: "NF E22-141 内花键参数表（精简版）",
        group: Some("精简版"),
        summary: "NF 内花键精简卡：执行标准/定心方式/模数/齿数/压力角/齿根样式/加工方法/大径/小径 + 量棒 V / 跨棒距 G，不含公差列",
        systems: &[],
        direction: Some("int"),
        renderer: CardRenderer::CardLite,
        form: Some(&crate::nf_table::FORM),
    },
    CardTypeSpec {
        id: "NF花键精简表_外",
        aliases: &["nf精简外", "NF精简花键_外", "nf精简表_外", "nfliteext"],
        label: "NF E22-141 外花键参数表（精简版）",
        group: Some("精简版"),
        summary: "NF 外花键精简卡：执行标准/定心方式/模数/齿数/压力角/齿根样式/加工方法/大径/小径 + 跨测齿数 K / 公法线 W，不含公差列",
        systems: &[],
        direction: Some("ext"),
        renderer: CardRenderer::CardLite,
        form: Some(&crate::nf_ext_table::FORM),
    },
    CardTypeSpec {
        id: "DIN花键精简表_内",
        aliases: &["din精简", "din精简内", "DIN精简花键_内", "din精简表_内", "dinlite"],
        label: "DIN 5480 内花键参数表（精简版）",
        group: Some("精简版"),
        summary: "DIN 5480 内花键精简卡：标记/齿数/模数/压力角 + 齿根圆/齿根成形圆/齿顶圆/量圆 D_M/量圆距 M2 极限（按 DIN 自身口径，不含公差列）",
        systems: &[],
        direction: Some("int"),
        renderer: CardRenderer::CardLite,
        form: Some(&crate::din_table::FORM_INT),
    },
    CardTypeSpec {
        id: "DIN花键精简表_外",
        aliases: &["din精简外", "DIN精简花键_外", "din精简表_外", "dinliteext"],
        label: "DIN 5480 外花键参数表（精简版）",
        group: Some("精简版"),
        summary: "DIN 5480 外花键精简卡：标记/齿数/模数/压力角 + 齿顶圆/齿根成形圆/齿根圆/量圆 D_M/量圆距 M1 极限（按 DIN 自身口径，不含公差列）",
        systems: &[],
        direction: Some("ext"),
        renderer: CardRenderer::CardLite,
        form: Some(&crate::din_table::FORM_EXT),
    },
    CardTypeSpec {
        id: "ANSI花键精简表_内_中文",
        aliases: &["ansi精简", "ansi精简内中", "ANSI精简花键_内_中", "ansilitecn"],
        label: "ANSI 花键参数表（内·纯中文·精简版）",
        group: Some("精简版"),
        summary: "ANSI B92.1 内花键精简卡（纯中文）：类型/齿数/径节/压力角/基圆/节圆/大径/小径 + 跨棒距 M / 量棒 Dp，不含公差列",
        systems: &[],
        direction: Some("int"),
        renderer: CardRenderer::CardLite,
        form: Some(&crate::ansi_table::FORM),
    },
    CardTypeSpec {
        id: "ANSI花键精简表_外_中文",
        aliases: &["ansi精简外中", "ANSI精简花键_外_中", "ansilitecnext"],
        label: "ANSI 花键参数表（外·纯中文·精简版）",
        group: Some("精简版"),
        summary: "ANSI B92.1 外花键精简卡（纯中文）：类型/齿数/径节/压力角/基圆/节圆/大径/小径 + 公法线 W / 跨测齿数 K，不含公差列",
        systems: &[],
        direction: Some("ext"),
        renderer: CardRenderer::CardLite,
        form: Some(&crate::ansi_table::FORM),
    },
    CardTypeSpec {
        id: "ANSI花键精简表_内_英文",
        aliases: &["ansi精简内英", "ANSI精简花键_内_英", "ansiliteen"],
        label: "ANSI 花键参数表（内·纯英文·精简版）",
        group: Some("精简版"),
        summary: "ANSI B92.1 内花键精简卡（纯英文）：SPLINE TYPE/TEETH/PITCH/ALPHA/base·pitch dia + PIN DIST. / PIN DIA.，不含公差列",
        systems: &[],
        direction: Some("int"),
        renderer: CardRenderer::CardLite,
        form: Some(&crate::ansi_table::FORM),
    },
    CardTypeSpec {
        id: "ANSI花键精简表_外_英文",
        aliases: &["ansi精简外英", "ANSI精简花键_外_英", "ansiliteenext"],
        label: "ANSI 花键参数表（外·纯英文·精简版）",
        group: Some("精简版"),
        summary: "ANSI B92.1 外花键精简卡（纯英文）：SPLINE TYPE/TEETH/PITCH/ALPHA/base·pitch dia + BASE TANG. / SPAN TEETH，不含公差列",
        systems: &[],
        direction: Some("ext"),
        renderer: CardRenderer::CardLite,
        form: Some(&crate::ansi_table::FORM),
    },
    CardTypeSpec {
        id: "齿轮精简表",
        aliases: &["gear精简", "齿轮精简", "精简齿轮", "GEAR精简表", "gearlite"],
        label: "齿轮参数表（GB/T 10095·精简版）",
        group: Some("精简版"),
        summary: "齿轮精简卡：模数/齿数/压力角/变位 + 分度圆/齿顶圆/齿根圆 + 公法线 W / 跨齿数 K（公差与未收项一律不上卡）",
        systems: &[],
        direction: None,
        renderer: CardRenderer::CardLite,
        form: Some(&crate::gear_table::FORM),
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
                    "group": c.group,
                    "summary": c.summary,
                    "systems": systems,
                    "direction": c.direction,
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
         * GB 花键参数表（内/外两张卡，一卡一方向）：`OCSMCARD 花键参数表 [std GB] <九字段表达式> [6H|5f] [dp 4.5] [root 平|圆] [at x,y] [rot 度]`（旧 `内/外` 记号兼容；外卡表下方 `花键参数表_外`）\n\
         * 齿轮参数表：`OCSMCARD 齿轮参数表 <九字段表达式> [mate z₂] [dwg 图号] [grade 精度等级] [center a] [at x,y] [rot 度]`\n\
         * ANSI 花键参数表（内/外 × 中/英 = 四张卡）：`OCSMCARD ANSI花键参数表_中文 <九字段表达式> [profile ANSI30R] [at x,y]`（外卡 `ANSI花键参数表_外_中文`；英文版换 `_英文`；旧写法 `内 P16 Z20` 兼容）\n\
         * NF 内/外花键参数表（两张）：`OCSMCARD NF内花键参数表 <九字段表达式> [中心 外径|齿面] [根 平|圆] [配合 松动|滑动|固定|压] [at x,y]`（外卡 `OCSMCARD NF外花键参数表`；旧写法 `A300 M7.5 Z38` 兼容）\n\
         * DIN 5480 内/外花键参数表（两张，单栏 Bild 6）：`OCSMCARD DIN花键参数表 <九字段表达式> [N9H] [ae …] [as …] [at x,y]`（外卡 `DIN花键参数表_外`；旧写法 `M3 Z38 B120 N9H W8f` 兼容）\n\
         * 精简版（下拉「精简版」分组，11 张：GB/NF/DIN/ANSI×4/齿轮）：只列基本参数 + 主要测量量、\
         **不含公差列**；参数/查表/公式与完整卡同源 —— `OCSMCARD <精简卡名> <对应完整卡参数>`\
         （例 `OCSMCARD NF花键精简表_内 A300 M7.5 Z38`、`OCSMCARD 齿轮精简表 <九字段表达式>`）"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表驱动完整性：22 张卡（一卡一方向 + 精简版 11 张）、体系 id 可解析、别名/JSON 形状。
    #[test]
    fn card_types_resolve_systems_and_tokens() {
        assert_eq!(
            CARD_TYPES.len(),
            22,
            "一卡一方向 13 张 + 精简版扩体系 9 张（NF/DIN/ANSI×4/齿轮）= 22"
        );
        // 首卡 = GB 内（旧 id 保持不变：深链 ?card= 与 CLI 旧写法兼容）
        let c = &CARD_TYPES[0];
        assert_eq!(c.id, "花键参数表", "旧 id 保持");
        assert_eq!(c.direction, Some("int"));
        assert_eq!(c.label, "GB 花键参数表（内）");
        assert_eq!(c.renderer, CardRenderer::SplineTable);
        for id in c.systems {
            assert!(
                crate::spline_gui::system_by_id(id).is_some(),
                "卡类型「{}」的体系 {id} 不在选项表",
                c.id
            );
        }
        let want = [
            ("花键参数表_外", Some("ext"), CardRenderer::SplineTable, "GB 花键参数表（外）"),
            ("齿轮参数表", None, CardRenderer::GearTable, "齿轮参数表（GB/T 10095）"),
            ("ANSI花键参数表_中文", Some("int"), CardRenderer::AnsiTableCn, "ANSI 花键参数表（内·纯中文）"),
            ("ANSI花键参数表_外_中文", Some("ext"), CardRenderer::AnsiTableCn, "ANSI 花键参数表（外·纯中文）"),
            ("ANSI花键参数表_英文", Some("int"), CardRenderer::AnsiTableEn, "ANSI 花键参数表（内·纯英文）"),
            ("ANSI花键参数表_外_英文", Some("ext"), CardRenderer::AnsiTableEn, "ANSI 花键参数表（外·纯英文）"),
            ("NF内花键参数表", Some("int"), CardRenderer::NfTable, "NF E22-141 内花键参数表"),
            ("NF外花键参数表", Some("ext"), CardRenderer::NfExtTable, "NF E22-141 外花键参数表"),
            ("DIN花键参数表", Some("int"), CardRenderer::DinTable, "DIN 5480 内花键参数表"),
            ("DIN花键参数表_外", Some("ext"), CardRenderer::DinTable, "DIN 5480 外花键参数表"),
            ("GB花键精简表_内", Some("int"), CardRenderer::SplineLite, "GB 花键参数表（内·精简版）"),
            ("GB花键精简表_外", Some("ext"), CardRenderer::SplineLite, "GB 花键参数表（外·精简版）"),
            ("NF花键精简表_内", Some("int"), CardRenderer::CardLite, "NF E22-141 内花键参数表（精简版）"),
            ("NF花键精简表_外", Some("ext"), CardRenderer::CardLite, "NF E22-141 外花键参数表（精简版）"),
            ("DIN花键精简表_内", Some("int"), CardRenderer::CardLite, "DIN 5480 内花键参数表（精简版）"),
            ("DIN花键精简表_外", Some("ext"), CardRenderer::CardLite, "DIN 5480 外花键参数表（精简版）"),
            ("ANSI花键精简表_内_中文", Some("int"), CardRenderer::CardLite, "ANSI 花键参数表（内·纯中文·精简版）"),
            ("ANSI花键精简表_外_中文", Some("ext"), CardRenderer::CardLite, "ANSI 花键参数表（外·纯中文·精简版）"),
            ("ANSI花键精简表_内_英文", Some("int"), CardRenderer::CardLite, "ANSI 花键参数表（内·纯英文·精简版）"),
            ("ANSI花键精简表_外_英文", Some("ext"), CardRenderer::CardLite, "ANSI 花键参数表（外·纯英文·精简版）"),
            ("齿轮精简表", None, CardRenderer::CardLite, "齿轮参数表（GB/T 10095·精简版）"),
        ];
        for (i, (id, dir, renderer, label)) in want.iter().enumerate() {
            let c = &CARD_TYPES[i + 1];
            assert_eq!(c.id, *id);
            assert_eq!(c.direction, *dir, "{} 方向", id);
            assert_eq!(c.renderer, *renderer, "{id} 渲染器");
            assert_eq!(c.label, *label, "{id} 显示名");
            if !id.starts_with("花键参数表") && !id.starts_with("GB花键精简表") {
                assert!(c.systems.is_empty(), "{id} 不应有体系");
            }
        }
        // 一卡一方向：有方向的卡必须与卡名一致；齿轮无方向
        for c in CARD_TYPES {
            match c.direction {
                None => assert!(
                    c.id == "齿轮参数表" || c.id == "齿轮精简表",
                    "只有齿轮两张卡无方向：{}",
                    c.id
                ),
                Some(d) => assert!(d == "int" || d == "ext", "{} direction={d}", c.id),
            }
            assert!(
                c.label.contains("GB") || c.label.contains("ANSI") || c.label.contains("NF") || c.label.contains("DIN"),
                "卡名应带标准号：{}",
                c.label
            );
        }
        // 同两侧的卡名/方向不混：ANSI 四张各两内两外；GB/NF/DIN 各一内一外
        for id in ["花键参数表", "NF内花键参数表"] {
            assert_eq!(card_type_by_token(id).unwrap().direction, Some("int"));
        }
        // 表驱动 GUI：花键卡无 form（专用面板基准）；其余全部有字段清单。
        assert!(CARD_TYPES[0].form.is_none(), "花键卡保持专用面板");
        assert!(CARD_TYPES[1].form.is_none(), "GB 外卡同专用面板");
        for c in &CARD_TYPES[2..] {
            let f = c.form.unwrap_or_else(|| panic!("{} 缺 form 字段清单", c.id));
            assert!(!f.fields.is_empty(), "{} form 为空", c.id);
            assert!(
                f.fields.iter().all(|x| !x.kind.is_empty() && !x.label.is_empty()),
                "{} form 字段缺 kind/label",
                c.id
            );
        }
        assert_eq!(CARD_TYPES[2].form.unwrap().fields.len(), 5, "齿轮 5 字段");
        for i in 3..=6 {
            assert_eq!(CARD_TYPES[i].form.unwrap().fields.len(), 4, "ANSI 4 字段（去方向）");
            assert!(
                !CARD_TYPES[i]
                    .form
                    .unwrap()
                    .fields
                    .iter()
                    .any(|x| x.key == "side"),
                "ANSI 面板不再放方向字段"
            );
        }
        assert_eq!(CARD_TYPES[7].form.unwrap().fields.len(), 7, "NF 内 7 字段");
        assert_eq!(CARD_TYPES[8].form.unwrap().fields.len(), 7, "NF 外 7 字段");
        assert_eq!(CARD_TYPES[11].form.unwrap().fields.len(), 5, "GB 精简内 5 字段");
        assert_eq!(CARD_TYPES[12].form.unwrap().fields.len(), 4, "GB 精简外 4 字段");
        for c in &CARD_TYPES[11..] {
            assert_eq!(c.group, Some("精简版"), "精简卡下拉分组：{}", c.id);
        }
        for c in &CARD_TYPES[..11] {
            assert!(c.group.is_none(), "普通卡不分组：{}", c.id);
        }
        for i in [9usize, 10] {
            let f = CARD_TYPES[i].form.unwrap();
            assert_eq!(f.fields.len(), 10, "DIN 单栏 10 字段（含 expr）");
            let has = |k: &str| f.fields.iter().any(|x| x.key == k);
            if CARD_TYPES[i].direction == Some("int") {
                assert!(has("hub") && !has("shaft"), "DIN 内卡只留孔配合");
                assert!(has("tact_n") && !has("tact_w"), "DIN 内卡只留 N 侧公差覆盖");
            } else {
                assert!(has("shaft") && !has("hub"), "DIN 外卡只留轴配合");
                assert!(has("tact_w") && !has("tact_n"), "DIN 外卡只留 W 侧公差覆盖");
            }
        }
        // 非花键卡都有表达式输入框（textarea）
        for c in &CARD_TYPES[2..] {
            let f = c.form.unwrap();
            let e = f.fields.iter().find(|x| x.key == "expr");
            let e = e.unwrap_or_else(|| panic!("{} 缺 expr 字段", c.id));
            assert_eq!(e.kind, "textarea", "{} expr 应为 textarea", c.id);
        }
        assert!(
            CARD_TYPES[7].form.unwrap().fields.iter().any(|x| x.key == "fit"),
            "NF 配合类别字段"
        );
        assert!(
            CARD_TYPES[8].form.unwrap().fields.iter().any(|x| x.key == "centering"),
            "NF 外定心字段"
        );
        // 记号/别名（大小写不敏感；旧名全部可命中）
        for tok in [
            "花键参数表", "SPLINE", "splinetable", "花键", "gb", "GB花键参数表", "GB花键",
            "齿轮", "GEAR", "ansicn", "ANSI花键参数表_英文", "ansien", "NF内花键参数表", "nf",
            "NFINT", "NF内花键", "NF外花键参数表", "NF外花键", "nfext", "NFout",
            "DIN花键参数表", "din", "DIN5480", "din花键",
            "splineext", "GB花键参数表_外", "ansicnext", "ANSI外中文",
            "ansienext", "dinext", "DIN外花键",
            "GB花键精简表_内", "GB花键精简表_外", "精简内", "精简外", "splinelite",
        ] {
            assert!(card_type_by_token(tok).is_some(), "别名/记号 {tok} 应命中");
        }
        assert!(card_type_by_token("铭牌").is_none(), "本期没有铭牌卡");
        assert!(card_type_by_token("  ").is_none());
        // 下发的 JSON 形状（GUI 只渲染）
        let j = card_types_json();
        assert_eq!(j[0]["id"], "花键参数表");
        assert_eq!(j[0]["direction"], "int");
        assert_eq!(j[0]["systems"][0]["id"], "gb3478");
        assert_eq!(j[0]["renderer"], "spline_table");
        assert_eq!(j[1]["id"], "花键参数表_外");
        assert_eq!(j[1]["direction"], "ext");
        assert_eq!(j[2]["renderer"], "gear_table");
        assert!(j[2]["direction"].is_null());
        assert_eq!(j[3]["renderer"], "ansi_table_cn");
        assert_eq!(j[4]["id"], "ANSI花键参数表_外_中文");
        assert_eq!(j[5]["renderer"], "ansi_table_en");
        assert_eq!(j[7]["id"], "NF内花键参数表");
        assert_eq!(j[7]["renderer"], "nf_table");
        assert_eq!(j[8]["id"], "NF外花键参数表");
        assert_eq!(j[8]["renderer"], "nf_ext_table");
        assert_eq!(j[9]["id"], "DIN花键参数表");
        assert_eq!(j[9]["direction"], "int");
        assert_eq!(j[10]["id"], "DIN花键参数表_外");
        assert_eq!(j[10]["renderer"], "din_table");
        assert_eq!(j[11]["id"], "GB花键精简表_内");
        assert_eq!(j[11]["renderer"], "spline_lite");
        assert_eq!(j[11]["direction"], "int");
        assert_eq!(j[11]["group"], "精简版");
        assert_eq!(j[12]["id"], "GB花键精简表_外");
        assert_eq!(j[12]["direction"], "ext");
        assert_eq!(j[12]["group"], "精简版");
        assert!(j[0]["group"].is_null(), "普通卡 group = null");
        // form 下发：花键 null；其余含 fields/note/missing_note
        assert!(j[0]["form"].is_null());
        assert!(j[1]["form"].is_null());
        assert_eq!(j[2]["form"]["fields"].as_array().unwrap().len(), 5);
        assert!(j[7]["form"]["fields"].as_array().unwrap().iter().any(|f| f["key"] == "fit"));
        assert!(j[7]["form"]["missing_note"].as_str().unwrap().contains("p29"));
        assert_eq!(j[8]["form"]["fields"].as_array().unwrap().len(), 7);
        assert_eq!(j[9]["form"]["fields"].as_array().unwrap().len(), 10);
        assert!(j[9]["summary"].as_str().unwrap().contains("Bild 6"));
        assert!(j[9]["summary"].as_str().unwrap().contains("内花键"));
        assert!(j[10]["summary"].as_str().unwrap().contains("外花键"));
        assert!(j[8]["summary"].as_str().unwrap().contains("NF E22-141"));
        assert!(j[0]["summary"].as_str().unwrap().contains("GB/T 3478"));
        assert!(usage_line().contains("OCSMCARD 花键参数表"));
        assert!(usage_line().contains("OCSMCARD 齿轮参数表"));
        assert!(usage_line().contains("ANSI花键参数表_中文"));
        assert!(usage_line().contains("ANSI花键参数表_外_中文"));
        assert!(usage_line().contains("OCSMCARD NF内花键参数表"));
        assert!(usage_line().contains("OCSMCARD NF外花键参数表"));
        assert!(usage_line().contains("OCSMCARD DIN花键参数表"));
        assert!(usage_line().contains("DIN花键参数表_外"));
    }
}
