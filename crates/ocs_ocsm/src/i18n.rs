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

/// 按 key 查词条（线性扫描；catalog 到几百条前够用，变大再换表）。
pub fn lookup(key: &str) -> Option<&'static Msg> {
    CATALOG.iter().find(|m| m.key == key)
}

/// 当前语言（解析见模块头注释）。
pub fn lang() -> Lang {
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
pub fn set_lang(lang: Lang) {
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
