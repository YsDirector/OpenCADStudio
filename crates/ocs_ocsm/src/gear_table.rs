//! 智能卡片「齿轮参数表」（`OCSMCARD` 第二张卡）：**表格块几何内建**（模板逐图元照录，
//! 不依赖外部 DXF）+ 19 个属性值映射 + 命令参数解析（吃**九字段齿形表达式**）。
//!
//! 模板：`~/桌面/GB/参数表/齿轮参数表.dxf`（27 LINE + 46 TEXT + 18 ATTDEF；几何见下方
//! const，由 `ezdxf` 逐图元反解）。表格块与取值无关（标签/符号固定），只建一次块
//! （`OCSM_GEARTABLE_GB`），每次插入用 `INSERT.attributes` 填值。
//!
//! **19 个属性**：模板 18 个 ATTDEF 逐项照录（tag/位置/字高/层/对齐）；另把模板
//! `精度等级` 行的**样例 TEXT**（`887FHGB10095-88`）提升为第 19 个 ATTDEF
//! （`GRADE_TAG`）—— 样例值硬编码在块里会误导，提升为属性后可随卡片输入填写。
//!
//! 取值来源（用户 2026-09-26 口径：**复用既有齿轮引擎，不另算一遍**）：
//! * `mn/z/α/ha*/β/螺旋方向/x/全齿高`：九字段表达式反解（`shaft::parse_program`）→
//!   由 `DA/DF` 反解 `ha*`/`c*` → 交给 `gear::GearParams`（与齿轮生成器同一套
//!   `d/da/df/mt/alpha_t` 公式；`ha*/c*` 反解后引擎 `da/df` 与表达式闭环复核）；
//! * `公法线 W / 公法线 k`：`GearParams::span_measurement()`（**直齿外齿轮**才有；
//!   内齿轮/斜齿轮无此口径 → 如实标缺）；
//! * `中心距`：给了配对齿数按引擎 `mt(z1+z2)/2`（`center` 可显式覆盖）；极限偏差缺；
//! * `精度等级` 与 GB/T 10095-88 公差项目（Fr/FW/ff/fpt/Fβ）：**本仓未收该标准数据**
//!   → 按「来源可回溯」原则**如实标缺**（`MISSING`），不臆造。
//!
//! 排版口径：主值最多 3 位小数（显示与内部计算分开）；所有文本样式一律 `OCSM_GB`
//! （不动全局样式定义）；值属性用**实体级** `VALUE_WIDTH_FACTOR` 压缩防串列。

use crate::gear::{GearKind, GearParams, GearStd};
use ocs_plugin_api::host::acadrust::entities::{
    AttributeDefinition, AttributeEntity, Entity as _, EntityCommon, EntityType,
    HorizontalAlignment, Insert, Text, TextHorizontalAlignment, TextVerticalAlignment,
    VerticalAlignment,
};
use ocs_plugin_api::host::acadrust::types::{Color, LineWeight, Vector3};

/// 表格块名（齿轮参数表只有一种，不分内/外）。
pub const BLOCK: &str = "OCSM_GEARTABLE_GB";

/// 缺项标记（本仓没有的数据一律显示它，并在 GUI/回执里点明原因 —— 不臆造）。
pub const MISSING: &str = "—";

/// 模板 `精度等级` 行的样例 TEXT 提升成的属性 tag（第 19 个属性）。
pub const GRADE_TAG: &str = "精度等级";

/// 值属性**实体级**字宽压缩（不动全局 `OCSM_GB` 样式；模板值列宽 ~20）。
pub const VALUE_WIDTH_FACTOR: f64 = 0.7;

// ══════════════════════════════════════════════════════════════════════════
// 模板数据（逐图元反解；不依赖外部 DXF）
// ══════════════════════════════════════════════════════════════════════════

/// 模板 `~/桌面/GB/参数表/齿轮参数表.dxf` 的 27 条 LINE（逐图元照录；`(a, b, layer)`）。
const GEAR_LINES: &[([f64; 2], [f64; 2], &str)] = &[
    ([-40.0, -114.0], [0.0, -114.0], "2细线层"),
    ([-80.0, -108.0], [0.0, -108.0], "2细线层"),
    ([-80.0, -102.0], [0.0, -102.0], "2细线层"),
    ([-80.0, -96.00000000000001], [0.0, -96.00000000000001], "2细线层"),
    ([-80.0, -90.0], [0.0, -90.0], "2细线层"),
    ([-80.0, -84.0], [0.0, -84.0], "2细线层"),
    ([-80.0, -78.0], [0.0, -78.0], "2细线层"),
    ([-80.0, -72.0], [0.0, -72.0], "2细线层"),
    ([-40.0, -66.0], [0.0, -66.0], "2细线层"),
    ([-80.0, -60.0], [0.0, -60.0], "2细线层"),
    ([-80.0, -54.0], [0.0, -54.0], "2细线层"),
    ([-80.0, -48.0], [0.0, -48.0], "2细线层"),
    ([-80.0, -42.0], [0.0, -42.0], "2细线层"),
    ([-80.0, -36.0], [0.0, -36.0], "2细线层"),
    ([-80.0, -30.0], [0.0, -30.0], "2细线层"),
    ([-80.0, -24.0], [0.0, -24.0], "2细线层"),
    ([-80.0, -18.0], [0.0, -18.0], "2细线层"),
    ([-80.0, -12.0], [0.0, -12.0], "2细线层"),
    ([-80.0, -6.0], [0.0, -6.0], "2细线层"),
    ([-80.0, 0.0], [0.0, 0.0], "2细线层"),
    ([-80.0, 0.0], [-80.0, -120.0], "0"),
    ([-40.0, 0.0], [-40.0, -120.0], "2细线层"),
    ([-20.0, 0.0], [-20.0, -30.0], "2细线层"),
    ([0.0, 0.0], [0.0, -120.0], "0"),
    ([-20.0, -36.0], [-20.0, -48.0], "2细线层"),
    ([-20.0, -54.0], [-20.0, -120.0], "2细线层"),
    ([-80.0, -120.0], [0.0, -120.0], "2细线层"),
];

/// 模板 46 条 TEXT（`(value, ins_x, ins_y, h, wf, layer, halign, valign, align_x, align_y)`）。
/// 模板里的样例 TEXT `887FHGB10095-88`（精度等级行）**提升为第 19 个 ATTDEF**，见 `GEAR_SAMPLE_GRADE`。
const GEAR_TEXTS: &[(&str, f64, f64, f64, f64, &str, i16, i16, f64, f64)] = &[
    ("法向模数", -66.20295433412753, -5.088443562470388, 3.5, 0.6669999957084656, "6文字层", 0, 0, -66.20295433412753, -5.088443562470388),
    ("齿数", -66.20295433412753, -11.08844356247039, 3.5, 0.6669999957084656, "6文字层", 0, 0, -66.20295433412753, -11.08844356247039),
    ("齿形角", -66.20295433412753, -17.08844356247039, 3.5, 0.6669999957084656, "6文字层", 0, 0, -66.20295433412753, -17.08844356247039),
    ("齿顶高系数", -66.20295433412753, -23.08844356247039, 3.5, 0.6669999957084656, "6文字层", 0, 0, -66.20295433412753, -23.08844356247039),
    ("螺旋角", -66.20295433412753, -29.08844356247039, 3.5, 0.6669999957084656, "6文字层", 0, 0, -66.20295433412753, -29.08844356247039),
    ("螺旋方向", -66.20295433412753, -35.08844356247039, 3.5, 0.6669999957084656, "6文字层", 0, 0, -66.20295433412753, -35.08844356247039),
    ("径向变位系数", -66.20295433412753, -41.08844356247039, 3.5, 0.6669999957084656, "6文字层", 0, 0, -66.20295433412753, -41.08844356247039),
    ("全齿高", -66.20295433412753, -47.08844356247039, 3.5, 0.6669999957084656, "6文字层", 0, 0, -66.20295433412753, -47.08844356247039),
    ("精度等级", -66.20295433412753, -53.08844356247039, 3.5, 0.6669999957084656, "6文字层", 0, 0, -66.20295433412753, -53.08844356247039),
    ("齿轮副中心距及其极限偏差", -77.6116580526807, -59.08844356247039, 3.5, 0.6669999957084656, "6文字层", 0, 0, -77.6116580526807, -59.08844356247039),
    ("配对齿轮", -66.20295433412753, -67.69694562274663, 3.5, 0.6669999957084656, "6文字层", 0, 0, -66.20295433412753, -67.69694562274663),
    ("公差组", -66.20295433412753, -77.08844356247039, 3.5, 0.6669999957084656, "6文字层", 0, 0, -66.20295433412753, -77.08844356247039),
    ("公法线长度变动公差", -73.0420936167925, -89.08844356247039, 3.5, 0.6669999957084656, "6文字层", 0, 0, -73.0420936167925, -89.08844356247039),
    ("齿形公差", -66.20295433412753, -95.08844356247037, 3.5, 0.6669999957084656, "6文字层", 0, 0, -66.20295433412753, -95.08844356247037),
    ("齿距极限偏差", -66.20295433412753, -101.0884435624704, 3.5, 0.6669999957084656, "6文字层", 0, 0, -66.20295433412753, -101.0884435624704),
    ("齿向公差", -66.20295433412753, -107.0884435624704, 3.5, 0.6669999957084656, "6文字层", 0, 0, -66.20295433412753, -107.0884435624704),
    ("公法线", -66.20295433412753, -115.6516360087796, 3.5, 0.6669999957084656, "6文字层", 0, 0, -66.20295433412753, -115.6516360087796),
    ("齿圈径向跳动公差", -70.96009359332923, -83.08844356247039, 3.5, 0.6669999957084656, "6文字层", 0, 0, -70.96009359332923, -83.08844356247039),
    ("mn", -32.2197526123839, -4.45836311807659, 3.5, 0.6669999957084656, "6文字层", 0, 0, -32.2197526123839, -4.45836311807659),
    ("z", -32.2197526123839, -10.45836311807659, 3.5, 0.6669999957084656, "6文字层", 0, 0, -32.2197526123839, -10.45836311807659),
    ("α", -32.2197526123839, -16.45836311807659, 3.5, 0.6669999957084656, "6文字层", 0, 0, -32.2197526123839, -16.45836311807659),
    ("h", -32.2197526123839, -22.45836311807659, 3.5, 0.6669999957084656, "6文字层", 0, 0, -32.2197526123839, -22.45836311807659),
    ("β", -32.2197526123839, -28.45836311807659, 3.5, 0.6669999957084656, "6文字层", 0, 0, -32.2197526123839, -28.45836311807659),
    ("α", -29.93497039443992, -23.24127435706646, 2.0, 0.6669999957084656, "6文字层", 0, 0, -29.93497039443992, -23.24127435706646),
    ("*", -29.72726350799894, -20.1280848934573, 2.0, 0.6669999957084656, "6文字层", 0, 0, -29.72726350799894, -20.1280848934573),
    ("x", -32.2197526123839, -40.45836311807659, 3.5, 0.6669999957084656, "6文字层", 0, 0, -32.2197526123839, -40.45836311807659),
    ("h", -32.2197526123839, -46.45836311807658, 3.5, 0.6669999957084656, "6文字层", 0, 0, -32.2197526123839, -46.45836311807658),
    ("a±f", -33.24525585049878, -59.13996194129788, 3.5, 0.6669999957084656, "6文字层", 0, 0, -33.24525585049878, -59.13996194129788),
    ("α", -26.40394362430334, -59.56182201771219, 2.0, 0.6669999957084656, "6文字层", 0, 0, -26.40394362430334, -59.56182201771219),
    ("图号", -32.41515046354744, -65.4360503074116, 3.5, 0.6669999957084656, "6文字层", 0, 0, -32.41515046354744, -65.4360503074116),
    ("齿数", -32.41515046354744, -71.1435645545306, 3.5, 0.6669999957084656, "6文字层", 0, 0, -32.41515046354744, -71.1435645545306),
    ("检验项目代号", -38.95808068055612, -77.12652817287142, 3.5, 0.6669999957084656, "6文字层", 0, 0, -38.95808068055612, -77.12652817287142),
    ("F", -33.24612459677155, -82.93781618581863, 3.5, 0.6669999957084656, "6文字层", 0, 0, -33.24612459677155, -82.93781618581863),
    ("r", -31.58446519384825, -83.14536164295419, 2.5, 0.6669999957084656, "6文字层", 0, 0, -31.58446519384825, -83.14536164295419),
    ("F", -33.24612459677155, -88.93781618581863, 3.5, 0.6669999957084656, "6文字层", 0, 0, -33.24612459677155, -88.93781618581863),
    ("W", -31.58446519384825, -89.14536164295419, 2.5, 0.6669999957084656, "6文字层", 0, 0, -31.58446519384825, -89.14536164295419),
    ("f", -33.24612459677155, -94.93781618581863, 3.5, 0.6669999957084656, "6文字层", 0, 0, -33.24612459677155, -94.93781618581863),
    ("f", -31.58446519384825, -95.14536164295419, 2.5, 0.6669999957084656, "6文字层", 0, 0, -31.58446519384825, -95.14536164295419),
    ("f", -33.24612459677155, -100.9378161858186, 3.5, 0.6669999957084656, "6文字层", 0, 0, -33.24612459677155, -100.9378161858186),
    ("pt", -31.58446519384825, -101.1453616429542, 2.5, 0.6669999957084656, "6文字层", 0, 0, -31.58446519384825, -101.1453616429542),
    ("F", -33.24612459677155, -106.9378161858186, 3.5, 0.6669999957084656, "6文字层", 0, 0, -33.24612459677155, -106.9378161858186),
    ("β", -31.58446519384825, -107.1453616429542, 2.5, 0.6669999957084656, "6文字层", 0, 0, -31.58446519384825, -107.1453616429542),
    ("W", -33.24612459677155, -112.9378161858186, 3.5, 0.6669999957084656, "6文字层", 0, 0, -33.24612459677155, -112.9378161858186),
    ("kn", -30.54592860594562, -113.1453616429542, 2.5, 0.6669999957084656, "6文字层", 0, 0, -30.54592860594562, -113.1453616429542),
    ("k", -31.89602606243397, -118.8736314730472, 3.5, 0.6669999957084656, "6文字层", 0, 0, -31.89602606243397, -118.8736314730472),
    // 表头 `公差(或极限偏差)值` 实体级字宽 0.6（模板 0.819 会与中列表头 `检验项目代号` 叠字；
    // OCS 截图/DXF 干涉检查后按「调整字宽」口径降档，位置/字高照模板）。
    ("公差(或极限偏差)值", -19.50848117938926, -77.08844356247039, 3.5, 0.6, "6文字层", 5, 0, -0.6802058188077922, -77.08844356247039),
];

/// 模板 18 个 ATTDEF（`(tag, ins_x, ins_y, align_x, align_y, h, wf, layer, halign)`）。
const GEAR_ATTDEFS: &[(&str, f64, f64, f64, f64, f64, f64, &str, i16)] = &[
    ("法向模数", -17.96762215397962, -4.881056227080307, -0.6408894807123034, -4.881056227080307, 3.5, 1.739708854606389, "6文字层", 5),
    ("齿数", -18.56736156419515, -10.88105622708031, -2.052275648351042, -10.88105622708031, 3.5, 3.395391813385939, "6文字层", 5),
    ("齿形角", -16.12007591559245, -17.10987742947594, -3.54086799480035, -17.10987742947594, 3.5, 1.742574257425745, "6文字层", 5),
    ("齿顶高系数", -19.30481712288178, -23.33763911508106, -0.7809348120244409, -23.33763911508106, 3.5, 1.481039385230216, "6文字层", 5),
    ("螺旋角", -14.92369122528555, -28.85889893252556, -2.344483304493451, -28.85889893252556, 3.5, 1.742574257425745, "6文字层", 5),
    ("螺旋方向", -29.03628050233664, -35.08738252949161, -12.16004287857424, -35.08738252949161, 3.5, 1.727964100757903, "6文字层", 5),
    ("径向变位系数", -18.85993636494896, -41.18296916885685, -0.8952117152880987, -40.68155142008532, 3.5, 1.193726026905851, "6文字层", 5),
    ("全齿高", -15.83169881496394, -46.86687082753872, -3.252490894171842, -46.86687082753872, 3.5, 1.742574257425745, "6文字层", 5),
    ("中心距及极限偏差", -19.06065950348477, -59.63657070906544, -0.5941267868547015, -59.63657070906544, 3.5, 0.9164197954612989, "6文字层", 5),
    ("配对齿轮图号", -18.9478342679119, -65.15581051386005, -1.414602567732174, -65.07350623048774, 3.5, 1.170616486499716, "6文字层", 5),
    ("配对齿轮齿数", -18.6684200270484, -71.15854162661856, -1.344749007516213, -71.15663328150731, 3.5, 1.150680963590394, "6文字层", 5),
    ("齿圈径向跳动公差", -18.90729408262439, -83.05634669014171, -0.9973120669952209, -83.24562530975373, 3.5, 0.8888500546424838, "6文字层", 5),
    ("公法线长度公差", -19.5470835047368, -89.37866148797008, -0.3911894089381462, -89.37866148797008, 3.5, 1.088221218474777, "6文字层", 5),
    ("齿形公差", -19.46921402730493, -94.98090510451107, -0.7026673186656467, -94.98090510451107, 3.5, 1.884274900239898, "6文字层", 5),
    ("齿距极限偏差", -19.29664146978376, -100.8810562270803, -0.9973120669952209, -100.8810562270803, 3.5, 1.215486592810374, "6文字层", 5),
    ("齿向公差", -19.14187990436369, -106.8810562270803, -0.8488102727915247, -106.8810562270803, 3.5, 1.836734935322122, "6文字层", 5),
    ("公法线", -17.96762215397962, -112.8810562270803, -1.543375798462022, -112.8810562270803, 3.5, 2.227571682224276, "6文字层", 5),
    ("公法线K", -17.96762215397962, -118.8810562270802, -1.621245275893841, -118.8810562270802, 3.5, 1.645186234843752, "6文字层", 5),
];

/// 模板 `精度等级` 行的样例 TEXT 位置（提升为属性用）：`(x, y, h, wf)`。
const GEAR_SAMPLE_GRADE: (f64, f64, f64, f64) = (-33.76452468337471, -52.91358232257266, 3.0, 0.6669999957084656);


/// 模板 18 个 ATTDEF 的 tag 顺序（文档顺序；`attdefs()` 在其中插入 `GRADE_TAG`）。
pub const TEMPLATE_TAGS: &[&str] = &[
    "法向模数",
    "齿数",
    "齿形角",
    "齿顶高系数",
    "螺旋角",
    "螺旋方向",
    "径向变位系数",
    "全齿高",
    "中心距及极限偏差",
    "配对齿轮图号",
    "配对齿轮齿数",
    "齿圈径向跳动公差",
    "公法线长度公差",
    "齿形公差",
    "齿距极限偏差",
    "齿向公差",
    "公法线",
    "公法线K",
];

fn halign_of(v: i16) -> HorizontalAlignment {
    match v {
        1 => HorizontalAlignment::Center,
        2 => HorizontalAlignment::Right,
        3 => HorizontalAlignment::Aligned,
        4 => HorizontalAlignment::Middle,
        5 => HorizontalAlignment::Fit,
        _ => HorizontalAlignment::Left,
    }
}

fn text_halign_of(v: i16) -> TextHorizontalAlignment {
    match v {
        1 => TextHorizontalAlignment::Center,
        2 => TextHorizontalAlignment::Right,
        3 => TextHorizontalAlignment::Aligned,
        4 => TextHorizontalAlignment::Middle,
        5 => TextHorizontalAlignment::Fit,
        _ => TextHorizontalAlignment::Left,
    }
}

fn text_valign_of(v: i16) -> TextVerticalAlignment {
    match v {
        1 => TextVerticalAlignment::Bottom,
        2 => TextVerticalAlignment::Middle,
        3 => TextVerticalAlignment::Top,
        _ => TextVerticalAlignment::Baseline,
    }
}

fn common_of(layer: &str) -> EntityCommon {
    let mut c = EntityCommon::default();
    c.layer = layer.to_string();
    c.color = Color::ByLayer;
    c.linetype = "ByLayer".into();
    c
}

fn text_ent(
    value: &str,
    x: f64,
    y: f64,
    h: f64,
    wf: f64,
    layer: &str,
    halign: i16,
    valign: i16,
    ax: f64,
    ay: f64,
) -> EntityType {
    let mut t = Text::with_value(value, Vector3::new(x, y, 0.0));
    t.height = h;
    t.width_factor = wf;
    t.style = "OCSM_GB".into();
    t.horizontal_alignment = text_halign_of(halign);
    t.vertical_alignment = text_valign_of(valign);
    if halign != 0 || valign != 0 {
        t.alignment_point = Some(Vector3::new(ax, ay, 0.0));
    }
    t.common = common_of(layer);
    EntityType::Text(t)
}

fn attdef(
    tag: &str,
    x: f64,
    y: f64,
    ax: f64,
    ay: f64,
    h: f64,
    layer: &str,
    halign: i16,
) -> AttributeDefinition {
    let mut ad = AttributeDefinition::new(tag.to_string(), String::new(), " ".to_string());
    ad.insertion_point = Vector3::new(x, y, 0.0);
    ad.alignment_point = Vector3::new(ax, ay, 0.0);
    ad.height = h;
    ad.width_factor = VALUE_WIDTH_FACTOR;
    ad.text_style = "OCSM_GB".into();
    ad.horizontal_alignment = halign_of(halign);
    ad.vertical_alignment = VerticalAlignment::Baseline;
    ad.flags.preset = true;
    ad.common = common_of(layer);
    ad
}

/// 表格块成员：27 线 + 46 标签/符号 TEXT + 19 ATTDEF（18 模板项 + `精度等级`）。
pub fn block_entities() -> Vec<EntityType> {
    let mut out = Vec::with_capacity(GEAR_LINES.len() + GEAR_TEXTS.len() + 19);
    for (a, b, layer) in GEAR_LINES {
        out.push(crate::partgen_kit::line(*a, *b, layer));
    }
    for (v, x, y, h, wf, layer, ha, va, ax, ay) in GEAR_TEXTS {
        out.push(text_ent(v, *x, *y, *h, *wf, layer, *ha, *va, *ax, *ay));
    }
    for ad in attdefs() {
        out.push(EntityType::AttributeDefinition(ad));
    }
    out
}

/// 19 个 ATTDEF：模板 18 项（文档顺序）在 `全齿高` 之后插入 `精度等级`（模板样例行）。
pub fn attdefs() -> Vec<AttributeDefinition> {
    let mut out = Vec::with_capacity(19);
    for (tag, x, y, ax, ay, h, _wf, layer, halign) in GEAR_ATTDEFS {
        out.push(attdef(tag, *x, *y, *ax, *ay, *h, layer, *halign));
        if *tag == "全齿高" {
            let (gx, gy, gh, gwf) = GEAR_SAMPLE_GRADE;
            let mut grade = attdef(GRADE_TAG, gx, gy, gx, gy, gh, "6文字层", 0);
            grade.width_factor = gwf.max(VALUE_WIDTH_FACTOR);
            out.push(grade);
        }
    }
    out
}

// ══════════════════════════════════════════════════════════════════════════
// 取值（与齿轮引擎同源）
// ══════════════════════════════════════════════════════════════════════════

/// 主值/尺寸的**显示**格式：最多 3 位小数（截到 0.001 再抹尾零）。
pub fn fmt_mm(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn fmt_num(v: f64) -> String {
    crate::partgen_kit::trim(v)
}

fn fmt_deg(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}°", v.round() as i64)
    } else {
        format!("{}°", crate::partgen_kit::trim(v))
    }
}

/// 「齿轮参数表」卡参数（CLI 与 GUI 同一份字段）。
#[derive(Debug, Clone, PartialEq)]
pub struct GearTableSpec {
    /// 九字段表达式原文（回显）。
    pub expr: String,
    /// 表达式反解 + `DA/DF` 反解 `ha*/c*` 后的**同一套**齿轮参数。
    pub params: GearParams,
    /// 配对齿轮齿数（给中心距用；缺 = 中心距标缺）。
    pub mate_z: Option<u32>,
    /// 配对齿轮图号（手填字符串；缺 = 标缺）。
    pub mate_dwg: Option<String>,
    /// 精度等级（模板样例行；手填字符串；缺 = 标缺）。
    pub grade: Option<String>,
    /// 中心距显式覆盖（缺 = 由 `mate_z` 算；再缺 = 标缺）。
    pub center: Option<f64>,
    pub at: Option<[f64; 2]>,
    pub rot: f64,
}

/// 九字段表达式 → `GearParams`（**复用 `spline_table::parse_expr_gear` 的解析器**）。
///
/// `ha*/c*` 由表达式 `DA/DF` 反解（外齿 `ha*=(Da−d)/2m−x`；内齿取相反；两向
/// `c*=(|Da−Df|)/2m−2ha*`），再交回 `GearParams` 复核 `da/df` 是否闭环。
pub fn params_from_gear(g: &crate::shaft::Gear) -> Result<GearParams, String> {
    if !(g.m.is_finite() && g.m > 0.0) {
        return Err(format!("齿轮参数表：模数 m={} 非法", g.m));
    }
    let da = g.da.ok_or_else(|| {
        "齿轮参数表：表达式缺 `DA`（大径）—— 齿顶高系数要从它反解（轴/齿轮生成器 GUI 可复制完整九字段）"
            .to_string()
    })?;
    let df = g.df.ok_or_else(|| {
        "齿轮参数表：表达式缺 `DF`（小径）—— 全齿高与顶隙系数要从它反解".to_string()
    })?;
    let kind = if g.kind.is_internal() {
        GearKind::Internal
    } else {
        GearKind::External
    };
    let mut p = GearParams {
        kind,
        m: g.m,
        z: g.z,
        alpha_deg: g.alpha_deg,
        ha: 1.0,
        c: 0.25,
        beta_deg: g.beta_deg,
        h: g.h.unwrap_or(10.0 * g.m),
        x: g.x,
        spline: None,
        std: GearStd::M,
        dp: None,
    };
    // 反解（表达式口径：外齿 major=Da=齿顶、minor=Df=齿根；内齿 major=Da=外侧齿根、
    // minor=Df=里侧齿顶 —— 与 `gear_tooth_expr` 的内齿 (df, da) 取法一致）。
    let d = p.d();
    let ha = match kind {
        GearKind::External => (da - d) / (2.0 * g.m) - g.x,
        GearKind::Internal => (d - df) / (2.0 * g.m) - g.x,
    };
    let c = (da - df).abs() / (2.0 * g.m) - 2.0 * ha;
    p.ha = ha;
    p.c = c;
    p.validate()
        .map_err(|e| format!("齿轮参数表：表达式反解的齿形参数不合法：{e}"))?;
    // 闭环复核：反解参数交回引擎算出的 da/df 必须与表达式一致（否则表达式自相矛盾）。
    let tol = 1e-6 * da.abs().max(1.0);
    let (want_major, want_minor) = match kind {
        GearKind::External => (p.da(), p.df()),
        GearKind::Internal => (p.df(), p.da()),
    };
    if (want_major - da).abs() > tol || (want_minor - df).abs() > tol {
        return Err(format!(
            "齿轮参数表：表达式 DA/DF 与 M/Z/ALPHA/X 不自洽（反解 ha*={}、c*={} 后引擎给 Da={}、Df={}，表达式写 Da={}、Df={}）",
            fmt_num(ha),
            fmt_num(c),
            fmt_mm(want_major),
            fmt_mm(want_minor),
            fmt_mm(da),
            fmt_mm(df)
        ));
    }
    Ok(p)
}

/// 19 项取值（顺序 = `attdefs()`）。
pub fn values(spec: &GearTableSpec) -> Result<Vec<(String, String)>, String> {
    let p = &spec.params;
    let full_depth = (p.da() - p.df()).abs() / 2.0;
    let direction = if p.beta_deg.abs() < 1e-9 {
        "直齿".to_string()
    } else if p.beta_deg > 0.0 {
        "右旋".to_string()
    } else {
        "左旋".to_string()
    };
    let (k_text, w_text) = match p.span_measurement() {
        Some((k, w)) => (k.to_string(), fmt_mm(w)),
        None => (MISSING.to_string(), MISSING.to_string()),
    };
    let center_text = if let Some(a) = spec.center {
        fmt_mm(a)
    } else if let Some(z2) = spec.mate_z {
        fmt_mm(p.mt() * (p.z as f64 + z2 as f64) / 2.0)
    } else {
        MISSING.to_string()
    };
    let out: Vec<(&str, String)> = vec![
        ("法向模数", fmt_mm(p.m)),
        ("齿数", p.z.to_string()),
        ("齿形角", fmt_deg(p.alpha_deg)),
        ("齿顶高系数", fmt_num(p.ha)),
        ("螺旋角", fmt_deg(p.beta_deg.abs())),
        ("螺旋方向", direction),
        ("径向变位系数", fmt_num(p.x)),
        ("全齿高", fmt_mm(full_depth)),
        (GRADE_TAG, spec.grade.clone().unwrap_or_else(|| MISSING.to_string())),
        ("中心距及极限偏差", center_text),
        (
            "配对齿轮图号",
            spec.mate_dwg.clone().unwrap_or_else(|| MISSING.to_string()),
        ),
        (
            "配对齿轮齿数",
            spec.mate_z.map(|z| z.to_string()).unwrap_or_else(|| MISSING.to_string()),
        ),
        // GB/T 10095-88 公差项目：本仓未收 → 如实标缺（不臆造）。
        ("齿圈径向跳动公差", MISSING.to_string()),
        ("公法线长度公差", MISSING.to_string()),
        ("齿形公差", MISSING.to_string()),
        ("齿距极限偏差", MISSING.to_string()),
        ("齿向公差", MISSING.to_string()),
        ("公法线", w_text),
        ("公法线K", k_text),
    ];
    // 顺序必须与 attdefs() 一致（表驱动护栏：加/改属性时在这里暴露）。
    let tags = attdefs();
    let got: Vec<&str> = out.iter().map(|(t, _)| *t).collect();
    let want: Vec<&str> = tags.iter().map(|ad| ad.tag.as_str()).collect();
    if got != want {
        return Err(format!("齿轮参数表：取值映射顺序与模板属性不一致：{got:?} != {want:?}"));
    }
    Ok(out
        .into_iter()
        .map(|(t, v)| (t.to_string(), v))
        .collect())
}

/// 建 INSERT（基点在 `at`，旋转 `rot_deg` 度；19 个 `INSERT.attributes` 取自 `values()`）。
pub fn build_insert(spec: &GearTableSpec, at: [f64; 2], rot_deg: f64) -> Result<Insert, String> {
    let values = values(spec)?;
    let mut ins = Insert::new(BLOCK, Vector3::new(at[0], at[1], 0.0));
    ins.rotation = rot_deg.to_radians();
    {
        let c = &mut ins.common;
        c.layer = crate::partgen::LAYER_MAIN.to_string();
        c.color = Color::ByLayer;
        c.linetype = "ByLayer".to_string();
        c.line_weight = LineWeight::ByLayer;
    }
    for ad in attdefs() {
        let val = values
            .iter()
            .find(|(tag, _)| tag == &ad.tag)
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        let mut tmpl = ad.clone();
        tmpl.rotation = 0.0; // 旋转由 INSERT 变换施加
        let mut attr = AttributeEntity::from_definition(&tmpl, Some(val));
        attr.apply_transform(&ins.get_transform());
        ins.attributes.push(attr);
    }
    Ok(ins)
}

/// 19 项 Markdown（计算书/回执用；与表同一份取值）。
pub fn markdown_table(spec: &GearTableSpec) -> Result<String, String> {
    let vals = values(spec)?;
    let mut md = String::new();
    md.push_str("| 属性 | 值 |\n|---|---|\n");
    for (tag, v) in vals {
        md.push_str(&format!("| {tag} | {v} |\n"));
    }
    Ok(md)
}

// 模板数据的逐图元段落由 `// ══════════════════════════════════════════════════════════════════════════
// GUI 选项表（列口径）与表单模型（CLI/GUI/HTTP 同一份字段）
// ══════════════════════════════════════════════════════════════════════════

/// 一项属性的取值口径（进原生 `title=`，不做常显）。
#[derive(Debug, Clone, Copy)]
pub struct GearColumnSpec {
    pub tag: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub formula: &'static str,
    pub source: &'static str,
}

/// 19 项口径（顺序 = `attdefs()`；表驱动护栏见 `values()` 的顺序复核）。
pub const GEAR_COLUMNS: &[GearColumnSpec] = &[
    GearColumnSpec {
        tag: "法向模数",
        label: "法向模数 mn",
        unit: "mm",
        formula: "九字段表达式反解 `M`（斜齿即法向模数 Mn）",
        source: "轴/齿轮生成器同一份表达式",
    },
    GearColumnSpec {
        tag: "齿数",
        label: "齿数 z",
        unit: "",
        formula: "九字段表达式反解 `Z`",
        source: "轴/齿轮生成器同一份表达式",
    },
    GearColumnSpec {
        tag: "齿形角",
        label: "齿形角 α",
        unit: "°",
        formula: "九字段表达式反解 `ALPHA`（法向齿形角）",
        source: "轴/齿轮生成器同一份表达式",
    },
    GearColumnSpec {
        tag: "齿顶高系数",
        label: "齿顶高系数 ha*",
        unit: "",
        formula: "外齿 ha* = (Da−d)/(2m) − x；内齿 ha* = (d−Da)/(2m) − x（d = mt·z）",
        source: "表达式 DA 反解 + gear.rs 的 d()/mt()",
    },
    GearColumnSpec {
        tag: "螺旋角",
        label: "螺旋角 β",
        unit: "°",
        formula: "九字段表达式反解 `BETA`（取绝对值；方向看下一行）",
        source: "轴/齿轮生成器同一份表达式",
    },
    GearColumnSpec {
        tag: "螺旋方向",
        label: "螺旋方向",
        unit: "",
        formula: "β>0 右旋、β<0 左旋、β=0 直齿（表达式 BETA 符号）",
        source: "轴/齿轮生成器同一份表达式",
    },
    GearColumnSpec {
        tag: "径向变位系数",
        label: "径向变位系数 x",
        unit: "",
        formula: "九字段表达式反解 `X`",
        source: "轴/齿轮生成器同一份表达式",
    },
    GearColumnSpec {
        tag: "全齿高",
        label: "全齿高 h",
        unit: "mm",
        formula: "h = |da − df| / 2（内/外齿同一式）",
        source: "gear.rs 的 da()/df()",
    },
    GearColumnSpec {
        tag: "精度等级",
        label: "精度等级",
        unit: "",
        formula: "手填（模板该行是样例 TEXT `887FHGB10095-88`，本卡提升为属性，缺省「—」）",
        source: "用户填写；本仓未收 GB/T 10095 数据",
    },
    GearColumnSpec {
        tag: "中心距及极限偏差",
        label: "中心距及极限偏差",
        unit: "mm",
        formula: "a = mt(z₁+z₂)/2（给配对齿数时；`center` 可显式覆盖）；极限偏差 ±fα 本仓未收",
        source: "gear.rs 的 mt()；GB/T 10095-88 未收",
    },
    GearColumnSpec {
        tag: "配对齿轮图号",
        label: "配对齿轮图号",
        unit: "",
        formula: "手填 `dwg`（缺省「—」）",
        source: "用户填写",
    },
    GearColumnSpec {
        tag: "配对齿轮齿数",
        label: "配对齿轮齿数",
        unit: "",
        formula: "手填 `mate`（中心距用；缺省「—」）",
        source: "用户填写",
    },
    GearColumnSpec {
        tag: "齿圈径向跳动公差",
        label: "齿圈径向跳动公差 Fr",
        unit: "",
        formula: "GB/T 10095-88 齿圈径向跳动公差 —— 本仓未收该标准数据",
        source: "缺（不臆造）",
    },
    GearColumnSpec {
        tag: "公法线长度公差",
        label: "公法线长度变动公差 FW",
        unit: "",
        formula: "GB/T 10095-88 公法线长度变动公差 —— 本仓未收该标准数据",
        source: "缺（不臆造）",
    },
    GearColumnSpec {
        tag: "齿形公差",
        label: "齿形公差 ff",
        unit: "",
        formula: "GB/T 10095-88 齿形公差 —— 本仓未收该标准数据",
        source: "缺（不臆造）",
    },
    GearColumnSpec {
        tag: "齿距极限偏差",
        label: "齿距极限偏差 fpt",
        unit: "",
        formula: "GB/T 10095-88 齿距极限偏差 —— 本仓未收该标准数据",
        source: "缺（不臆造）",
    },
    GearColumnSpec {
        tag: "齿向公差",
        label: "齿向公差 Fβ",
        unit: "",
        formula: "GB/T 10095-88 齿向公差 —— 本仓未收该标准数据",
        source: "缺（不臆造）",
    },
    GearColumnSpec {
        tag: "公法线",
        label: "公法线 W",
        unit: "mm",
        formula: "W = m·cosαt·[(k−0.5)π + z·invαt] + 2·x·m·sinαt（直齿外齿轮）",
        source: "gear.rs span_measurement()（与 GB/T 3478.6 式(11) 同一条渐开线跨距式）",
    },
    GearColumnSpec {
        tag: "公法线K",
        label: "公法线跨测齿数 k",
        unit: "",
        formula: "k = round(z·αt/180° + 0.5)（直齿外齿轮；30° 时即 z/6+0.5）",
        source: "gear.rs span_measurement()",
    },
];

/// 齿轮卡的选项/口径 JSON（随 `/api/spline_options` 下发；页面只渲染）。
pub fn options_json() -> serde_json::Value {
    serde_json::json!({
        "expression_example": "GEAR EX M3 Z20 ALPHA20 X0 DA66 DF52.5 BETA0 H30",
        "expression_note": "九字段统一齿形表达式（轴/齿轮生成器 GUI 可复制）；DA/DF 用于反解 ha*/c* 与全齿高",
        "columns": GEAR_COLUMNS.iter().map(|c| serde_json::json!({
            "tag": c.tag,
            "label": c.label,
            "unit": c.unit,
            "formula": c.formula,
            "source": c.source,
        })).collect::<Vec<_>>(),
        "missing_note": "GB/T 10095-88 的 Fr/FW/ff/fpt/Fβ 与中心距极限偏差本仓未收；\
                         内齿轮/斜齿轮的公法线口径本仓未收 —— 这些格子一律显示「—」，不臆造。",
        "note": "取值复用既有齿轮引擎（gear.rs）：表达式反解后 ha*/c* 由 DA/DF 反解，\
                 交回 GearParams 复核 da/df 闭环；公法线走 GearParams::span_measurement()。",
    })
}

/// CLI/HTTP 表单模型（字段与 GUI 控件一一对应）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct GearTableModel {
    /// 卡类型（GUI 回传；后端按 renderer 分派，这里只记不看）。
    #[serde(default)]
    pub card: String,
    /// 九字段统一齿形表达式。
    pub expr: String,
    /// 配对齿轮齿数（中心距用）。
    #[serde(default)]
    pub mate_z: Option<u32>,
    /// 配对齿轮图号。
    #[serde(default)]
    pub mate_dwg: Option<String>,
    /// 精度等级（手填字符串）。
    #[serde(default)]
    pub grade: Option<String>,
    /// 中心距显式覆盖。
    #[serde(default)]
    pub center: Option<f64>,
    /// 显式落点；缺省 = 待放置件。
    #[serde(default)]
    pub at: Option<[f64; 2]>,
    #[serde(default)]
    pub rot: f64,
}

/// 九字段表达式 → 齿形段（**复用 `shaft::parse_program`**；报错前缀换成本卡）。
pub fn parse_expr_gear(expr: &str) -> Result<crate::shaft::Gear, String> {
    if expr.trim().is_empty() {
        return Err(
            "齿轮参数表：缺九字段齿形表达式（形如 `GEAR EX M3 Z20 ALPHA20 X0 DA66 DF52.5 BETA0 H30`；\
             轴/齿轮生成器 GUI 可直接复制）"
                .to_string(),
        );
    }
    let program = crate::shaft::parse_program(expr)
        .map_err(|e| format!("齿轮参数表：齿形表达式无法解析：{e}"))?;
    if program.segments.len() != 1 {
        return Err(format!(
            "齿轮参数表：表达式应只有一段齿形（收到 {} 段）——请只粘生成器复制的那一行齿形表达式",
            program.segments.len()
        ));
    }
    program.segments[0].gear.ok_or_else(|| {
        "齿轮参数表：表达式里没有齿形段（应以 `GEAR`/`SPLINE` + M/Z/ALPHA… 开头）".to_string()
    })
}

impl GearTableModel {
    /// →（校验过的 `GearTableSpec`）。
    pub fn spec(&self) -> Result<GearTableSpec, String> {
        if let Some(z) = self.mate_z {
            if !(2..=1000).contains(&z) {
                return Err(format!("齿轮参数表：配对齿轮齿数 z₂={z} 超出范围（2–1000）"));
            }
        }
        if let Some(a) = self.center {
            if !(a.is_finite() && a > 0.0) {
                return Err(format!("齿轮参数表：中心距 center={a} 必须是正数"));
            }
        }
        let g = parse_expr_gear(&self.expr)?;
        let params = params_from_gear(&g)?;
        Ok(GearTableSpec {
            expr: self.expr.trim().to_string(),
            params,
            mate_z: self.mate_z,
            mate_dwg: self.mate_dwg.clone().filter(|s| !s.trim().is_empty()),
            grade: self.grade.clone().filter(|s| !s.trim().is_empty()),
            center: self.center,
            at: self.at,
            rot: self.rot,
        })
    }

    /// 预览 JSON（不碰图纸）。
    pub fn preview_json(&self) -> Result<serde_json::Value, String> {
        let spec = self.spec()?;
        let vals = values(&spec)?;
        let p = &spec.params;
        let mut items = Vec::with_capacity(GEAR_COLUMNS.len());
        let mut missing = Vec::new();
        for (c, (tag, value)) in GEAR_COLUMNS.iter().zip(vals) {
            debug_assert_eq!(c.tag, tag);
            if value == MISSING {
                missing.push(c.label.to_string());
            }
            items.push(serde_json::json!({
                "tag": tag,
                "label": c.label,
                "unit": c.unit,
                "value": value,
                "formula": c.formula,
                "source": c.source,
                "missing": value == MISSING,
            }));
        }
        let readout = serde_json::json!([
            {"k": "反解：模数 m", "v": fmt_mm(p.m)},
            {"k": "齿数 z", "v": p.z.to_string()},
            {"k": "齿形角 α", "v": fmt_deg(p.alpha_deg)},
            {"k": "变位 x", "v": fmt_num(p.x)},
            {"k": "螺旋角 β", "v": fmt_deg(p.beta_deg)},
            {"k": "表达式 Da / Df", "v": format!("{} / {}", fmt_mm(p.da()), fmt_mm(p.df()))},
        ]);
        Ok(serde_json::json!({
            "ok": true,
            "card": "齿轮参数表",
            "renderer": "gear_table",
            "title": format!("{}参数表（{}）", if p.kind.is_internal() { "内齿轮" } else { "外齿轮" }, spec.expr),
            "expr": spec.expr,
            "readout": readout,
            "items": items,
            "missing": missing,
            "missing_note": "本仓未收 GB/T 10095-88（Fr/FW/ff/fpt/Fβ）与中心距极限偏差；\
                             内齿轮/斜齿轮无公法线口径 —— 如实标缺，不臆造。",
        }))
    }

    /// 待放置件要带的 19 个 ATTRIB（tag → 值 + ATTDEF 模板）。
    pub fn pending_attrs(&self) -> Result<Vec<(AttributeDefinition, String)>, String> {
        let spec = self.spec()?;
        let vals = values(&spec)?;
        let mut out = Vec::with_capacity(vals.len());
        for ad in attdefs() {
            let v = vals
                .iter()
                .find(|(tag, _)| tag == &ad.tag)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            out.push((ad, v));
        }
        Ok(out)
    }

    /// 直接落点用的 `INSERT`（CLI 与 GUI 同一条路径）。
    pub fn build_insert(&self) -> Result<Insert, String> {
        let spec = self.spec()?;
        let at = self.at.unwrap_or([0.0, 0.0]);
        build_insert(&spec, at, self.rot)
    }

    /// 插入回执里的一段。
    pub fn echo_note(&self) -> Result<String, String> {
        let spec = self.spec()?;
        let p = &spec.params;
        let (k, w) = match p.span_measurement() {
            Some((k, w)) => (k.to_string(), fmt_mm(w)),
            None => (MISSING.to_string(), MISSING.to_string()),
        };
        Ok(format!(
            "{} z{} m{}（ha*={}，c*={}）公法线 k={} W={}",
            if p.kind.is_internal() { "内齿轮" } else { "外齿轮" },
            p.z,
            fmt_mm(p.m),
            fmt_num(p.ha),
            fmt_num(p.c),
            k,
            w
        ))
    }

    /// 待放置件的 `OCSM_PART` 元数据（薄台账）。
    pub fn part_meta_json(&self) -> Result<String, String> {
        let spec = self.spec()?;
        Ok(serde_json::json!({
            "family": "gear_table",
            "card": "齿轮参数表",
            "expr": spec.expr,
            "mate_z": spec.mate_z,
        })
        .to_string())
    }
}

impl GearTableSpec {
    /// 解析 `OCSMCARD 齿轮参数表 <九字段表达式> [mate z₂] [dwg 图号] [grade 精度等级] [center a] [at x,y] [rot 度]`。
    pub fn parse(text: &str) -> Result<Self, String> {
        let tokens: Vec<&str> = text.split_whitespace().collect();
        if tokens.is_empty() {
            return Err(usage());
        }
        const KEYS: &[&str] = &[
            "mate", "dwg", "grade", "center", "at", "rot", "配对齿数", "图号", "精度等级", "中心距",
            "旋转",
        ];
        let mut i = 0usize;
        while i < tokens.len() {
            let k = tokens[i].to_ascii_lowercase();
            if KEYS.contains(&k.as_str()) {
                break;
            }
            i += 1;
        }
        let expr = tokens[..i].join(" ");
        let mut model = GearTableModel {
            card: "齿轮参数表".into(),
            expr,
            mate_z: None,
            mate_dwg: None,
            grade: None,
            center: None,
            at: None,
            rot: 0.0,
        };
        let mut need = |i: &mut usize, what: &str| -> Result<String, String> {
            *i += 1;
            tokens
                .get(*i)
                .map(|s| s.to_string())
                .ok_or_else(|| format!("齿轮参数表：{what} 缺少数值/字符串"))
        };
        while i < tokens.len() {
            let key = tokens[i].to_ascii_lowercase();
            match key.as_str() {
                "mate" | "配对齿数" => {
                    let v = need(&mut i, "mate")?;
                    let z: u32 = v
                        .parse()
                        .map_err(|e| format!("齿轮参数表：mate={v} 不是整数：{e}"))?;
                    model.mate_z = Some(z);
                }
                "dwg" | "图号" => {
                    model.mate_dwg = Some(need(&mut i, "dwg")?);
                }
                "grade" | "精度等级" => {
                    model.grade = Some(need(&mut i, "grade")?);
                }
                "center" | "中心距" => {
                    let v = need(&mut i, "center")?;
                    let a: f64 = v
                        .parse()
                        .map_err(|e| format!("齿轮参数表：center={v} 不是数字：{e}"))?;
                    model.center = Some(a);
                }
                "at" => {
                    let v = need(&mut i, "at")?;
                    let (x, y) = v
                        .split_once(',')
                        .ok_or_else(|| format!("齿轮参数表：at「{v}」应为 `x,y`"))?;
                    let x: f64 = x
                        .trim()
                        .parse()
                        .map_err(|e| format!("齿轮参数表：at x={x} 不是数字：{e}"))?;
                    let y: f64 = y
                        .trim()
                        .parse()
                        .map_err(|e| format!("齿轮参数表：at y={y} 不是数字：{e}"))?;
                    model.at = Some([x, y]);
                }
                "rot" | "旋转" => {
                    let v = need(&mut i, "rot")?;
                    model.rot = v
                        .parse()
                        .map_err(|e| format!("齿轮参数表：rot={v} 不是数字：{e}"))?;
                }
                other => {
                    return Err(format!("齿轮参数表：不认识的参数「{other}」。\n{}", usage()))
                }
            }
            i += 1;
        }
        model.spec()
    }
}

/// 命令用法（`OCSMCARD` 报错指路）。
pub fn usage() -> String {
    "智能卡片「齿轮参数表」用法：`OCSMCARD 齿轮参数表 <九字段表达式> \
     [mate z₂] [dwg 图号] [grade 精度等级] [center a] [at x,y] [rot 度]`。\
     表达式形如 `GEAR EX M3 Z20 ALPHA20 X0 DA66 DF52.5 BETA0 H30`（轴/齿轮生成器 GUI 可直接复制）；\
     DA/DF 用来反解 ha*/c*，内/外齿由表达式 KIND 决定。\
     GB/T 10095-88 公差（Fr/FW/ff/fpt/Fβ）与中心距极限偏差本仓未收 —— 卡片显示「—」，不臆造。"
        .to_string()
}

// ══════════════════════════════════════════════════════════════════════════
// 测试
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// 表达式桩（m2 z40 α20 x0，模板齿轮画法档）。
    const EXPR: &str = "GEAR EX M2 Z40 ALPHA20 X0 DA84 DF75 BETA0 H20";

    fn model() -> GearTableModel {
        GearTableModel {
            card: "齿轮参数表".into(),
            expr: EXPR.into(),
            mate_z: None,
            mate_dwg: None,
            grade: None,
            center: None,
            at: None,
            rot: 0.0,
        }
    }

    /// 逐图元对模板：27 线（边框/分栏/行距）+ 46 TEXT + 19 ATTDEF；样式一律 OCSM_GB。
    #[test]
    fn gear_template_geometry_line_by_line() {
        let ents = block_entities();
        assert_eq!(ents.len(), 27 + 46 + 19, "图元数");
        let near5 = |a: f64, b: f64| (a - b).abs() < 1e-5;
        // 外框（模板：左右竖线在 x=-80/0；上下横线 y=0/-120）
        let has_line = |a: [f64; 2], b: [f64; 2]| {
            ents.iter().any(|e| matches!(e, EntityType::Line(l)
                if near5(l.start.x, a[0]) && near5(l.start.y, a[1])
                    && near5(l.end.x, b[0]) && near5(l.end.y, b[1])))
        };
        assert!(has_line([-80.0, 0.0], [0.0, 0.0]), "上框");
        assert!(has_line([0.0, 0.0], [0.0, -120.0]), "右框");
        assert!(has_line([-80.0, -120.0], [0.0, -120.0]), "下框");
        assert!(has_line([-80.0, 0.0], [-80.0, -120.0]), "左框");
        // 行距 6：横线 y = -6k（k=1..19），其中 k=19 是下框
        for k in 1..=19 {
            let y = -6.0 * k as f64;
            assert!(
                ents.iter().any(|e| matches!(e, EntityType::Line(l)
                    if near5(l.start.y, y) && near5(l.end.y, y))),
                "横线 y={y}"
            );
        }
        // 3 条竖分栏：-40（通高）、-20 的分段（0..-30、-36..-48、-54..-120）
        assert!(ents.iter().any(|e| matches!(e, EntityType::Line(l)
            if near5(l.start.x, -40.0) && near5(l.start.y, 0.0) && near5(l.end.y, -120.0))));
        assert!(ents.iter().any(|e| matches!(e, EntityType::Line(l)
            if near5(l.start.x, -20.0) && near5(l.start.y, 0.0) && near5(l.end.y, -30.0))));
        assert!(ents.iter().any(|e| matches!(e, EntityType::Line(l)
            if near5(l.start.x, -20.0) && near5(l.start.y, -36.0) && near5(l.end.y, -48.0))));
        assert!(ents.iter().any(|e| matches!(e, EntityType::Line(l)
            if near5(l.start.x, -20.0) && near5(l.start.y, -54.0) && near5(l.end.y, -120.0))));
        // 文字：全部 6文字层 + OCSM_GB；条数（46 TEXT + 19 ATTDEF 里的文字）
        let mut texts = 0usize;
        for e in &ents {
            match e {
                EntityType::Text(t) => {
                    assert_eq!(t.common.layer, "6文字层", "文字层：{}", t.value);
                    assert_eq!(t.style, "OCSM_GB", "文字样式：{}", t.value);
                    texts += 1;
                }
                EntityType::AttributeDefinition(ad) => {
                    assert_eq!(ad.common.layer, "6文字层", "属性层：{}", ad.tag);
                    assert_eq!(ad.text_style, "OCSM_GB", "属性样式：{}", ad.tag);
                }
                _ => {}
            }
        }
        assert_eq!(texts, 46);
        // 18 个模板 tag 全在（外加提升的 精度等级）
        let tags: Vec<String> = attdefs().iter().map(|a| a.tag.clone()).collect();
        assert_eq!(tags.len(), 19);
        let tmpl: Vec<String> = tags.iter().filter(|t| *t != GRADE_TAG).cloned().collect();
        assert_eq!(tmpl, TEMPLATE_TAGS, "模板 18 个 ATTDEF 逐项同序");
        assert_eq!(tags[8], GRADE_TAG, "精度等级插在 全齿高 之后（模板样例行位置）");
        // 样例 TEXT 位置提升为属性
        let grade = attdefs().iter().find(|a| a.tag == GRADE_TAG).cloned().unwrap();
        assert!(near(grade.insertion_point.x, GEAR_SAMPLE_GRADE.0));
        assert!(near(grade.insertion_point.y, GEAR_SAMPLE_GRADE.1));
    }

    /// 19 项口径表 ↔ ATTDEF 逐项同序（表驱动护栏）。
    #[test]
    fn gear_columns_cover_all_attdefs() {
        let atts = attdefs();
        assert_eq!(GEAR_COLUMNS.len(), atts.len());
        for (i, (c, ad)) in GEAR_COLUMNS.iter().zip(atts.iter()).enumerate() {
            assert_eq!(c.tag, ad.tag, "第 {} 项", i + 1);
            assert!(!c.label.is_empty() && !c.formula.is_empty() && !c.source.is_empty());
        }
    }

    /// 取值正确性：模板档 m2 z40 α20 → ha*=1、c*=0.25、全齿高 4.5、W 27.6896、k=5。
    #[test]
    fn gear_values_from_engine_and_missing() {
        let spec = model().spec().unwrap();
        let p = &spec.params;
        assert!(near(p.m, 2.0) && p.z == 40);
        assert!((p.ha - 1.0).abs() < 1e-9, "DA=84 反解 ha*=1：{}", p.ha);
        assert!((p.c - 0.25).abs() < 1e-9, "DF=75 反解 c*=0.25：{}", p.c);
        let vals = values(&spec).unwrap();
        assert_eq!(vals.len(), 19);
        let get = |tag: &str| vals.iter().find(|(t, _)| t == tag).unwrap().1.clone();
        assert_eq!(get("法向模数"), "2");
        assert_eq!(get("齿数"), "40");
        assert_eq!(get("齿形角"), "20°");
        assert_eq!(get("齿顶高系数"), "1");
        assert_eq!(get("螺旋角"), "0°");
        assert_eq!(get("螺旋方向"), "直齿");
        assert_eq!(get("径向变位系数"), "0");
        assert_eq!(get("全齿高"), "4.5");
        assert_eq!(get("公法线"), "27.69", "W 显示 3 位小数");
        assert_eq!(get("公法线K"), "5");
        // 未收数据 → 标缺
        for tag in [
            "齿圈径向跳动公差",
            "公法线长度公差",
            "齿形公差",
            "齿距极限偏差",
            "齿向公差",
            "中心距及极限偏差",
            GRADE_TAG,
        ] {
            assert_eq!(get(tag), MISSING, "{tag} 应标缺");
        }
        // 配对齿数 → 中心距 = mt(z1+z2)/2 = 2×60/2 = 60
        let mut m2 = model();
        m2.mate_z = Some(20);
        m2.mate_dwg = Some("OCS-002".into());
        m2.grade = Some("7-7-7 GB/T 10095-88".into());
        let spec2 = m2.spec().unwrap();
        let vals2 = values(&spec2).unwrap();
        let get2 = |tag: &str| vals2.iter().find(|(t, _)| t == tag).unwrap().1.clone();
        assert_eq!(get2("中心距及极限偏差"), "60");
        assert_eq!(get2("配对齿轮齿数"), "20");
        assert_eq!(get2("配对齿轮图号"), "OCS-002");
        assert_eq!(get2(GRADE_TAG), "7-7-7 GB/T 10095-88");
        // center 显式覆盖
        let mut m3 = model();
        m3.mate_z = Some(20);
        m3.center = Some(60.5);
        assert_eq!(
            values(&m3.spec().unwrap()).unwrap().iter().find(|(t, _)| t == "中心距及极限偏差").unwrap().1,
            "60.5"
        );
    }

    /// 内齿 / 斜齿 / 变位档：反解与缺项口径。
    #[test]
    fn gear_values_internal_helical_and_shift() {
        // 内齿轮 m2 z40 α20（表达式口径：DA=外侧齿根=d+2.25m=84.5、DF=里侧齿顶=d−2m=76）
        // → 全齿高 4.25、公法线标缺
        let mut mi = model();
        mi.expr = "GEAR IN M2 Z40 ALPHA20 X0 DA84.5 DF76 BETA0 H20".into();
        let si = mi.spec().unwrap();
        assert!(si.params.kind.is_internal());
        assert!((si.params.ha - 1.0).abs() < 1e-9);
        let vi = values(&si).unwrap();
        let get = |v: &Vec<(String, String)>, tag: &str| {
            v.iter().find(|(t, _)| t == tag).unwrap().1.clone()
        };
        assert_eq!(get(&vi, "全齿高"), "4.25");
        assert_eq!(get(&vi, "公法线"), MISSING, "内齿轮无公法线口径");
        assert_eq!(get(&vi, "公法线K"), MISSING);
        // 斜齿轮（九字段轴段解析器本期只做直齿，无法从表达式构造；卡片值映射直接测）：
        // 方向=右旋、公法线标缺
        let hp = GearParams {
            kind: GearKind::External,
            m: 2.0,
            z: 40,
            alpha_deg: 20.0,
            ha: 1.0,
            c: 0.25,
            beta_deg: 8.0,
            h: 20.0,
            x: 0.0,
            spline: None,
            std: GearStd::M,
            dp: None,
        };
        hp.validate().unwrap();
        let sh = GearTableSpec {
            expr: "GEAR EX M2 Z40 ALPHA20 X0 BETA8 H20".into(),
            params: hp,
            mate_z: None,
            mate_dwg: None,
            grade: None,
            center: None,
            at: None,
            rot: 0.0,
        };
        assert!(sh.params.is_helical());
        let vh = values(&sh).unwrap();
        assert_eq!(get(&vh, "螺旋方向"), "右旋");
        assert_eq!(get(&vh, "螺旋角"), "8°");
        assert_eq!(get(&vh, "公法线"), MISSING);
        // 变位 x=0.5（da = d+2m(1.5)=86、df = d−2m(1.25)=74）
        let mut mx = model();
        mx.expr = "GEAR EX M2 Z40 ALPHA20 X0.5 DA86 DF74 BETA0 H20".into();
        let sx = mx.spec().unwrap();
        assert!((sx.params.x - 0.5).abs() < 1e-9);
        assert!((sx.params.ha - 1.0).abs() < 1e-9);
        let vx = values(&sx).unwrap();
        assert_eq!(get(&vx, "径向变位系数"), "0.5");
        assert_eq!(get(&vx, "全齿高"), "6");
    }

    /// 表达式不自洽（DA 与 M/Z/X 对不上）→ 直白报错（不产半张表）。
    #[test]
    fn gear_expr_mismatch_and_parse_errors() {
        // da=90 推出 ha*=2.5、c*=−1.25 < 0 —— 反解结果被引擎 validate 拦下
        let mut bad = model();
        bad.expr = "GEAR EX M2 Z40 ALPHA20 X0 DA90 DF75 BETA0 H20".into();
        let e = bad.spec().unwrap_err();
        assert!(e.contains("不合法"), "{e}");
        // da=70 < d —— ha*=−2.5 < 0.5 同样被拦
        let mut bad2 = model();
        bad2.expr = "GEAR EX M2 Z40 ALPHA20 X0 DA70 DF63 BETA0 H20".into();
        assert!(bad2.spec().unwrap_err().contains("h"));
        // 缺 DA/DF
        let mut nod = model();
        nod.expr = "GEAR EX M2 Z40 ALPHA20 X0 BETA0 H20".into();
        assert!(nod.spec().unwrap_err().contains("DA"));
        // 空表达式
        let mut emp = model();
        emp.expr = "".into();
        assert!(emp.spec().unwrap_err().contains("表达式"));
        // CLI 解析：mate/dwg/grade/center/at/rot
        let s = GearTableSpec::parse(&format!(
            "{EXPR} mate 20 dwg OCS-002 grade 7-7-7 center 60.5 at 10,20 rot 30"
        ))
        .unwrap();
        assert_eq!(s.mate_z, Some(20));
        assert_eq!(s.mate_dwg.as_deref(), Some("OCS-002"));
        assert_eq!(s.grade.as_deref(), Some("7-7-7"));
        assert_eq!(s.center, Some(60.5));
        assert_eq!(s.at, Some([10.0, 20.0]));
        assert_eq!(s.rot, 30.0);
        // 不认识的参数
        let e = GearTableSpec::parse(&format!("{EXPR} bogus 1")).unwrap_err();
        assert!(e.contains("bogus"), "{e}");
    }

    /// 排版/几何断言：值文本不出表边界、不跨列（中列右边界 x≤−20；右列左边界 x≥−20），
    /// 标签不出标签列（x≤−40）。
    #[test]
    fn gear_text_boxes_stay_in_columns() {
        use crate::spline_table::text_extent;
        let spec = model().spec().unwrap();
        let vals = values(&spec).unwrap();
        // 值框：Fit/右锚 → [ax − w, y, ax, y+h]
        for ad in attdefs() {
            let v = vals.iter().find(|(t, _)| t == &ad.tag).unwrap().1.clone();
            let w = text_extent(&v, ad.height, ad.width_factor);
            let anchor_x = ad.alignment_point.x;
            let left = if matches!(ad.horizontal_alignment, HorizontalAlignment::Left) {
                ad.insertion_point.x
            } else {
                anchor_x - w
            };
            let right = if matches!(ad.horizontal_alignment, HorizontalAlignment::Left) {
                ad.insertion_point.x + w
            } else {
                anchor_x
            };
            assert!(
                left >= -40.0 - 1e-9 && right <= 1e-9,
                "{} 值「{v}」越界 [{left:.3}, {right:.3}]",
                ad.tag
            );
            // 值列分栏：模板值列最小左界 −20（跨列行 螺旋方向/精度等级 从 −40 起，单独放行）
            if !matches!(ad.tag.as_str(), "螺旋方向" | GRADE_TAG) {
                assert!(
                    left >= -20.0 - 1e-9,
                    "{} 值「{v}」左边界 {left:.3} 进标签/符号列",
                    ad.tag
                );
            }
        }
        // 标签/符号 TEXT：标签不进符号列（x≤−40），符号不进值列（x≤−20）
        for (v, x, y, h, wf, _layer, ha, _va, ax, _ay) in GEAR_TEXTS {
            let left = *x;
            let right = if *ha == 0 { *ax + text_extent(v, *h, *wf) } else { *ax };
            assert!(right <= 1e-9, "TEXT「{v}」出表右边界 {right:.3}");
            if right <= -40.0 + 1e-6 {
                continue; // 标签列
            }
            // 中列符号：不得越过 x=−20（个别手绘符号如 `h α *` 组合按模板照录，放行；
            // 表头 `公差(或极限偏差)值` 是 Fit 右锚，单独负断言）
            if !matches!(*v, "*" | "α" | "h" | "公差(或极限偏差)值") {
                assert!(right <= -20.0 + 1e-6, "符号「{v}」进值列：{right:.3}");
            }
            let _ = (left, y);
        }
        // 公差(或极限偏差)值 是 Fit 右锚（模板）：框 = [ax−w, ay, ax, ay+h]
        let tol_head = GEAR_TEXTS
            .iter()
            .find(|t| t.0 == "公差(或极限偏差)值")
            .unwrap();
        assert_eq!(tol_head.6, 5, "表头应为 Fit（右锚）");
        let w = text_extent(tol_head.0, tol_head.3, tol_head.4);
        assert!(tol_head.8 - w > -40.0, "表头左边界 {} 进标签列", tol_head.8 - w);
        assert!(tol_head.8 <= 1e-9, "表头右边界 {} 出表", tol_head.8);
        // 与中列表头 `检验项目代号` 不叠字（OCS 出图检查后把字宽从模板 0.819 降到 0.6）。
        assert_eq!(tol_head.4, 0.6, "表头字宽应已降到 0.6");
        let mid_head_right = -38.95808117938926 + text_extent("检验项目代号", 3.5, 0.667);
        assert!(
            tol_head.8 - w >= mid_head_right - 1e-6,
            "表头左边界 {:.3} 与中列表头右边界 {:.3} 叠字",
            tol_head.8 - w,
            mid_head_right
        );
    }

    /// CLI/GUI 同源：同一表达式两边出同一份 19 项。
    #[test]
    fn gear_cli_and_model_same_values() {
        let cli = GearTableSpec::parse(EXPR).unwrap();
        let gui = model().spec().unwrap();
        assert_eq!(values(&cli).unwrap(), values(&gui).unwrap());
        let md = markdown_table(&cli).unwrap();
        assert!(md.contains("| 公法线 | 27.69 |"));
        assert!(md.lines().count() >= 21);
    }
}
