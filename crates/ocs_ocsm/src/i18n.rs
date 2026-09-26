//! 双语机制（中 / 英）：**单一 message catalog** + 取词 API + 语言来源。
//!
//! 用户 2026-09-27 定案（分阶段：本阶段只立骨架 + 打通一条竖切，不做全量翻译）：
//! * 卡定义只有一份，**卡面标签随语言渲染**（禁止给每张卡再复制一份英文卡）；
//!   标准符号（`D_ei` / `Az` / `Wn` / `Kn` / `d_B` …）保持原样不译。
//! * 单一 catalog（表驱动，**一行一 key**：`zh` / `en`），按域分节
//!   （命令输出 / 报错 / GUI / 卡面标签 / 计算书 / 手册）。
//! * 语言 = **跟随宿主 + 可手动覆盖**：宿主 API（`ocs_plugin_api`）当前**没有**
//!   locale/language 字段（已核对 HostApi 全部方法），因此用环境变量兜底 + 程序内
//!   覆盖；一旦宿主暴露语言设置，只需改 [`resolve_env_lang`] 一处。
//!
//! # 语言来源（优先级从高到低）
//!
//! 1. 程序内 [`set_lang`]（GUI 设置 / 命令开关 / 测试；可 [`set_lang_auto`] 复位）；
//! 2. 环境变量 [`LANG_ENV`]（`OCSMLANG=zh|en`，用户/运维手动覆盖）；
//! 3. 别名 [`LANG_ENV_ALIAS`]（`OCSM_LANG`）；
//! 4. `LC_ALL` → `LC_MESSAGES` → `LANG`（宿主/system locale 的代理）；
//! 5. 默认 **`zh`**（保守默认：本仓既有输出全是中文，缺省不改变现行为）。
//!
//! 值接受 `zh` / `zh-CN` / `zh_CN.UTF-8` / `en` / `en_US.UTF-8`（大小写不敏感，
//! 只看 `_` `-` `.` `@` 前的第一段）。
//!
//! # 缺 key 的保守默认（**绝不返回静默空串**）
//!
//! * key 在 catalog 里、目标语言串为空 → **回落 `zh`**，并记入 [`missing_keys`]；
//! * key 完全不在 catalog 里 → 返回 **key 本身**（可见占位，便于一眼定位），
//!   并记入 [`missing_keys`]；
//! * 运行期不 panic（插件里让命令整个挂掉更糟）；缺失由 [`catalog_problems`]
//!   静态测试 + [`missing_keys`] 运行期点名兜住。
//!
//! 其它：`{name}` 占位符用 [`t_fmt`] 插值；catalog 完整性（两语齐全 / 无重复 key /
//! 占位符两语一致）由 [`catalog_problems`] 覆盖，见本文件底部单测。
//!
// 阶段 1 骨架：语言切换/诊断/缺 key 点名等 API 是给第二阶段接线与联调用的，
// 产品路径目前只用了 t/t_fmt；先允许 dead_code，不给构建加新警告。
#![allow(dead_code)]

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

/// 已支持的语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    /// 稳定语言代号（下发 GUI / 写日志用）。
    pub fn code(self) -> &'static str {
        match self {
            Lang::Zh => "zh",
            Lang::En => "en",
        }
    }

    /// 人类侧显示名（本身也走 catalog，随当前语言）。
    pub fn label(self) -> String {
        t(match self {
            Lang::Zh => "meta.lang.zh",
            Lang::En => "meta.lang.en",
        })
    }
}

/// 一条词条：**一行一 key：`zh` / `en`**。
#[derive(Debug, Clone, Copy)]
pub struct Msg {
    /// 稳定 key（`域.对象.用途`，如 `cmd.tf.usage`）。
    pub key: &'static str,
    /// 中文（缺省语言；本阶段字段名照「先立骨架」口径保留）。
    pub zh: &'static str,
    /// 英文。
    pub en: &'static str,
}

impl Msg {
    const fn new(key: &'static str, zh: &'static str, en: &'static str) -> Self {
        Self { key, zh, en }
    }
}

// ───────────────────────── catalog（按域分节） ─────────────────────────
//
// 本阶段只登记「已接线」的词条（竖切域）与语言元信息；其余全量字符串先盘点
// （`~/桌面/OCSM/review/i18n_盘点.md`），按域分批录入，**列在这里的必须真有人用**。

/// 单一 message catalog。加词条 = 按域在对应小节加一行。
pub const CATALOG: &[Msg] = &[
    // ── 语言元信息（GUI / 测试）──────────────────────────────────────
    Msg::new("meta.lang.zh", "中文", "Chinese"),
    Msg::new("meta.lang.en", "英文", "English"),

    // ── 命令输出（命令回执 / 用法 / 提示）────────────────────────────
    // 竖切域 ①：`TF` 的用法串（见 lib.rs::parse_frame_args）。
    Msg::new(
        "cmd.tf.usage",
        "用法：TF <名称> [比例] [at x,y] [rot 度]（不带参数 = 打开选择窗口）",
        "Usage: TF <name> [scale] [at x,y] [rot deg] (no args = open the picker window)",
    ),

    // ── 报错（解析错误 / 校验失败）──────────────────────────────────
    // 竖切域 ②：`TF` 参数认不出（带 `{arg}` / `{usage}` 插值，见 lib.rs）。
    Msg::new(
        "cmd.tf.err.unknown_param",
        "认不出的参数「{arg}」。{usage}",
        "Unrecognized argument \"{arg}\". {usage}",
    ),

    // ── GUI：语言切换控件（服务端注入每个页面；见 guide_server::render_page）──
    Msg::new("gui.lang.title", "语言", "Language"),
    // 语言名按「自身语言」显示（双语值相同；切换按钮两侧都要能认出自己）。
    Msg::new("gui.lang.zh", "中文", "中文"),
    Msg::new("gui.lang.en", "English", "English"),

    // ── GUI：公共错误（ocsm_gui_common.js；`{...}` 由页面 JS `.replace()` 填）──
    Msg::new(
        "gui.common.err.404",
        "端点 {path} 不存在（HTTP 404）：可能是插件未重启或版本不匹配 —— 请完全退出并重开 OCS",
        "Endpoint {path} not found (HTTP 404): the plugin may not have been restarted or the version does not match — fully quit and reopen OCS",
    ),
    Msg::new(
        "gui.common.err.http",
        "HTTP {status}：{detail}",
        "HTTP {status}: {detail}",
    ),
    Msg::new("gui.common.err.no_body", "无响应体", "empty response body"),
    Msg::new(
        "gui.common.err.not_json",
        "响应不是 JSON（HTTP {status}）：{text}",
        "Response is not JSON (HTTP {status}): {text}",
    ),
    Msg::new(
        "gui.common.err.net",
        "请求失败（网络层）：{err}",
        "Request failed (network): {err}",
    ),
    Msg::new("gui.common.err.request_failed", "请求失败", "Request failed"),

    // ── GUI：窗口/页面标题（窗口名 + <title>/<h1>）────────────────────
    Msg::new("gui.window.guide", "标注配置", "Annotation settings"),
    Msg::new("gui.window.parts", "零件库", "Parts library"),
    Msg::new("gui.window.gear", "齿轮", "Gear"),
    Msg::new("gui.window.shaft", "轴生成器", "Shaft generator"),
    Msg::new("gui.window.hole", "孔生成器", "Hole generator"),
    Msg::new("gui.window.spline", "花键参数表", "Spline tables"),
    Msg::new("gui.window.joint", "螺栓副装配", "Bolt joint assembly"),
    Msg::new("gui.window.rough", "表面粗糙度", "Surface roughness"),
    Msg::new("gui.window.manual", "命令手册", "Command manual"),
    Msg::new("gui.window.bom", "明细表编辑", "BOM editor"),
    Msg::new("gui.window.default", "标注配置", "Annotation settings"),

    // ── GUI：外部打开的提示页（opened_elsewhere_html）────────────────
    Msg::new(
        "gui.notice.opened_title",
        "「{label}」已经用独立窗口打开了",
        "\"{label}\" is already open in a separate window",
    ),
    Msg::new(
        "gui.notice.opened_body1",
        "图纸里的链接（Ctrl+点击）只能落在这种带地址栏的浏览器页签上，所以插件又开了一个 OCSM 独立窗口\n（去掉了地址栏/页签，与其它 OCSM 页面一致）—— 请到那个窗口里操作，",
        "Links in the drawing (Ctrl+click) can only open in a browser tab with an address bar, so the plugin opened a separate OCSM window\n(no address bar/tabs, consistent with the other OCSM pages) — use that window; ",
    ),
    Msg::new("gui.notice.opened_body2", "本页可以直接关掉", "you can close this page"),
    Msg::new("gui.notice.opened_body3", "。", "."),
    Msg::new(
        "gui.notice.bom_hint1",
        "明细表编辑器下次也可以直接运行命令 ",
        "Next time the BOM editor can also be opened by running the command ",
    ),
    Msg::new(
        "gui.notice.bom_hint2",
        " 打开，不走浏览器。",
        ", without a browser.",
    ),
    Msg::new("gui.notice.close_page", "关闭本页", "Close this page"),

    // ── GUI：标注配置（guide_gui.html）──────────────────────────────
    Msg::new("gui.guide.h1", "标注配置", "Annotation settings"),
    Msg::new("gui.guide.type.linear", "线性", "Linear"),
    Msg::new("gui.guide.type.diameter", "直径", "Diameter"),
    Msg::new("gui.guide.type.radius", "半径", "Radius"),
    Msg::new("gui.guide.type.datum", "基准", "Datum"),
    Msg::new("gui.guide.type.view", "向视图", "Auxiliary view"),
    Msg::new("gui.guide.type.angle", "角度", "Angle"),
    Msg::new("gui.guide.type.section", "剖切", "Section"),
    Msg::new("gui.guide.type.tolerance", "形位公差", "Geometric tolerance"),
    Msg::new("gui.guide.type.weld", "焊接", "Weld"),
    Msg::new("gui.guide.type.leader", "引线", "Leader"),
    Msg::new("gui.guide.type.balloon", "序号", "Item number"),
    Msg::new("gui.guide.type.arclen", "弧长", "Arc length"),
    Msg::new("gui.guide.type.detail", "局部放大", "Detail view"),
    Msg::new("gui.guide.apply", "应用", "Apply"),
    Msg::new("gui.guide.refresh", "应用并刷新", "Apply and refresh"),
    Msg::new("gui.guide.save_new", "另存为新标注", "Save as new annotation"),
    Msg::new("gui.guide.update", "更新标注", "Update annotation"),
    Msg::new("gui.guide.close", "关闭", "Close"),

    // ── GUI：标准件库（parts_gui.html）──────────────────────────────
    Msg::new("gui.parts.h1", "OCSM 标准件库", "OCSM Standard Parts"),
    Msg::new("gui.parts.tree", "零件树（点选零件）", "Part tree (click a part)"),
    Msg::new(
        "gui.parts.tree_note",
        "灰色 = 待实现（已列在树上，按需排期）",
        "Gray = not implemented yet (listed; scheduled on demand)",
    ),
    Msg::new("gui.parts.spec_view", "规格与视图", "Spec and view"),
    Msg::new("gui.parts.type", "型别", "Type"),
    Msg::new("gui.parts.dia", "公称直径", "Nominal diameter"),
    Msg::new(
        "gui.parts.len",
        "长度 l（mm，仅列该直径标准长度系列）",
        "Length l (mm; only standard lengths for this diameter)",
    ),
    Msg::new("gui.parts.spec_code", "规格代号", "Spec code"),
    Msg::new(
        "gui.parts.spec_custom_ph",
        "自定义规格，例 6x23x26x6",
        "Custom spec, e.g. 6x23x26x6",
    ),
    Msg::new("gui.parts.d", "轴径 d（mm，自由输入）", "Shaft diameter d (mm, free input)"),
    Msg::new(
        "gui.parts.b1",
        "槽宽 b1 覆盖（mm，留空 = 该 d 档默认）",
        "b1 override (mm; blank = default for this d)",
    ),
    Msg::new("gui.parts.view", "视图（出库哪个视图）", "View (which view to produce)"),
    Msg::new("gui.parts.export", "零件出库", "Produce part"),
    Msg::new("gui.parts.preview_alt", "零件预览", "Part preview"),
    Msg::new("gui.parts.tip", "← 在左侧展开并选择零件", "← Expand a part in the tree on the left"),
    Msg::new("gui.parts.tag_pending", "待实现", "pending"),
    Msg::new("gui.parts.base_default", "基点 = 头部支承面 × 轴线", "Base point = head bearing face × axis"),
    Msg::new("gui.parts.len_default", "长度 l", "Length l"),
    Msg::new(
        "gui.parts.len_suffix",
        "（mm，仅列该直径的取值）",
        " (mm; values for this diameter only)",
    ),
    Msg::new(
        "gui.parts.err.family",
        "该零件族数据异常：{err}",
        "Part family data error: {err}",
    ),
    Msg::new(
        "gui.parts.err.no_view",
        "该族暂无可出库视图（待画法确认）",
        "No producible view for this family yet (drawing method pending)",
    ),
    Msg::new("gui.parts.err.d_positive", "d 必须是正数", "d must be a positive number"),
    Msg::new("gui.parts.err.d_range", "d 超出标准表范围", "d is outside the standard table range"),
    Msg::new("gui.parts.err.not_number", "{label} 不是数字", "{label} is not a number"),
    Msg::new(
        "gui.parts.err.missing_param",
        "参数 {name} 必给（花键：L 满齿段长；退刀槽：P）",
        "Parameter {name} is required (spline: L = full-tooth length; relief groove: P)",
    ),
    Msg::new(
        "gui.parts.err.p_choice",
        "P 必须是表 2 单键：{choices}",
        "P must be a single key from table 2: {choices}",
    ),
    Msg::new(
        "gui.parts.err.spec_form",
        "规格代号要写成 N×d×D×B（例 6x23x26x6）",
        "Spec code must be N×d×D×B (e.g. 6x23x26x6)",
    ),
    Msg::new(
        "gui.parts.hint.p_ok",
        "P={p}：g1={g1}、g2={g2}、dg=d−{red}、r={r}；表 2 单键（P）：{choices}",
        "P={p}: g1={g1}, g2={g2}, dg=d−{red}, r={r}; single keys (P) from table 2: {choices}",
    ),
    Msg::new(
        "gui.parts.hint.d_band",
        "该 d 属于「{band}」档；默认 b1={def}；可选 b1={choices}",
        "d falls in band \"{band}\"; default b1={def}; choices b1={choices}",
    ),
    Msg::new(
        "gui.parts.hint.len_values",
        "该直径{label} 的取值：{choices} mm",
        "{label} for this diameter: {choices} mm",
    ),
    Msg::new(
        "gui.parts.hint.len_single",
        "该直径{label} = {len} mm",
        "{label} for this diameter = {len} mm",
    ),
    Msg::new("gui.parts.hint.in_table", "表内规格", "Spec from table"),
    Msg::new("gui.parts.hint.off_table", "表外规格（de 必给）", "Spec off table (de required)"),
    Msg::new(
        "gui.parts.hint.spec_tail",
        "；L 必给（侧视/剖视），de 留空 = 按规格查表",
        "; L required (side/section view); de blank = look up by spec",
    ),
    Msg::new("gui.parts.custom_spec", "自定义规格…", "Custom spec…"),
    Msg::new("gui.parts.req_yes", "必给", "required"),
    Msg::new("gui.parts.req_blank", "留空 = 默认", "blank = default"),
    Msg::new("gui.parts.status.exporting", "出库中…", "Producing…"),
    Msg::new("gui.parts.status.exported", "已出库", "Part produced"),
    Msg::new(
        "gui.parts.status.exported_tail",
        "（若本窗口未自动关闭，请切回图纸继续）",
        " (if this window did not close automatically, switch back to the drawing)",
    ),

    // ── GUI：孔生成器（hole_gui.html）───────────────────────────────
    Msg::new("gui.hole.h1", "OCSM 孔生成器", "OCSM Hole Generator"),
    Msg::new(
        "gui.hole.sub",
        "简单 / 螺纹 / 沉头 / 埋头 · 盲孔 / 贯通",
        "Simple / threaded / counterbore / countersink · blind / through",
    ),
    Msg::new("gui.hole.kind.simple", "简单孔", "Simple hole"),
    Msg::new("gui.hole.kind.threaded", "螺纹孔", "Threaded hole"),
    Msg::new("gui.hole.kind.counterbore", "沉头孔", "Counterbore"),
    Msg::new("gui.hole.kind.countersink", "埋头孔", "Countersink"),
    Msg::new(
        "gui.hole.hint",
        "选孔型 → 定孔径 → 点「确定」回到图纸放置（自动值不合适时可手改）。",
        "Pick a hole type → set the diameter → click OK to place it in the drawing (edit automatic values if needed).",
    ),
    Msg::new("gui.hole.ok", "确定", "OK"),
    Msg::new("gui.hole.cancel", "取消", "Cancel"),

    // ── GUI：智能卡片（spline_gui.html）─────────────────────────────
    Msg::new("gui.spline.h1", "OCSM 智能卡片", "OCSM Smart Cards"),
    Msg::new("gui.spline.report", "计算书", "Report"),
    Msg::new("gui.spline.export", "出表", "Create table"),
    Msg::new("gui.spline.cancel", "取消", "Cancel"),
    Msg::new("gui.spline.copy", "复制", "Copy"),
    Msg::new("gui.spline.summary", "摘要表落图纸", "Place summary table"),
    Msg::new("gui.spline.close", "关闭", "Close"),

    // ── GUI：螺栓副装配（joint_gui.html）────────────────────────────
    Msg::new("gui.joint.h1", "OCSM 螺栓副装配", "OCSM Bolt Joint Assembly"),
    Msg::new("gui.joint.bolt", "① 螺栓", "① Bolt"),
    Msg::new(
        "gui.joint.chain",
        "② 件链（从被连接件往外，依次穿过）",
        "② Part chain (from the clamped parts outward)",
    ),
    Msg::new(
        "gui.joint.presets",
        "③ 常用件链（点一下套用，之后仍可逐件改）",
        "③ Common chains (click to apply; still editable part by part)",
    ),
    Msg::new("gui.joint.place_title", "放置", "Placement"),
    Msg::new(
        "gui.joint.preview",
        "装配预览（真实几何，含遮挡裁剪）",
        "Assembly preview (real geometry, with occlusion clipping)",
    ),
    Msg::new("gui.joint.add", "+ 加一件", "+ Add part"),
    Msg::new("gui.joint.clear", "清空件链", "Clear chain"),
    Msg::new("gui.joint.place", "装配到图纸", "Place into drawing"),

    // ── GUI：表面粗糙度（rough_gui.html）────────────────────────────
    Msg::new("gui.rough.h1", "OCSM 表面粗糙度", "OCSM Surface Roughness"),
    Msg::new("gui.rough.base", "基础体（V 内差异）", "Base symbol (difference in V)"),
    Msg::new("gui.rough.extra", "附加标注", "Additional annotation"),
    Msg::new("gui.rough.preview", "预览", "Preview"),
    Msg::new(
        "gui.rough.attrs",
        "属性文字（ATTDEF，可缺省空白）",
        "Attribute text (ATTDEF; may be blank)",
    ),
    Msg::new("gui.rough.attr_a_up", "A′ 粗糙度上限", "A′ upper roughness limit"),
    Msg::new("gui.rough.attr_a", "A 粗糙度下限", "A lower roughness limit"),
    Msg::new("gui.rough.attr_e", "E 备注", "E Remarks"),
    Msg::new("gui.rough.p_symbol", "加工符号 P", "Machining symbol P"),
    Msg::new("gui.rough.rotate", "旋转", "Rotation"),
    Msg::new(
        "gui.rough.rot_hint",
        "符号绕插入点旋转（默认 0）。",
        "Rotate the symbol around the insertion point (default 0).",
    ),
    Msg::new(
        "gui.rough.c2_hint",
        "仅 C2（以不去除材料的方法获得）时强制空白。",
        "Forced blank for C2 (obtained without material removal) only.",
    ),
    Msg::new("gui.rough.blank", "空白", "blank"),
    Msg::new("gui.rough.apply", "应用并关闭", "Apply and close"),
    Msg::new("gui.rough.c1", "C1 通用", "C1 General"),
    Msg::new("gui.rough.c2", "C2 以不去除材料的方法获得", "C2 Obtained without material removal"),
    Msg::new("gui.rough.c3", "C3 去除材料", "C3 Material removal"),
    Msg::new("gui.rough.c4", "C4 焊后加工", "C4 Post-weld machining"),
    Msg::new("gui.rough.r1", "基础", "Basic"),
    Msg::new("gui.rough.r2", "周边相同处理", "Same treatment all around"),
    Msg::new("gui.rough.r3", "高级", "Advanced"),
    Msg::new("gui.rough.r4", "上限开关", "Upper limit switch"),
    Msg::new("gui.rough.r5", "上限开关+周边相同处理", "Upper limit + same treatment all around"),

    // ── GUI：命令手册（manual_gui.html）─────────────────────────────
    Msg::new("gui.manual.h1", "OCSM 命令手册", "OCSM Command Manual"),
    Msg::new("gui.manual.commands", "命令目录", "Command list"),
    Msg::new(
        "gui.manual.topics",
        "操作教程 / 制图知识",
        "Tutorials / drafting knowledge",
    ),
    Msg::new(
        "gui.manual.empty",
        "← 左侧点一个命令或主题；这里显示它的用法与步骤。",
        "← Click a command or topic on the left; its usage and steps appear here.",
    ),
    Msg::new(
        "gui.manual.note",
        "教程正文是磁盘上的 markdown（与 AI 用的 skill 手册同一批文件）。搜索目录：",
        "Tutorials are markdown files on disk (the same set used by the AI skill manual). Search directories:",
    ),
    Msg::new("gui.manual.loading", "读取中…", "Loading…"),
    Msg::new("gui.manual.read_fail", "读取失败", "Failed to load"),
    Msg::new("gui.manual.not_found", "未找到该主题的 md", "No markdown found for this topic"),
    Msg::new("gui.manual.source", "来源：{path}", "Source: {path}"),
    Msg::new(
        "gui.manual.count",
        "　共 {cmds} 条命令、{topics} 篇教程",
        " {cmds} commands, {topics} tutorials",
    ),
    Msg::new(
        "gui.manual.no_md1",
        "没找到手册 md。把手册放到 ",
        "No manual markdown found. Put the manual under ",
    ),
    Msg::new(
        "gui.manual.no_md2",
        "，或用环境变量 ",
        ", or point the environment variable ",
    ),
    Msg::new(
        "gui.manual.no_md3",
        " 指定目录后重开本窗口。",
        " at a directory and reopen this window.",
    ),
    Msg::new(
        "gui.manual.init_fail",
        "初始化失败：{err}",
        "Initialization failed: {err}",
    ),
    Msg::new(
        "gui.manual.request_fail",
        "请求失败：{err}",
        "Request failed: {err}",
    ),
    Msg::new(
        "gui.manual.search_dirs",
        "搜索目录：{dirs}",
        "Search directories: {dirs}",
    ),
    Msg::new("gui.manual.group.0", "入门与总览", "Getting started / overview"),
    Msg::new("gui.manual.group.1", "建图与插入", "Drawing setup and insertion"),
    Msg::new("gui.manual.group.2", "标注与符号", "Dimensions and symbols"),
    Msg::new("gui.manual.group.3", "表格与自动化", "Tables and automation"),
    Msg::new("gui.manual.group.4", "机械制图知识", "Mechanical drafting knowledge"),

    // ── GUI：明细表编辑（bom_gui.html）──────────────────────────────
    Msg::new("gui.bom.h1", "明细表编辑", "BOM Editor"),
    Msg::new("gui.bom.reload", "刷新", "Reload"),
    Msg::new("gui.bom.export_x", "导出 xlsx", "Export xlsx"),
    Msg::new("gui.bom.export_c", "导出 csv", "Export csv"),
    Msg::new("gui.bom.import", "导入 xlsx/csv…", "Import xlsx/csv…"),
    Msg::new("gui.bom.add", "＋加行", "+ Add row"),
    Msg::new("gui.bom.apply", "应用到图纸", "Apply to drawing"),

    // ── GUI：齿轮 / 轴生成器（页头与主按钮）─────────────────────────
    Msg::new(
        "gui.gear.title",
        "OCSM 齿轮（外齿轮 / 内齿轮）",
        "OCSM Gear (External / Internal)",
    ),
    Msg::new("gui.gear.h1", "OCSM 齿轮 / 花键", "OCSM Gear / Spline"),
    Msg::new(
        "gui.gear.sub",
        "外齿轮 / 内齿轮（齿圈）",
        "External gear / internal gear (ring)",
    ),
    Msg::new("gui.gear.copy_expr", "复制表达式", "Copy expression"),
    Msg::new("gui.gear.export", "生成到图纸", "Generate into drawing"),
    Msg::new("gui.shaft.title", "OCSM 轴生成器", "OCSM Shaft Generator"),
    Msg::new("gui.shaft.h1", "OCSM 轴生成器", "OCSM Shaft Generator"),
    Msg::new(
        "gui.shaft.sub",
        "段表 ↔ 行文本双向同步 + 实时预览",
        "Segment table ↔ DSL two-way sync + live preview",
    ),
    Msg::new("gui.shaft.export", "生成到图纸", "Generate into drawing"),
    Msg::new("gui.shaft.add_row", "+ 加行", "+ Add row"),
    Msg::new("gui.shaft.del_row", "删行", "Delete row"),
    Msg::new("gui.shaft.copy_row", "复制行", "Copy row"),
    Msg::new("gui.shaft.up_row", "上移", "Move up"),
    Msg::new("gui.shaft.down_row", "下移", "Move down"),
    Msg::new("gui.shaft.parse_add", "解析并加段", "Parse and add segment"),
    Msg::new("gui.shaft.open_gear", "打开齿轮生成器", "Open gear generator"),

    // ── 命令目录（手册窗口左侧总表；guide_server::COMMAND_CATALOG 一句话摘要）──
    // 命令关键字/示例/图层名/标准符号照原样保留（不译）；译的是说明性句子。
    Msg::new(
        "cmd.catalog.ocsm",
        "初始化：建图层/线型/文字样式/标注样式（并打开零件库窗口）",
        "Initialize: create layers/linetypes/text styles/dimension styles (also opens the parts library window)",
    ),
    Msg::new(
        "cmd.catalog.digits",
        "切当前图层；有选中对象时把对象移到该层",
        "Switch the current layer; if objects are selected, move them to that layer",
    ),
    Msg::new(
        "cmd.catalog.frameinit",
        "图框：不带参数=打开图框选择窗口；带参数=一行直插（`TF a3_landscape 1:2 at 0,0 [rot 度]`）",
        "Frame: no args = open the frame picker; with args = insert in one line (`TF a3_landscape 1:2 at 0,0 [rot deg]`)",
    ),
    Msg::new(
        "cmd.catalog.frameinsert",
        "按所选图框 + 比例插入（光标跟随，比例感知标注样式）；也可带参数直插（同 TF）",
        "Insert the chosen frame at the chosen scale (cursor-follow, scale-aware dimension styles); args work like TF",
    ),
    Msg::new(
        "cmd.catalog.part",
        "标准件/结构要素插入：不带参数=开零件库窗口（左「标准件」树 + 右「结构要素」树）+放置态；带参数=一行直插（标准件 `XL 族 d l [view …] [at x,y] [rot 度]`；结构要素 `XL detail_grind_od d [b1 值] [at x,y] [rot 度]`；外螺纹退刀槽 `XL detail_thread_relief d P 螺距 [g1 值 g2 值 dg 值 r 值 alpha 值] [at x,y] [rot 度]`；毂槽 `XL detail_hub_keyway d [len 毂长] [view main|side]`（b/t₂/r 由 d 查表，len 缺省 30）；平键 `XL key_1096_{a|b|c} b L`、`XL key_1097_{a|b} b L`（d 槽位=键宽 b，L 省略/0=该档默认，L 须 ∈ 标准系列且 L<10b；1097 的 L1/L2/L3 由 L 查 GB/T 1097 长度系列表派生，表外 L 报错））",
        "Standard parts / structural details: no args = open the parts library window (left \"Standard parts\" tree + right \"Structural details\" tree) + placement mode; with args = insert in one line (standard part `XL family d l [view …] [at x,y] [rot deg]`; structural detail `XL detail_grind_od d [b1 value] [at x,y] [rot deg]`; external thread relief `XL detail_thread_relief d P pitch [g1 v g2 v dg v r v alpha v] [at x,y] [rot deg]`; hub keyway `XL detail_hub_keyway d [len hub length] [view main|side]` (b/t₂/r looked up from d; len defaults to 30); parallel key `XL key_1096_{a|b|c} b L`, `XL key_1097_{a|b} b L` (the `d` argument position = key width b; L omitted/0 = default for that range; L must be in the standard series and L<10b; for 1097, L1/L2/L3 are derived by looking up the GB/T 1097 length-series table; L outside the table is an error))",
    ),
    Msg::new(
        "cmd.catalog.card",
        "智能卡片（工具栏「卡片」组按钮；旧短命令 `XLT` 已移除）：不带参数=开图形界面（七张卡：GB 花键 / 齿轮 / ANSI 花键中英 / NF 内 / NF 外 / DIN 5480；卡类型表驱动，可 `?card=<id>` 深链；均支持九字段齿形表达式反解）；带参数=`OCSMCARD <卡类型> …`，如 `OCSMCARD 花键参数表 [std GB] 内 6H <表达式> [dp 4.5] [root 平|圆] [at x,y]`、`OCSMCARD NF外花键参数表 SPLINE EX M7.5 Z38 ALPHA20 X0.8 BETA0 H30`；表格块几何内建（模板逐图元），卡类型/体系/公式口径表由后端 `CARD_TYPES`+选项表下发；以后加表格/铭牌 = `CARD_TYPES` 加数据行 + 一个渲染器",
        "Smart cards (\"Cards\" toolbar group; old short command `XLT` removed): no args = open the GUI (seven cards: GB spline / gear / ANSI spline CN+EN / NF internal / NF external / DIN 5480; card types are table-driven, with `?card=<id>` deep links; all support nine-field tooth-profile expression back-solving); with args = `OCSMCARD <card type> …`, e.g. `OCSMCARD 花键参数表 [std GB] 内 6H <expression> [dp 4.5] [root 平|圆] [at x,y]`, `OCSMCARD NF外花键参数表 SPLINE EX M7.5 Z38 ALPHA20 X0.8 BETA0 H30`; table-block geometry is built in (template entity by entity); card types/systems/formula tables come from the backend `CARD_TYPES` + option tables; adding future tables/nameplates = add a data row to `CARD_TYPES` + write a renderer",
    ),
    Msg::new(
        "cmd.catalog.joint",
        "螺栓副装配：不带参数=开装配窗口+放置态；带参数=一行直装（件链算长度、遮挡裁剪、一次撤销）",
        "Bolt joint assembly: no args = open the assembly window + placement mode; with args = assemble in one line (chain length computed, occlusion clipping, single undo)",
    ),
    Msg::new(
        "cmd.catalog.powerdim",
        "智能标注：拾取点模式标线性/对齐/半径/直径（Enter 切线段点选）",
        "Smart dimension: point-pick mode for linear/aligned/radius/diameter (Enter switches to segment picking)",
    ),
    Msg::new(
        "cmd.catalog.dimgulide",
        "引导线标注：选引导线 → 配置窗口（尺寸/剖视/向视/局部放大/角度/弧长/焊接/引线/序号/公差/粗糙度/形位公差）",
        "Leader annotation: pick a leader → configuration window (dimension/section/auxiliary view/detail/angle/arc length/weld/leader/item number/tolerance/roughness/geometric tolerance)",
    ),
    Msg::new(
        "cmd.catalog.centerline",
        "中心线：点圆/圆弧 → 十字中心线；点两根直线 → 角平分线中心线（`3中心线层`，线长 = 直径/投影长 + 图框比例×6mm）",
        "Centerlines: click a circle/arc → cross centerlines; click two lines → angle-bisector centerline (`3中心线层`, length = diameter/projected length + frame scale×6mm)",
    ),
    Msg::new(
        "cmd.catalog.gear",
        "齿轮（外齿轮 / 内齿轮（齿圈））+ 渐开线花键（花键模式）：不带参数=开齿轮/花键窗口（模式复选框 + 参数 + 视图按钮 + 实时预览）；带参数=一行直插（`OCSMGEAR 2 40 20 view 剖视图`、`OCSMGEAR int 2 40 30 view 端视图`；花键：`OCSMGEAR 花键 [内花键] std=DIN [profile=DIN30] db=40 2 18 h=30 view 端视图`，预设代号 GB30P/GB30R/GB375R/GB45R/DIN30 可直接代 std+profile）。齿轮模式只认 模数/齿数/压力角/变位系数 等常规项，给标准号或 d_B 明确报错。花键模式：GB 无基准直径（给 d_B 报错）；DIN 的 d_B 是主参数（d_B+m/d_B+z/m+z 三种给法，表外按公式推并标注来源），内/外花键用同一「齿轮种类」开关。内花键与内齿轮同口径（用户定案「内花键剖视图和内齿轮一样，不存在侧视图」）：只有 剖视图 + 端视图，无侧视图；剖视图齿圈内齿不剖（端面/齿顶线/齿根线/内孔壁/孔口倒角 + 分度线/轴线，不打剖面线），齿圈外壁留用户延伸。计算书：命令加 `REPORT`（如 `OCSMGEAR 花键 std=DIN db=40 2 18 h=30 REPORT`）—— 纯计算不插图，输出含公式/代入数值/结果/依据来源的 Markdown 计算书，`REPORT=路径` 另写文件",
        "Gear (external / internal gear (ring)) + involute spline (spline mode): no args = open the gear/spline window (mode checkbox + parameters + view buttons + live preview); with args = insert in one line (`OCSMGEAR 2 40 20 view 剖视图`, `OCSMGEAR int 2 40 30 view 端视图`; spline: `OCSMGEAR 花键 [内花键] std=DIN [profile=DIN30] db=40 2 18 h=30 view 端视图`; presets GB30P/GB30R/GB375R/GB45R/DIN30 can stand in for std+profile). Gear mode accepts only the usual items (module/teeth/pressure angle/profile shift, etc.); a standard number or d_B is a clear error. Spline mode: GB has no reference diameter (giving d_B is an error); for DIN, d_B is the primary parameter (three ways to give it: d_B+m / d_B+z / m+z; outside the table it is derived by formula with the source noted); internal/external splines use the same \"gear kind\" switch. Internal splines follow the same convention as internal gears (user ruling: \"an internal spline section view is the same as an internal gear; there is no side view\"): only section + end views, no side view; in the section view the internal teeth are not sectioned (face/tip/root lines + bore wall + bore chamfer + pitch line/axis, no hatching), and the outer wall of the ring is left for the user to extend. Report: append `REPORT` (e.g. `OCSMGEAR 花键 std=DIN db=40 2 18 h=30 REPORT`) — computation only, no geometry; prints a Markdown report with formulas/substituted values/results/sources; `REPORT=path` also writes a file",
    ),
    Msg::new(
        "cmd.catalog.shaft",
        "轴生成器：不带参数=开轴生成器窗口（段表 ↔ 行文本双向同步 + 实时预览 + 视图按钮）+ 放置态；带参数=行 DSL/JSON 一行直插（段拼接 + 端面倒角 + 砂轮越程槽 + 螺纹段 M + 齿轮段 GEAR + 矩形花键段 SPLINE + 轴槽 KEY（GB/T 1095 平键键槽，本期只做轴槽） + 视图 VIEW 常规|剖视（双视图已移除）；退刀槽就是一小段小直径轴段）。`OCSMSHAFT S30 E30 L45 CH2@L | S40 E40 L7 M1.5 | S36 E36 L5 | GEAR M3 Z20 VIEW 剖视 at x,y rot 度`；齿轮段可 `GEAR M3 Z20 ALPHA25`（压力角默认 20°）；花键 `OCSMSHAFT SPLINE 6x23x26x6 L30`（可 `de 71` 覆盖，矩形花键）；轴槽 `OCSMSHAFT S25 E25 L40 CH2@L KEY A 18 | S30 E30 L30`（**轴段类型**，进段表 KEY 列：键型 A/B/C + 键长 L（所选键型的键长）+ 位置中置/端置；**b×h 由该段直径 d 查 GB/T 1095 d 列自动定**，显式 `b8h7` 只作校验、必须落在该轴径档标准配对；t1 按 b 查 GB/T 1095 表，可 `t1 5` 覆盖；B/C 的键长自动按槽端圆弧折算（圆弧半径 b/2 吃直段：中置 B +b / C +b/2；端置 B +b/2 / C 不折算），实际槽长为折算长度（端置再 +t1）；显示口径：中置恒显示 A、端置 B/C 显示 C；可选 `双槽`（`DOUBLE`）＝绕轴心 180° 对置、仅剖视图体现、约 1.5 倍单键转矩；与 GEAR/SPLINE/M/OV/RL 互斥；导向平键 `OCSMSHAFT S30 E30 L50 KEY A 25 导向`（GB/T 1097：只 A/B、L 取 1097 系列 25…450∩L<10b、槽长 = 键长、槽上自动 2 个固定螺钉螺纹孔 d0×L0（孔心距槽端 L3），与双槽互斥；起键孔不属 1097），见 handbook 03）。渐开线花键只在 OCSMGEAR 花键模式生成（轴段 INVOLSPLINE 已撤）；GB/T 3478 基本齿廓不含变位（花键模式下 GB 的 x 恒为 0 且锁死；CLI/查询串给非零 x 明确报错，DIN/NF 的 x 仍按 d_B/A 派生）。计算书：命令加 `REPORT`（如 `OCSMSHAFT SPLINE 6x23x26x6 L30 REPORT`）—— 纯计算不插图，输出段清单 Markdown 计算书，`REPORT=路径` 另写文件",
        "Shaft generator: no args = open the shaft generator window (segment table ↔ DSL two-way sync + live preview + view buttons) + placement mode; with args = one-line DSL/JSON insert (segment concatenation + end chamfers + grinding relief grooves + thread segments M + gear segments GEAR + rectangular spline segments SPLINE + shaft keyways KEY (GB/T 1095 parallel-key keyways; only shaft keyways for now) + view VIEW normal|section (dual view removed); a relief groove is just a short small-diameter shaft segment). `OCSMSHAFT S30 E30 L45 CH2@L | S40 E40 L7 M1.5 | S36 E36 L5 | GEAR M3 Z20 VIEW 剖视 at x,y rot deg`; gear segments accept `GEAR M3 Z20 ALPHA25` (pressure angle defaults to 20°); spline `OCSMSHAFT SPLINE 6x23x26x6 L30` (`de 71` overrides; rectangular spline); keyway `OCSMSHAFT S25 E25 L40 CH2@L KEY A 18 | S30 E30 L30` (**shaft segment type**, fills the segment table KEY column: key type A/B/C + key length L (for the chosen key type) + position centered/end; **b×h is determined automatically from GB/T 1095 by the segment diameter d**; an explicit `b8h7` is only validated and must match the standard pairing for that diameter range; t1 is looked up by b in GB/T 1095 and `t1 5` overrides; for B/C the key length is converted by the slot-end radius (radius b/2 eats into the straight part: centered B +b / C +b/2; end-position B +b/2 / C no conversion) and the actual slot length is the converted length (end-position adds t1); display convention: centered always shows A, end-position B/C shows C; optional `双槽` (`DOUBLE`) = two slots 180° apart, shown in section view only, about 1.5× single-key torque; mutually exclusive with GEAR/SPLINE/M/OV/RL; guide key `OCSMSHAFT S30 E30 L50 KEY A 25 导向` (GB/T 1097: only A/B, L from the 1097 series 25…450 ∩ L<10b, slot length = key length, two fixing-screw threaded holes d0×L0 added above the slot (hole centers at L3 from the slot end), mutually exclusive with double slot; the key-removal hole is not part of 1097), see handbook 03). Involute splines are generated only in OCSMGEAR spline mode (the INVOLSPLINE shaft segment was removed); the GB/T 3478 basic profile has no profile shift (in spline mode GB x is always 0 and locked; a non-zero x from the CLI/query string is a clear error, while DIN/NF x is still derived from d_B/A). Report: append `REPORT` (e.g. `OCSMSHAFT SPLINE 6x23x26x6 L30 REPORT`) — computation only, no geometry; prints a segment-list Markdown report; `REPORT=path` also writes a file",
    ),
    Msg::new(
        "cmd.catalog.hole",
        "孔生成器：不带参数=开孔生成器窗口（简单孔/螺纹孔/沉头孔/埋头孔；盲孔/贯通；沉头/埋头可选带螺纹；不带螺纹时子类型=钻头大小/自定义/螺栓间隙）+ 放置态；带参数=一行直插（`OCSMHOLE 螺纹孔 M10 H18 L15 at x,y rot 度`、`OCSMHOLE 沉头孔 无螺纹 M10 间隙 中等装配 H22`）；规格可直接写 GUI 下拉名（`G G1/8`、`UNC 1/4-20`、`NPT NPT1/2`、`Tr Tr8×1.5`、`ACME 1/4-16`，数值 `公称6.35 P1.058` 照旧）。自动螺纹长 = 1.5d；自动孔深 = 有效深+2P（仅带螺纹且盲孔时可选）；贯通无 118° 锥。数据：ISO 724 / GB/T 152.3 / GB/T 152.2 / GB/T 5277 / 底孔牙深明细表（见表 JSON 的 source）",
        "Hole generator: no args = open the hole generator window (simple/threaded/counterbore/countersink; blind/through; counterbore/countersink may be threaded; when unthreaded the subtype = drill size/custom/bolt clearance) + placement mode; with args = insert in one line (`OCSMHOLE 螺纹孔 M10 H18 L15 at x,y rot deg`, `OCSMHOLE 沉头孔 无螺纹 M10 间隙 中等装配 H22`); the size can be given directly as the GUI dropdown name (`G G1/8`, `UNC 1/4-20`, `NPT NPT1/2`, `Tr Tr8×1.5`, `ACME 1/4-16`; the numeric form `公称6.35 P1.058` still works). Automatic thread length = 1.5d; automatic hole depth = effective depth + 2P (available only for threaded blind holes); through holes have no 118° cone. Data: ISO 724 / GB/T 152.3 / GB/T 152.2 / GB/T 5277 / tap-drill tooth-depth table (see the source field in the table JSON)",
    ),
    Msg::new(
        "cmd.catalog.edit",
        "改标注：选中 OCSM 生成的标注 → 配置窗口改参数 → 重生成",
        "Edit annotation: select an OCSM-generated annotation → change parameters in the config window → regenerate",
    ),
    Msg::new(
        "cmd.catalog.rgh",
        "表面粗糙度：点选插入点 → 配置窗口（匿名块 + ATTDEF）",
        "Surface roughness: pick an insertion point → config window (anonymous block + ATTDEF)",
    ),
    Msg::new(
        "cmd.catalog.dim2gb",
        "一键转国标：原生标注 → OCSM_GB 样式 + 匿名块；智能圆心标记（CENTERMARK）一并换成 `3中心线层` 中心线（Ø + 图框比例×6）",
        "One-click GB conversion: native dimensions → OCSM_GB style + anonymous blocks; smart center marks (CENTERMARK) are also converted to `3中心线层` centerlines (Ø + frame scale×6)",
    ),
    Msg::new(
        "cmd.catalog.bom",
        "明细表：建表/刷新（BOM 30 = 本次首列 30 行；现有行的手改与数量锁保留）",
        "BOM: build/refresh (BOM 30 = 30 rows in the first column this time; manual edits to existing rows and quantity locks are kept)",
    ),
    Msg::new(
        "cmd.catalog.bomsync",
        "明细表：按序号球标重排/重建（球标联动入口）",
        "BOM: reorder/rebuild from item balloons (balloon-sync entry point)",
    ),
    Msg::new(
        "cmd.catalog.bomedit",
        "明细表：打开网页编辑器（＝Ctrl+点击图纸里的表块；改完点「应用到图纸」）",
        "BOM: open the web editor (= Ctrl+click the table block in the drawing; click \"Apply to drawing\" when done)",
    ),
    Msg::new(
        "cmd.catalog.bomlock",
        "明细表：锁定某行数量（BOMLOCK 5 3 / BOMLOCK 5 off）",
        "BOM: lock a row's quantity (BOMLOCK 5 3 / BOMLOCK 5 off)",
    ),
    Msg::new(
        "cmd.catalog.bomxlsx",
        "明细表：导出到 .xlsx（带「锁定数量」列，可外部编辑）",
        "BOM: export to .xlsx (with a \"lock quantity\" column, editable outside)",
    ),
    Msg::new(
        "cmd.catalog.bomxlsxi",
        "明细表：从 .xlsx/.csv 导入（手改数量自动上锁）",
        "BOM: import from .xlsx/.csv (manually changed quantities are locked automatically)",
    ),
    Msg::new(
        "cmd.catalog.bomcfg",
        "明细表：只改「每列行数」（表头/列宽/格式在 bom/ 模板块里，本命令改不了）",
        "BOM: change only \"rows per column\" (headers/column widths/format live in the bom/ template blocks; this command cannot change them)",
    ),
    Msg::new(
        "cmd.catalog.mcp",
        "打印 MCP/HTTP 接入信息（给外部 AI/脚本）",
        "Print MCP/HTTP access info (for external AI/scripts)",
    ),
    Msg::new(
        "cmd.catalog.help",
        "打开本手册窗口（命令目录 + 操作教程）",
        "Open this manual window (command list + tutorials)",
    ),

    // ── 命令输出/报错：孔生成器（OCSMHOLE / DK；hole.rs）+ 螺纹表（thread.rs）──
    // 报错里保留可核对的原始信息（值/表名/规格名）；命令关键字与表内 `name` 不译。
    Msg::new(
        "cmd.hole.err.thread_pair_missing",
        "螺纹 M{d}×{p} 不在 ISO 724 表里（粗牙/细牙都没有这个组合）",
        "Thread M{d}×{p} is not in the ISO 724 table (neither coarse nor fine has this combination)",
    ),
    Msg::new(
        "cmd.hole.err.thread_coarse_missing",
        "M{d} 不在 ISO 724 粗牙表里（粗牙范围 M{d_lo}…M{d_hi}）",
        "M{d} is not in the ISO 724 coarse-thread table (coarse range M{d_lo}…M{d_hi})",
    ),
    Msg::new(
        "cmd.hole.err.counterbore_gb70_2",
        "GB/T 152.3-1988 没有 GB/T 70.2（内六角平圆头）的沉孔表（官方只有 表1 适用 GB 70、表2 适用 GB 6190、GB 6191 及 GB 65），按不插值规则 70.2 标缺 —— 请选 70.1 或用自定义",
        "GB/T 152.3-1988 has no counterbore table for GB/T 70.2 (hex-socket button head): officially only Table 1 for GB 70 and Table 2 for GB 6190, GB 6191 and GB 65; per the no-interpolation rule 70.2 is marked missing — choose 70.1 or use a custom value",
    ),
    Msg::new(
        "cmd.hole.err.reco_unknown",
        "不认识的沉孔推荐值「{other}」（gb70_1/gb70_2/gb6190）",
        "Unrecognized counterbore recommendation \"{other}\" (gb70_1/gb70_2/gb6190)",
    ),
    Msg::new(
        "cmd.hole.err.reco_unknown_cli",
        "不认识的沉孔推荐值「{other}」（70.1/70.2/6190）",
        "Unrecognized counterbore recommendation \"{other}\" (70.1/70.2/6190)",
    ),
    Msg::new(
        "cmd.hole.err.counterbore_row_missing",
        "GB/T 152.3-1988 {table} 表里没有 M{d}（可用 {list}）",
        "The GB/T 152.3-1988 {table} table has no M{d} (available {list})",
    ),
    Msg::new(
        "cmd.hole.err.countersink_row_missing",
        "GB/T 152.2-2014（沉头螺钉用沉孔）表里没有 M{d}（可用 M{d_lo}…M{d_hi}；标准只到 M10）",
        "The GB/T 152.2-2014 (countersink for flat-head screws) table has no M{d} (available M{d_lo}…M{d_hi}; the standard only goes to M10)",
    ),
    Msg::new(
        "cmd.hole.err.clearance_row_missing",
        "GB/T 5277 通孔表里没有 M{d}（可用 M{d_lo}…M{d_hi}）",
        "The GB/T 5277 clearance-hole table has no M{d} (available M{d_lo}…M{d_hi})",
    ),
    Msg::new(
        "cmd.hole.err.tap_row_missing",
        "底孔径表（螺纹孔底孔）里没有 M{d}{p}",
        "The tap-drill table (threaded-hole pilot) has no M{d}{p}",
    ),
    Msg::new(
        "cmd.hole.err.clearance_fit_unknown",
        "不认识的螺栓间隙配合「{other}」（close/normal/loose）",
        "Unrecognized bolt clearance fit \"{other}\" (close/normal/loose)",
    ),
    Msg::new(
        "cmd.hole.err.thread_fit_unknown",
        "不认识的螺纹配合「{other}」（本期只有 6H/6G）",
        "Unrecognized thread fit \"{other}\" (only 6H/6G for now)",
    ),
    Msg::new(
        "cmd.hole.err.d_invalid",
        "公称直径 d={d} 非法（必须 > 0）",
        "Nominal diameter d={d} is invalid (must be > 0)",
    ),
    Msg::new(
        "cmd.hole.err.simple_has_thread_subtype",
        "简单孔不带螺纹：子类型请选 钻头大小 / 自定义 / 螺栓间隙",
        "A simple hole is unthreaded: choose drill size / custom / bolt clearance as the subtype",
    ),
    Msg::new(
        "cmd.hole.err.threaded_needs_thread_subtype",
        "螺纹孔必须带螺纹：子类型请选 标准螺纹 / 细牙螺纹",
        "A threaded hole must be threaded: choose standard thread / fine thread as the subtype",
    ),
    Msg::new(
        "cmd.hole.err.cbore_csink_metric_only",
        "沉头/埋头推荐值表只覆盖公制 M（GB/T 152.3-1988 / 152.2-2014）—— 非公制螺纹请用简单孔/螺纹孔",
        "The counterbore/countersink recommendation tables cover metric M only (GB/T 152.3-1988 / 152.2-2014) — for non-metric threads use a simple/threaded hole",
    ),
    Msg::new(
        "cmd.hole.err.nonmetric_no_metric_fine",
        "非公制螺纹请用「子类型」选牙型系列（如 UNC/UNF/Stub ACME），不要用公制细牙",
        "For non-metric threads choose the thread family with the subtype (e.g. UNC/UNF/Stub ACME); do not use metric fine",
    ),
    Msg::new(
        "cmd.hole.err.fine_needs_concrete_pitch",
        "细牙螺纹需要具体螺距（大小里选 M10×1.25 这类）",
        "A fine thread needs a concrete pitch (pick e.g. M10×1.25 as the size)",
    ),
    Msg::new(
        "cmd.hole.err.fine_subtype_but_coarse",
        "子类型是细牙，但 M{d}×{p} 是粗牙规格",
        "The subtype is fine, but M{d}×{p} is a coarse-thread spec",
    ),
    Msg::new(
        "cmd.hole.err.standard_subtype_but_fine",
        "子类型是标准螺纹（粗牙），但 M{d}×{p} 是细牙规格 —— 请切「细牙螺纹」",
        "The subtype is standard thread (coarse), but M{d}×{p} is a fine-thread spec — switch to fine thread",
    ),
    Msg::new(
        "cmd.hole.err.minor_nonpositive",
        "螺纹 M{d}×{p} 的小径 d−1.0825P={minor} ≤ 0，参数非法",
        "The minor diameter d−1.0825P={minor} of M{d}×{p} is ≤ 0; invalid parameters",
    ),
    Msg::new(
        "cmd.hole.err.drill_required",
        "钻头大小需要选一个标准麻花钻直径（GB/T 6135.3 系列）",
        "Drill size requires a standard twist-drill diameter (GB/T 6135.3 series)",
    ),
    Msg::new(
        "cmd.hole.err.drill_not_in_series",
        "Ø{d} 不在 GB/T 6135.3-1996 直柄麻花钻直径系列里（0.20–20.00）—— 表外不插值",
        "Ø{d} is not in the GB/T 6135.3-1996 straight-shank twist-drill series (0.20–20.00) — no interpolation outside the table",
    ),
    Msg::new(
        "cmd.hole.err.custom_d_required",
        "自定义孔径必须填一个 > 0 的数值",
        "Custom hole diameter requires a value > 0",
    ),
    Msg::new(
        "cmd.hole.err.cbore_pilot_ge_sink",
        "底孔径 Ø{base} 不小于沉孔直径 Ø{sink}（GB/T 152.3 M{d}）",
        "Pilot diameter Ø{base} is not smaller than the counterbore diameter Ø{sink} (GB/T 152.3 M{d})",
    ),
    Msg::new(
        "cmd.hole.err.csink_pilot_ge_sink",
        "底孔径 Ø{base} 不小于埋头直径 Ø{sink}（GB/T 152.2 M{d}）",
        "Pilot diameter Ø{base} is not smaller than the countersink diameter Ø{sink} (GB/T 152.2 M{d})",
    ),
    Msg::new(
        "cmd.hole.err.full_thread_depth_manual",
        "螺纹有效长度选「全长」时孔深必须手填（自动孔深会循环依赖）",
        "With full-length thread length the hole depth must be entered manually (automatic depth would be circular)",
    ),
    Msg::new(
        "cmd.hole.err.npt_no_eff_len",
        "NPT{d} 表里没有螺纹有效长度 eff_len —— 表外不插值；请手填有效长度",
        "No effective thread length eff_len in the NPT{d} table — no interpolation outside the table; enter the effective length manually",
    ),
    Msg::new(
        "cmd.hole.err.r_no_eff_len",
        "R{d} 表里没有螺纹有效长度 eff_len（表第16栏）—— 表外不插值；请手填有效长度",
        "No effective thread length eff_len in the R{d} table (column 16) — no interpolation outside the table; enter the effective length manually",
    ),
    Msg::new(
        "cmd.hole.err.g_no_eff_len",
        "G{d} 没有有效长度（ISO 228-1 不规定；本表取同规格 R，而 R 无此规格）—— 不臆造：请手填有效长度，或改用 R / M",
        "G{d} has no effective length (ISO 228-1 does not define one; this table takes the same-size R, and R has no such size) — nothing is invented: enter the effective length manually, or switch to R / M",
    ),
    Msg::new(
        "cmd.hole.err.thread_len_invalid",
        "螺纹有效长度 L={len} 非法（必须 > 0）",
        "Effective thread length L={len} is invalid (must be > 0)",
    ),
    Msg::new(
        "cmd.hole.err.through_depth_manual",
        "贯通孔的孔深是板厚/通孔长度，不能自动 —— 请手填孔深",
        "For a through hole the depth is the plate thickness/through length and cannot be automatic — enter the hole depth manually",
    ),
    Msg::new(
        "cmd.hole.err.hole_depth_invalid",
        "孔深 H={h} 非法（必须 > 0）",
        "Hole depth H={h} is invalid (must be > 0)",
    ),
    Msg::new(
        "cmd.hole.err.thread_len_gt_hole_depth",
        "螺纹有效长度 L={l} 大于孔深 H={h}（有效长度必须在孔深内）",
        "Effective thread length L={l} exceeds hole depth H={h} (the effective length must fit inside the depth)",
    ),
    Msg::new(
        "cmd.hole.warn.runout_below_2p",
        "孔深 H={h} < 螺纹有效长度 L={l} + 2P={need}（工艺余量不足 2P）",
        "Hole depth H={h} < effective thread length L={l} + 2P={need} (runout margin below 2P)",
    ),
    Msg::new(
        "cmd.hole.err.simple_depth_manual",
        "简单孔：孔深不能自动（「自动」只用于螺纹孔）—— 请手填孔深",
        "Simple hole: depth cannot be automatic (automatic is only for threaded holes) — enter the hole depth manually",
    ),
    Msg::new(
        "cmd.hole.err.unthreaded_cbore_depth_manual",
        "不带螺纹的沉头/埋头孔：孔深不能自动 —— 请手填孔深",
        "Unthreaded counterbore/countersink: depth cannot be automatic — enter the hole depth manually",
    ),
    Msg::new(
        "cmd.hole.err.unthreaded_with_thread_subtype",
        "不带螺纹的孔不能用标准/细牙螺纹子类型",
        "An unthreaded hole cannot use the standard/fine-thread subtype",
    ),
    Msg::new(
        "cmd.hole.err.not_initialized",
        "这张图还没跑过 OCSM 初始化{why} —— 先执行 OCSM（建图层 + 线型 + 样式），再生成孔。",
        "This drawing has not run OCSM initialization{why} — run OCSM first (create layers + linetypes + styles), then generate holes.",
    ),
    Msg::new(
        "cmd.hole.why.missing_layers",
        "（缺图层：{layers}）",
        " (missing layers: {layers})",
    ),
    Msg::new(
        "cmd.hole.why.center_linetype",
        "（{layer} 没挂 CENTER2 点划线）",
        " ({layer} has no CENTER2 dash-dot linetype)",
    ),
    Msg::new(
        "cmd.hole.usage",
        "OCSMHOLE / DK：\n`OCSMHOLE [简单孔|螺纹孔|沉头孔|埋头孔] [带螺纹|无螺纹] [公制|UN/UNC/UNF/UNEF|G/BSPP|R/BSPT|NPT|ACME|矮牙|Tr] [钻孔 Ø8.5|自定义 孔径8.5|间隙 中等装配] [规格名 M10|M10×1.25|G1/8|1/4-20|NPT1/2|Tr8×1.5|ACME 1/4-16|公称6.35 P1.058] [P1.5] [推荐70.1|70.2] [H18] [L15] [盲孔|贯通] [全长] [6H|6G] [view 侧视图|俯视图|双视图] [at x,y] [rot 度]`\n（规格名直接查 GUI 同一份表 `name`：G 1/8、1/4-20、NPT1/2、Tr8x1.5 等都收；数值写法 `公称6.35 P1.058` 照旧）",
        "OCSMHOLE / DK:\n`OCSMHOLE [简单孔|螺纹孔|沉头孔|埋头孔] [带螺纹|无螺纹] [公制|UN/UNC/UNF/UNEF|G/BSPP|R/BSPT|NPT|ACME|矮牙|Tr] [钻孔 Ø8.5|自定义 孔径8.5|间隙 中等装配] [规格名 M10|M10×1.25|G1/8|1/4-20|NPT1/2|Tr8×1.5|ACME 1/4-16|公称6.35 P1.058] [P1.5] [推荐70.1|70.2] [H18] [L15] [盲孔|贯通] [全长] [6H|6G] [view 侧视图|俯视图|双视图] [at x,y] [rot deg]`\n(size names are looked up directly in the same `name` table as the GUI: G 1/8, 1/4-20, NPT1/2, Tr8x1.5 etc. are all accepted; the numeric form `公称6.35 P1.058` still works)",
    ),
    Msg::new(
        "cmd.hole.err.missing_args",
        "缺少参数",
        "Missing arguments",
    ),
    Msg::new(
        "cmd.hole.err.value_missing",
        "`{t}` 后面缺数值",
        "`{t}` is missing its value",
    ),
    Msg::new(
        "cmd.hole.err.spec_name_ambiguous",
        "规格名「{text}」命中多行（{names}）—— 请写全（带牙型系列/螺距）",
        "Spec name \"{text}\" matches multiple rows ({names}) — write it in full (with thread family/pitch)",
    ),
    Msg::new(
        "cmd.hole.err.view_name_missing",
        "`view` 后面缺视图名",
        "`view` is missing a view name",
    ),
    Msg::new(
        "cmd.hole.err.view_unknown",
        "不认识的视图「{other}」（侧视图/俯视图/双视图）",
        "Unrecognized view \"{other}\" (侧视图/俯视图/双视图)",
    ),
    Msg::new(
        "cmd.hole.err.at_xy_missing",
        "`at` 后面缺 x,y",
        "`at` is missing x,y",
    ),
    Msg::new(
        "cmd.hole.err.at_not_xy",
        "`at` 坐标应是 x,y（收到 {v}）",
        "`at` coordinates must be x,y (got {v})",
    ),
    Msg::new(
        "cmd.hole.err.x_invalid",
        "x 座标非法：{x}",
        "Invalid x coordinate: {x}",
    ),
    Msg::new(
        "cmd.hole.err.y_invalid",
        "y 座标非法：{y}",
        "Invalid y coordinate: {y}",
    ),
    Msg::new(
        "cmd.hole.err.rot_missing",
        "`rot` 后面缺角度",
        "`rot` is missing the angle",
    ),
    Msg::new(
        "cmd.hole.err.rot_invalid",
        "转角非法：{v}",
        "Invalid rotation angle: {v}",
    ),
    Msg::new(
        "cmd.hole.err.pitch_invalid",
        "螺距 P 非法：{v}",
        "Invalid pitch P: {v}",
    ),
    Msg::new(
        "cmd.hole.err.pitch_nonpositive",
        "螺距 P={p} 必须 > 0",
        "Pitch P={p} must be > 0",
    ),
    Msg::new(
        "cmd.hole.err.depth_invalid",
        "孔深非法：{v}",
        "Invalid hole depth: {v}",
    ),
    Msg::new(
        "cmd.hole.err.thread_len_parse_invalid",
        "螺纹有效长度非法：{v}",
        "Invalid effective thread length: {v}",
    ),
    Msg::new(
        "cmd.hole.err.custom_d_invalid",
        "自定义孔径非法：{v}",
        "Invalid custom hole diameter: {v}",
    ),
    Msg::new(
        "cmd.hole.err.fit_unsupported",
        "配合「{v}」不支持（6H/6G 或 精装配/中等装配/粗装配）",
        "Fit \"{v}\" is not supported (6H/6G or 精装配/中等装配/粗装配)",
    ),
    Msg::new(
        "cmd.hole.err.nominal_invalid",
        "公称直径非法：{rest}",
        "Invalid nominal diameter: {rest}",
    ),
    Msg::new(
        "cmd.hole.err.size_invalid",
        "尺寸非法：{t}（例 M10 / M10x1.25 / Ø8.5）",
        "Invalid size: {t} (e.g. M10 / M10x1.25 / Ø8.5)",
    ),
    Msg::new(
        "cmd.hole.err.pitch_token_invalid",
        "螺距非法：{t}",
        "Invalid pitch: {t}",
    ),
    Msg::new(
        "cmd.hole.err.unknown_param",
        "不识别的参数「{other}」（用法：{usage}）",
        "Unrecognized argument \"{other}\" (usage: {usage})",
    ),
    Msg::new(
        "cmd.hole.err.system_spec_mismatch",
        "体系与规格不匹配：{sys} 体系里没有「{text}」—— 这是 {conflicts}；请先写对体系，或改用本体系规格（示例：{examples}）",
        "System/spec mismatch: the {sys} system has no \"{text}\" — this is {conflicts}; write the correct system first, or use a spec from this system (examples: {examples})",
    ),
    Msg::new(
        "cmd.hole.err.spec_name_unknown",
        "未知规格名「{text}」（{sys} 体系）—— 表内示例：{examples}",
        "Unknown spec name \"{text}\" ({sys} system) — table examples: {examples}",
    ),
    Msg::new(
        "cmd.hole.err.size_missing",
        "缺少大小（例 `M10` / `M10×1.25`）",
        "Missing size (e.g. `M10` / `M10×1.25`)",
    ),
    Msg::new(
        "cmd.hole.err.need_one_view",
        "至少勾选一个视图（侧视图/俯视图）",
        "Select at least one view (侧视图/俯视图)",
    ),
    Msg::new(
        "cmd.thread.err.group_missing",
        "{sys} 没有子类型「{group}」（可用：{keys}）",
        "{sys} has no subtype \"{group}\" (available: {keys})",
    ),
    Msg::new(
        "cmd.thread.err.spec_missing",
        "{sys} 表里没有 {d}（mm, P={p}）—— 表外不插值；表内示例：{samples}",
        "The {sys} table has no {d} (mm, P={p}) — no interpolation outside the table; table examples: {samples}",
    ),
    Msg::new("cmd.thread.coarse_short", "粗牙", "coarse"),
    Msg::new(
        "cmd.thread.err.ambiguous",
        "{sys} {d} 命中多行（{names}）—— 请指定子类型/螺距",
        "{sys} {d} matches multiple rows ({names}) — specify a subtype/pitch",
    ),

    // ── 命令输出/报错：OCSMHOLE 命令层回执（lib.rs::cmd_hole）──────────
    Msg::new(
        "cmd.hole.err.parse_wrap",
        "OCSMHOLE 参数无效：{message}",
        "OCSMHOLE invalid arguments: {message}",
    ),
    Msg::new(
        "cmd.hole.err.geometry_wrap",
        "OCSMHOLE 几何非法：{message}",
        "OCSMHOLE invalid geometry: {message}",
    ),
    Msg::new(
        "cmd.hole.info.run_init_direct",
        "OCSMHOLE：先运行 OCSM（或点功能区「图幅」组里的 OCSM 初始化），再直接插孔。",
        "OCSMHOLE: run OCSM first (or click OCSM initialization in the ribbon Frame group), then insert holes directly.",
    ),
    Msg::new(
        "cmd.hole.info.run_init_window",
        "OCSMHOLE：先运行 OCSM 初始化，再打开孔生成器窗口。",
        "OCSMHOLE: run OCSM initialization first, then open the hole generator window.",
    ),
    Msg::new(
        "cmd.hole.err.server_start",
        "OCSMHOLE: 无法启动孔服务（宿主不支持 worker 请求）。",
        "OCSMHOLE: cannot start the hole service (host does not support worker requests).",
    ),
    Msg::new(
        "cmd.hole.info.window_opened",
        "OCSM 孔生成器：已打开窗口（类型 2×2 + 子类型/大小/配合 + 盲孔/贯通 + 孔深/螺纹范围自动）。点「确定」→ 回到图纸点击定位基点 → 移动光标旋转 → 再点击落定（可连续，Esc 结束）。",
        "OCSM Hole Generator: window opened (type 2×2 + subtype/size/fit + blind/through + automatic depth/thread range). Click OK → back in the drawing click the base point → move the cursor to rotate → click to place (repeatable; Esc to finish).",
    ),
    Msg::new(
        "cmd.hole.info.window_focus",
        "OCSM 孔生成器：窗口已打开（Alt+Tab 切换过去）。",
        "OCSM Hole Generator: the window is already open (Alt+Tab to switch to it).",
    ),
    Msg::new(
        "cmd.hole.hint.click_ok",
        "请在孔生成器窗口里点「确定」",
        "Click OK in the hole generator window",
    ),
    Msg::new("cmd.hole.place_what", "OCSM 孔", "OCSM Hole"),
    Msg::new("cmd.hole.kind.simple", "简单孔", "simple hole"),
    Msg::new("cmd.hole.kind.threaded", "螺纹孔", "threaded hole"),
    Msg::new("cmd.hole.kind.counterbore", "沉头孔", "counterbore"),
    Msg::new("cmd.hole.kind.countersink", "埋头孔", "countersink"),
    Msg::new(
        "cmd.hole.echo.created",
        "OCSMHOLE：已生成{kind} {size}（底孔Ø{drill}，深{depth}，{count} 个图元）于 ({x}, {y}) rot {rot}° → 1轮廓实线层 / 2细线层 / 3中心线层",
        "OCSMHOLE: created {kind} {size} (pilot Ø{drill}, depth {depth}, {count} entities) at ({x}, {y}) rot {rot}° → 1轮廓实线层 / 2细线层 / 3中心线层",
    ),
    Msg::new(
        "cmd.hole.info.warn_prefix",
        "OCSMHOLE 提示：{w}",
        "OCSMHOLE notice: {w}",
    ),

    // ── 命令输出/提示：放置态 prompt（lib.rs::PartPlace；★ 各命令共用）─────
    Msg::new(
        "cmd.place.prompt.wait",
        "{what}：{where_to}，然后在此点击放置。",
        "{what}: {where_to}, then click here to place.",
    ),
    Msg::new(
        "cmd.place.prompt.follow",
        "{what}：{l} —— 点击定位基点（可连续，Esc 结束）",
        "{what}: {l} — click to set the base point (repeatable; Esc to finish)",
    ),
    Msg::new(
        "cmd.place.prompt.rotate",
        "{what}：{l} —— 移动光标绕基点旋转，再点击落定（Esc 取消）",
        "{what}: {l} — move the cursor to rotate about the base point, then click to place (Esc to cancel)",
    ),

    // ── 命令输出/报错：中心线（OCSMCENTERLINE / ZX；centerline.rs）──────
    Msg::new(
        "cmd.centerline.err.bad_pick",
        "只能点圆/圆弧或直线",
        "Pick a circle/arc or a line",
    ),
    Msg::new(
        "cmd.centerline.err.zero_len_first",
        "这根线长度是 0，换一根",
        "This line has zero length; pick another",
    ),
    Msg::new(
        "cmd.centerline.err.zero_len_pair",
        "两根线里有一根长度是 0，重来",
        "One of the two lines has zero length; start over",
    ),
    Msg::new(
        "cmd.centerline.prompt.second",
        "OCSMCENTERLINE：再点第二根直线（角平分线中心线），Enter 取消",
        "OCSMCENTERLINE: pick the second line (angle-bisector centerline); Enter to cancel",
    ),
    Msg::new(
        "cmd.centerline.prompt.notice",
        "OCSMCENTERLINE：{n}",
        "OCSMCENTERLINE: {n}",
    ),
    Msg::new(
        "cmd.centerline.prompt.pick",
        "OCSMCENTERLINE 中心线：点圆/圆弧（十字）或直线（角平分线）",
        "OCSMCENTERLINE centerline: click a circle/arc (cross) or a line (bisector)",
    ),
    Msg::new(
        "cmd.centerline.err.no_object",
        "没点到对象：请点在圆/圆弧或直线上",
        "Nothing picked: click on a circle/arc or a line",
    ),
    Msg::new(
        "cmd.centerline.err.bad_selection",
        "OCSMCENTERLINE：选区不是「1 个圆/圆弧」或「2 根直线」→ 改为点选",
        "OCSMCENTERLINE: selection is not 1 circle/arc or 2 lines → switching to click picking",
    ),
    Msg::new(
        "cmd.centerline.undo_label",
        "OCSMCENTERLINE 中心线",
        "OCSMCENTERLINE centerline",
    ),
    Msg::new(
        "cmd.centerline.info.cross",
        "OCSMCENTERLINE：十字中心线（Ø{dia} + {scale}×{over}，线长 {len}）→ {layer}",
        "OCSMCENTERLINE: cross centerline (Ø{dia} + {scale}×{over}, line length {len}) → {layer}",
    ),
    Msg::new(
        "cmd.centerline.info.bisector",
        "OCSMCENTERLINE：角平分线中心线（长 {len} = 投影 {proj} + {scale}×{over}）→ {layer}",
        "OCSMCENTERLINE: angle-bisector centerline (length {len} = projection {proj} + {scale}×{over}) → {layer}",
    ),
    Msg::new(
        "cmd.centerline.info.cross_selection",
        "OCSMCENTERLINE：十字中心线（Ø{dia} + {scale}×{over}）→ {layer}",
        "OCSMCENTERLINE: cross centerline (Ø{dia} + {scale}×{over}) → {layer}",
    ),
    Msg::new(
        "cmd.centerline.info.bisector_selection",
        "OCSMCENTERLINE：角平分线中心线（第一根线 = 选区里第一个，长 {len}）→ {layer}",
        "OCSMCENTERLINE: angle-bisector centerline (first line = first in selection, length {len}) → {layer}",
    ),
    Msg::new(
        "cmd.centerline.info.draw_count",
        "OCSMCENTERLINE：已画出 {n} 条中心线",
        "OCSMCENTERLINE: drew {n} centerlines",
    ),

    // ── 命令输出/报错：序号球标（balloon.rs）────────────────────────
    Msg::new(
        "cmd.balloon.err.need_item",
        "序号标注：至少要有一个序号",
        "Item-number annotation: at least one item number is required",
    ),
    Msg::new(
        "cmd.balloon.err.blank_item",
        "序号标注：序号不能为空",
        "Item-number annotation: item numbers cannot be blank",
    ),
    Msg::new(
        "cmd.balloon.err.bad_scale",
        "序号标注：图幅倍率无效",
        "Item-number annotation: invalid frame scale",
    ),
    Msg::new(
        "cmd.balloon.err.not_horizontal",
        "序号标注：横线必须水平（第二段昣平画）——不支持竖肩线（序号不旋转）",
        "Item-number annotation: the horizontal line must be horizontal (the second segment is drawn flat) — vertical shoulder lines are not supported (item numbers are never rotated)",
    ),
    Msg::new(
        "cmd.balloon.err.zero_segment",
        "序号标注：第二段（横线方向段）长度为零",
        "Item-number annotation: the second segment (horizontal direction) has zero length",
    ),

    // ── 命令输出/报错：图框（TF / OCSMFRAMEINIT / OCSMFRAMEINSERT；lib.rs）──
    Msg::new(
        "cmd.frame.err.folder_missing",
        "找不到图框文件夹 {dir}。请在插件目录的 frame/ 中放入 DWG。",
        "Frame folder {dir} not found. Put DWG files in frame/ inside the plugin folder.",
    ),
    Msg::new(
        "cmd.frame.err.no_dwg",
        "{dir} 下没有 DWG 图框文件。",
        "No DWG frame files under {dir}.",
    ),
    Msg::new(
        "cmd.frame.err.need_name",
        "图框目录里有 {n} 个图框，请指名一个：{names}。",
        "The frame folder has {n} frames; name one: {names}.",
    ),
    Msg::new(
        "cmd.frame.err.name_missing",
        "没有叫「{want}」的图框。现有：{names}。",
        "No frame named \"{want}\". Available: {names}.",
    ),
    Msg::new(
        "cmd.frame.err.name_ambiguous",
        "「{want}」匹配到多个图框（{names}），请写全名。",
        "\"{want}\" matches multiple frames ({names}); write the full name.",
    ),
    Msg::new(
        "cmd.frame.info.found_n",
        "OCSMFRAMEINIT: 找到 {n} 个图框，正在打开选择窗口…",
        "OCSMFRAMEINIT: found {n} frames; opening the picker…",
    ),
    Msg::new(
        "cmd.frame.err.picker_failed",
        "OCSMFRAMEINIT: 无法打开图框选择窗口。",
        "OCSMFRAMEINIT: cannot open the frame picker.",
    ),
    Msg::new(
        "cmd.frame.err.bad_scale",
        "{who}: 非法的比例（必须为两个正整数且其一为 1，如 1:2 / 2:1 / 1:1）。",
        "{who}: invalid scale (two positive integers, one of which must be 1, e.g. 1:2 / 2:1 / 1:1).",
    ),
    Msg::new(
        "cmd.frame.prompt.specify_point",
        "指定图框「{block}」的插入点（比例 {scale}）：",
        "Specify the insertion point of frame \"{block}\" (scale {scale}):",
    ),
    Msg::new(
        "cmd.frame.info.specify_point",
        "指定图框「{block}」的插入点（比例 {scale}，缩放 {s:.2} 倍）…",
        "Specify the insertion point of frame \"{block}\" (scale {scale}, zoom {s:.2}×)…",
    ),
    Msg::new(
        "cmd.frame.info.inserted",
        "OCSMFRAMEINSERT: 已插入图框「{block}」比例 {scale}（缩放 {s:.2} 倍）基点 ({x}, {y})，旋转 {rot}°",
        "OCSMFRAMEINSERT: inserted frame \"{block}\" scale {scale} (zoom {s:.2}×) at ({x}, {y}), rotation {rot}°",
    ),
    Msg::new(
        "cmd.frame.output.inserted",
        "图框 {block} 插入完成（比例 {scale}）",
        "Frame {block} inserted (scale {scale})",
    ),
    Msg::new("cmd.frame.undo_label", "OCSM 插入图框", "OCSM insert frame"),
    Msg::new(
        "cmd.frame.err.no_pending",
        "OCSMFRAMEINSERT: 没有待处理的图框选择。请先运行 TF。",
        "OCSMFRAMEINSERT: no pending frame selection. Run TF first.",
    ),

    // ── 命令输出/报错：TF 参数校验（lib.rs::parse_frame_args / parse_point / parse_scale）──
    Msg::new(
        "cmd.tf.err.at_twice",
        "基点给了两次。{usage}",
        "Base point given twice. {usage}",
    ),
    Msg::new(
        "cmd.tf.err.at_needs_coords",
        "`at` 后面要给坐标，如 at 0,0。{usage}",
        "`at` must be followed by coordinates, e.g. at 0,0. {usage}",
    ),
    Msg::new(
        "cmd.tf.err.rot_needs_angle",
        "`rot` 后面要给角度。{usage}",
        "`rot` must be followed by an angle. {usage}",
    ),
    Msg::new(
        "cmd.tf.err.rot_not_number",
        "旋转角度「{next}」不是数字。{usage}",
        "Rotation angle \"{next}\" is not a number. {usage}",
    ),
    Msg::new(
        "cmd.tf.err.scale_twice",
        "比例给了两次。{usage}",
        "Scale given twice. {usage}",
    ),
    Msg::new(
        "cmd.tf.err.scale_invalid",
        "比例必须是两个正整数且其一为 1（例 1:2 / 2:1 / 1:1）。",
        "Scale must be two positive integers, one of which is 1 (e.g. 1:2 / 2:1 / 1:1).",
    ),
    Msg::new(
        "cmd.tf.err.point_form",
        "坐标「{tok}」应写成 x,y 或 x,y,z。",
        "Coordinate \"{tok}\" must be written as x,y or x,y,z.",
    ),
    Msg::new(
        "cmd.tf.err.point_not_number",
        "坐标「{tok}」里「{p}」不是数字。",
        "\"{p}\" in coordinate \"{tok}\" is not a number.",
    ),
    Msg::new(
        "cmd.tf.err.scale_form",
        "比例「{tok}」写法不对（示例 1:2）。",
        "Scale \"{tok}\" has the wrong form (e.g. 1:2).",
    ),
    Msg::new(
        "cmd.tf.err.scale_not_number",
        "比例「{tok}」不是数字（示例 1:2 / 2 / 0.5）。",
        "Scale \"{tok}\" is not a number (e.g. 1:2 / 2 / 0.5).",
    ),
    Msg::new(
        "cmd.tf.err.scale_not_positive",
        "比例「{tok}」必须是正数。",
        "Scale \"{tok}\" must be positive.",
    ),
    Msg::new(
        "cmd.tf.err.scale_part_not_number",
        "比例「{whole}」里「{s}」不是数字。",
        "\"{s}\" in scale \"{whole}\" is not a number.",
    ),
    Msg::new(
        "cmd.tf.err.scale_not_integer_ratio",
        "比例「{whole}」必须是整数比（示例 1:2）。",
        "Scale \"{whole}\" must be an integer ratio (e.g. 1:2).",
    ),
];

/// 手动覆盖：环境变量 `OCSMLANG`（最高优先级的进程外开关）。
pub const LANG_ENV: &str = "OCSMLANG";
/// 手动覆盖：`OCSM_LANG`（兼容别名）。
pub const LANG_ENV_ALIAS: &str = "OCSM_LANG";

// 0 = 自动（跟随环境），1 = zh，2 = en。
const LANG_AUTO: u8 = 0;
const LANG_ZH: u8 = 1;
const LANG_EN: u8 = 2;

static OVERRIDE: AtomicU8 = AtomicU8::new(LANG_AUTO);
/// 环境解析结果缓存（`OnceLock` 不能复位，测试要复位 → 用 `Mutex<Option<_>>`）。
static AUTO_CACHE: Mutex<Option<Lang>> = Mutex::new(None);
/// 运行期缺 key 点名表（有上限，避免异常路径把内存撑爆）。
static MISSING: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// 缺 key 记录上限。
const MISSING_LIMIT: usize = 256;

// 单测（`cargo test`）：语言覆盖**只作用于当前测试线程**。
// `cargo test` 一个进程里并行跑所有用例，而生产里语言是进程级；若测试里
// `set_lang` 也写全局，一个切 En 的用例会把断言中文文案的用例带崩（曾实测）。
// 测试线程内的产品路径仍走这层；服务端/其它线程用全局/环境（稳）。
// 产品构建没有这层：GUI `/api/i18n` 的切换仍是进程级 `OVERRIDE`。
#[cfg(test)]
thread_local! {
    static TEST_OVERRIDE: std::cell::Cell<Option<Lang>> = const { std::cell::Cell::new(None) };
}

/// 按 key 查词条（线性扫描；catalog 到几百条前够用，变大再换表）。
pub fn lookup(key: &str) -> Option<&'static Msg> {
    CATALOG.iter().find(|m| m.key == key)
}

/// 当前语言（解析见模块头注释）。
pub fn lang() -> Lang {
    #[cfg(test)]
    if let Some(l) = TEST_OVERRIDE.with(|c| c.get()) {
        return l;
    }
    match OVERRIDE.load(Ordering::Relaxed) {
        LANG_ZH => Lang::Zh,
        LANG_EN => Lang::En,
        _ => {
            let mut cache = AUTO_CACHE.lock().unwrap_or_else(|e| e.into_inner());
            *cache.get_or_insert_with(resolve_env_lang)
        }
    }
}

/// 手动覆盖当前语言（GUI 设置 / 命令开关 / 测试）。
///
/// `cargo test` 下只覆盖**当前线程**（并行用例互不干扰，见 `TEST_OVERRIDE`）；
/// 产品构建写进程级 `OVERRIDE`（GUI `/api/i18n` 选一次全插件生效）。
pub fn set_lang(lang: Lang) {
    #[cfg(test)]
    TEST_OVERRIDE.with(|c| c.set(Some(lang)));
    #[cfg(not(test))]
    OVERRIDE.store(
        match lang {
            Lang::Zh => LANG_ZH,
            Lang::En => LANG_EN,
        },
        Ordering::Relaxed,
    );
}

/// 复位为「自动」（跟随环境；顺带丢掉环境解析缓存，避免拿到旧值）。
pub fn set_lang_auto() {
    #[cfg(test)]
    TEST_OVERRIDE.with(|c| c.set(None));
    #[cfg(not(test))]
    OVERRIDE.store(LANG_AUTO, Ordering::Relaxed);
    reset_auto_cache();
}

/// 复位环境解析缓存（进程内改了 `LANG*` / `OCSMLANG` 后想立刻生效时调用；
/// 正常部署只在进程启动时解析一次）。
pub fn reset_auto_cache() {
    *AUTO_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// 解析一个语言值（`zh-CN` / `en_US.UTF-8` / `C` …）；不认识的返回 `None`。
pub fn parse_lang(raw: &str) -> Option<Lang> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    let head = s.split(['_', '-', '.', '@']).next().unwrap_or("");
    match head {
        "zh" | "cn" | "chinese" => Some(Lang::Zh),
        "en" | "english" => Some(Lang::En),
        _ => None,
    }
}

/// 环境变量 → 语言（含手动覆盖 `OCSMLANG`；见模块头优先级）。默认 `zh`。
pub fn resolve_env_lang() -> Lang {
    for var in [LANG_ENV, LANG_ENV_ALIAS, "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            if let Some(l) = parse_lang(&v) {
                return l;
            }
        }
    }
    Lang::Zh
}

/// 语言来源诊断（GUI / 日志）：返回 `(变量名, 原值)`；全未命中 = `None`。
pub fn env_lang_source() -> Option<(&'static str, String)> {
    for var in [LANG_ENV, LANG_ENV_ALIAS, "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            if parse_lang(&v).is_some() {
                return Some((var, v));
            }
        }
    }
    None
}

fn note_missing(key: &str) {
    let mut set = MISSING.lock().unwrap_or_else(|e| e.into_inner());
    if set.len() < MISSING_LIMIT && !set.iter().any(|k| k == key) {
        set.push(key.to_string());
    }
}

/// 渲染一条词条（空串 → 回落 `zh`；`zh` 也空 → `None`）。单测直接喂构造词条。
fn render_msg(m: &Msg, lang: Lang) -> Option<&'static str> {
    let pick = match lang {
        Lang::Zh => m.zh,
        Lang::En => m.en,
    };
    if !pick.is_empty() {
        Some(pick)
    } else if !m.zh.is_empty() {
        Some(m.zh)
    } else {
        None
    }
}

/// 取词（按当前语言；返回 owned `String`，避免把 catalog 生命周期漏到调用方）。
pub fn t(key: &str) -> String {
    t_lang(lang(), key)
}

/// 取词（指定语言；测试 / GUI 预渲染用）。
pub fn t_lang(lang: Lang, key: &str) -> String {
    match lookup(key) {
        Some(m) => match render_msg(m, lang) {
            Some(s) => s.to_string(),
            // catalog 里 zh 也是空（不该发生；catalog_problems 会点名）→ 可见 key。
            None => {
                note_missing(key);
                key.to_string()
            }
        },
        None => {
            note_missing(key);
            // 保守默认：不返回空串 / 不 panic；key 本身可见，便于一眼定位。
            key.to_string()
        }
    }
}

/// 带参数插值：`{name}` → `args` 里的值（未给到的占位符原样保留）。
pub fn t_fmt(key: &str, args: &[(&str, &str)]) -> String {
    t_fmt_lang(lang(), key, args)
}

/// 带参数插值（指定语言）。
pub fn t_fmt_lang(lang: Lang, key: &str, args: &[(&str, &str)]) -> String {
    let mut out = t_lang(lang, key);
    for (k, v) in args {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// 按语言列出全 catalog（`/api/i18n` 用；目标语言为空时回落 `zh`）。
pub fn catalog_pairs(lang: Lang) -> Vec<(&'static str, String)> {
    CATALOG
        .iter()
        .map(|m| (m.key, render_msg(m, lang).unwrap_or(m.key).to_string()))
        .collect()
}

/// 运行期缺失 key 名单（去重点名；有上限）。测试 / 联调用。
pub fn missing_keys() -> Vec<String> {
    MISSING.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// 清空缺失记录。
pub fn clear_missing_keys() {
    MISSING.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// catalog 静态问题清单：空 key / 重复 key / 任一语为空 / `{占位符}` 两语不一致。
/// 空 = 健康。
pub fn catalog_problems() -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for m in CATALOG {
        if m.key.trim().is_empty() {
            problems.push("空 key".to_string());
        }
        if seen.contains(&m.key) {
            problems.push(format!("重复 key：{}", m.key));
        } else {
            seen.push(m.key);
        }
        if m.zh.trim().is_empty() {
            problems.push(format!("{}：zh 为空", m.key));
        }
        if m.en.trim().is_empty() {
            problems.push(format!("{}：en 为空", m.key));
        }
        if placeholders(m.zh) != placeholders(m.en) {
            problems.push(format!("{}：占位符 zh/en 不一致", m.key));
        }
    }
    problems
}

/// 提取 `{name}` 占位符名（排序去重）。
fn placeholders(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else { break };
        let name = &after[..end];
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            out.push(name.to_string());
        }
        rest = &after[end + 1..];
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用例之间语言/环境是进程级全局 —— 与仓内各模块共用同一把测试锁。
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        crate::global_state_test_lock()
    }

    #[test]
    fn catalog_is_complete_and_has_no_duplicate_keys() {
        let problems = catalog_problems();
        assert!(problems.is_empty(), "catalog 静态问题：{problems:?}");
        assert!(!CATALOG.is_empty(), "catalog 不该为空（至少竖切域词条）");
    }

    #[test]
    fn placeholder_extraction_handles_nested_and_bad_forms() {
        assert_eq!(placeholders("a {x} b {y} {x}"), vec!["x", "y"]);
        assert_eq!(placeholders("无占位符"), Vec::<String>::new());
        assert_eq!(placeholders("残缺 {x"), Vec::<String>::new());
        assert_eq!(placeholders("{}"), Vec::<String>::new());
    }

    #[test]
    fn t_switches_between_zh_and_en() {
        let _g = lock();
        set_lang(Lang::Zh);
        let zh = t("cmd.tf.usage");
        assert!(zh.starts_with("用法：TF <名称>"), "中文取词：{zh}");
        set_lang(Lang::En);
        let en = t("cmd.tf.usage");
        assert!(en.starts_with("Usage: TF <name>"), "英文取词：{en}");
        assert_ne!(zh, en);
        set_lang_auto();
    }

    #[test]
    fn t_fmt_interpolates_in_both_languages() {
        let _g = lock();
        let arg = "extra";
        let usage = "U";
        let zh = t_fmt_lang(
            Lang::Zh,
            "cmd.tf.err.unknown_param",
            &[("arg", arg), ("usage", usage)],
        );
        assert_eq!(zh, "认不出的参数「extra」。U");
        let en = t_fmt_lang(
            Lang::En,
            "cmd.tf.err.unknown_param",
            &[("arg", arg), ("usage", usage)],
        );
        assert_eq!(en, "Unrecognized argument \"extra\". U");
    }

    #[test]
    fn missing_en_falls_back_to_zh_and_is_named() {
        let _g = lock();
        // 合成词条：en 空 → 回落 zh（真实 catalog 由完整性测试保证不为空）。
        let m = Msg::new("synthetic.only.zh", "只有中文", "");
        assert_eq!(render_msg(&m, Lang::En), Some("只有中文"));
        assert_eq!(render_msg(&m, Lang::Zh), Some("只有中文"));
        // zh / en 都空 → None（调用方转成「可见 key」，不是空串）。
        let empty = Msg::new("synthetic.empty", "", "");
        assert_eq!(render_msg(&empty, Lang::En), None);

        // catalog 里根本没有的 key：返回 key 本身 + 运行期点名，绝不空串。
        clear_missing_keys();
        let got = t_lang(Lang::En, "no.such.key.at.all");
        assert_eq!(got, "no.such.key.at.all");
        assert!(missing_keys().iter().any(|k| k == "no.such.key.at.all"));
        clear_missing_keys();
    }

    #[test]
    fn env_resolution_prefers_ocsmlang_then_locale() {
        let _g = lock();
        // 环境变量是进程级全局：改前存旧值，测完恢复。
        let vars = [LANG_ENV, LANG_ENV_ALIAS, "LC_ALL", "LC_MESSAGES", "LANG"];
        let saved: Vec<(&str, Option<String>)> =
            vars.iter().map(|v| (*v, std::env::var(v).ok())).collect();
        for v in vars {
            std::env::remove_var(v);
        }
        assert_eq!(resolve_env_lang(), Lang::Zh, "全无环境 → 保守默认 zh");
        std::env::set_var("LANG", "en_US.UTF-8");
        assert_eq!(resolve_env_lang(), Lang::En, "跟随 LANG");
        std::env::set_var("LC_ALL", "zh_CN.UTF-8");
        assert_eq!(resolve_env_lang(), Lang::Zh, "LC_ALL 优先于 LANG");
        std::env::set_var(LANG_ENV, "en");
        assert_eq!(resolve_env_lang(), Lang::En, "OCSMLANG 是手动覆盖（最高）");
        std::env::set_var(LANG_ENV, "de_DE");
        assert_eq!(
            resolve_env_lang(),
            Lang::Zh,
            "不认识的值跳过，看下一来源（LC_ALL=zh 生效）"
        );
        std::env::set_var("LC_ALL", "de_DE");
        assert_eq!(resolve_env_lang(), Lang::En, "LC_ALL 也不认识 → 落到 LANG=en");
        assert_eq!(parse_lang("zh-CN"), Some(Lang::Zh));
        assert_eq!(parse_lang("EN_us"), Some(Lang::En));
        assert_eq!(parse_lang("C"), None);
        assert_eq!(parse_lang("  "), None);
        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}
