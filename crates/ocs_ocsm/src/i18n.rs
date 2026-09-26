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

    // ── 命令输出/报错：螺栓副装配（OCSMJOINT；joint.rs）───────────────────
    Msg::new(
        "cmd.joint.usage",
        "用法：OCSMJOINT at x,y rot 度 [protrude 扣数] bolt=<族>:<d>[:<l>] [plate=<厚>|gap=<厚>|<螺母或垫圈族>=<d>] …",
        "Usage: OCSMJOINT at x,y rot deg [protrude turns] bolt=<family>:<d>[:<l>] [plate=<t>|gap=<t>|<nut or washer family>=<d>] …",
    ),
    Msg::new(
        "cmd.joint.err.at_needs_coords",
        "at 缺坐标。{usage}",
        "at: missing coordinates. {usage}",
    ),
    Msg::new(
        "cmd.joint.err.at_needs_y",
        "at 缺 y。{usage}",
        "at: missing y. {usage}",
    ),
    Msg::new(
        "cmd.joint.err.rot_needs_angle",
        "rot 缺角度。{usage}",
        "rot: missing angle. {usage}",
    ),
    Msg::new(
        "cmd.joint.err.protrude_needs_number",
        "protrude 缺数值。{usage}",
        "protrude: missing value. {usage}",
    ),
    Msg::new(
        "cmd.joint.err.protrude_negative",
        "protrude 不能为负（扣数）",
        "protrude cannot be negative (turns)",
    ),
    Msg::new(
        "cmd.joint.err.view_needs_name",
        "view 缺视图名。{usage}",
        "view: missing view name. {usage}",
    ),
    Msg::new(
        "cmd.joint.err.at_required",
        "缺 at x,y。{usage}",
        "Missing at x,y. {usage}",
    ),
    Msg::new(
        "cmd.joint.err.empty_chain",
        "件链为空。{usage}",
        "The part chain is empty. {usage}",
    ),
    Msg::new(
        "cmd.joint.err.empty_chain_bare",
        "件链为空",
        "the part chain is empty",
    ),
    Msg::new(
        "cmd.joint.err.chain_needs_bolt",
        "件链必须以 bolt=<族>:<d> 开头（它决定轴线与长度基准）。",
        "The part chain must start with bolt=<family>:<d> (it defines the axis and the length datum).",
    ),
    Msg::new(
        "cmd.joint.err.chain_needs_bolt_short",
        "件链必须以 bolt=<族>:<d> 开头",
        "The part chain must start with bolt=<family>:<d>",
    ),
    Msg::new(
        "cmd.joint.err.bad_json",
        "请求 JSON 无效: {e}",
        "Invalid request JSON: {e}",
    ),
    Msg::new(
        "cmd.joint.err.unknown_kind_json",
        "未知件类型: {other}",
        "Unknown part kind: {other}",
    ),
    Msg::new(
        "cmd.joint.err.chain_needs_bolt_json",
        "件链必须以 kind=bolt 开头",
        "The part chain must start with kind=bolt",
    ),
    Msg::new(
        "cmd.joint.err.family_no_size",
        "零件库没有 {family} 的 {d}，可用直径：{hint}",
        "The parts library has no {d} in {family}. Available diameters: {hint}",
    ),
    Msg::new(
        "cmd.joint.err.no_pitch",
        "{family} {d} 没有螺距数据，无法按扣数算露出",
        "{family} {d} has no pitch data; cannot compute protrusion in turns",
    ),
    Msg::new(
        "cmd.joint.err.item_size_missing",
        "{item} 的规格在零件库里查不到",
        "No library entry for the size of {item}",
    ),
    Msg::new(
        "cmd.joint.err.length_too_short",
        "{family} {d} 的供货长度最大 {lmax}，装不下（需 ≥ {need}）。换更长的族或改件链。",
        "{family} {d}: longest supplied length is {lmax}, too short (need ≥ {need}). Use a longer family or change the chain.",
    ),
    Msg::new(
        "cmd.joint.err.item_format",
        "件格式应为 `<类型>=<族>:<规格>` 或 `plate=<厚>`（收到 {token}）",
        "Part format must be `<kind>=<family>:<size>` or `plate=<t>` (got {token})",
    ),
    Msg::new(
        "cmd.joint.err.item_colon",
        "{token} 应为 <类型>=<族>:<规格>，如 bolt=hex_bolt_b_full:8",
        "{token} must be <kind>=<family>:<size>, e.g. bolt=hex_bolt_b_full:8",
    ),
    Msg::new(
        "cmd.joint.err.no_screw",
        "件链不支持螺钉（螺钉拧入螺纹孔、不配螺母）；螺栓副请用 六角头螺栓族",
        "The part chain does not accept screws (a screw threads into a tapped hole and takes no nut); use a hex-head bolt family for a bolted joint",
    ),
    Msg::new(
        "cmd.joint.err.unknown_kind",
        "未知件类型 {other}（可用 bolt/nut/washer/plate/gap，或直接写族名）",
        "Unknown part kind {other} (use bolt/nut/washer/plate/gap, or write the family name directly)",
    ),
    Msg::new(
        "cmd.joint.err.screw_not_expected",
        "{family} 是螺钉（screw）——螺钉拧入螺纹孔、不配螺母，不能当件链的 {expected} 用",
        "{family} is a screw — it threads into a tapped hole and takes no nut, so it cannot serve as the {expected} in the chain",
    ),
    Msg::new(
        "cmd.joint.err.family_kind_mismatch",
        "{family} 不是{expected}族",
        "{family} is not a {expected} family",
    ),
    Msg::new(
        "cmd.joint.err.size_format",
        "规格最多 <d>[:<l>]（收到 {raw}）",
        "Size must be at most <d>[:<l>] (got {raw})",
    ),
    Msg::new(
        "cmd.joint.err.library_no_d",
        "零件库没有 {family} 的 M{d}（可用：{hint}）",
        "The parts library has no M{d} in {family}. Available: {hint}",
    ),
    Msg::new(
        "cmd.joint.err.screw_not_bolt",
        "{family} 是螺钉（screw）——螺钉拧入螺纹孔、不配螺母，不是螺栓；件链只支持 螺栓（六角头螺栓族）/螺母/垫圈",
        "{family} is a screw — it threads into a tapped hole and takes no nut, so it is not a bolt. The chain supports only bolts (hex-head bolt families) / nuts / washers",
    ),
    Msg::new(
        "cmd.joint.err.kind_unsupported",
        "{family} 是 {other} 类，件链里只支持螺栓/螺母/垫圈",
        "{family} is of kind {other}; the chain supports only bolts/nuts/washers",
    ),
    Msg::new(
        "cmd.joint.err.family_not_implemented",
        "（该族未实现）",
        "(family not implemented)",
    ),
    Msg::new(
        "cmd.joint.note.no_plates",
        "无被连接件",
        "no connected parts",
    ),
    Msg::new(
        "cmd.joint.report",
        "OCSMJOINT：{bolt_name} {bolt_code} {d}×{l}｜件链 {chain} → Σ{stack}｜需 l ≥ {need}(= Σ{stack} + 露出{turns}扣×{pitch}) → 取供货长度 {l2}（实际外露 {pm}mm≈{pt}扣）｜基点 ({x}, {y}) rot {rot}°｜共 {n} 件",
        "OCSMJOINT: {bolt_name} {bolt_code} {d}×{l} | chain {chain} → Σ{stack} | need l ≥ {need} (= Σ{stack} + protrusion {turns} turns × {pitch}) → use supplied length {l2} (actual protrusion {pm} mm ≈ {pt} turns) | base point ({x}, {y}) rot {rot}° | {n} parts",
    ),
    Msg::new(
        "cmd.joint.item.bolt_with_length",
        "{name} {code} {d}×{l}",
        "{name} {code} {d}×{l}",
    ),
    Msg::new(
        "cmd.joint.item.bolt",
        "{name} {code} {d}",
        "{name} {code} {d}",
    ),
    Msg::new(
        "cmd.joint.item.nut",
        "{name} {code} {d}(m{thick})",
        "{name} {code} {d}(m{thick})",
    ),
    Msg::new(
        "cmd.joint.item.washer",
        "{name} {code} {d}(厚{thick})",
        "{name} {code} {d}(thk {thick})",
    ),
    Msg::new(
        "cmd.joint.item.plate",
        "板厚 {t}",
        "plate t={t}",
    ),
    Msg::new(
        "cmd.joint.item.gap",
        "间隔 {t}",
        "gap {t}",
    ),
    Msg::new(
        "cmd.joint.note.hidden",
        "{name} {spec} 被遮 {iv}",
        "{name} {spec} hidden {iv}",
    ),
    Msg::new(
        "cmd.joint.err.empty_geometry",
        "整链几何为空",
        "Joint geometry is empty",
    ),
    Msg::new(
        "cmd.joint.err.not_number",
        "不是有效数字: {raw}",
        "Not a valid number: {raw}",
    ),
    Msg::new(
        "cmd.joint.err.not_finite",
        "不是有限数字: {raw}",
        "Not a finite number: {raw}",
    ),

    // ── 命令输出/报错：一键转国标（OCSMDIM2GB / D2G；dim2gb.rs）───────
    Msg::new(
        "cmd.d2g.report",
        "OCSMDIM2GB：扫描原生标注 {seen} 个，转换成功 {converted} 个。",
        "OCSMDIM2GB: scanned {seen} native dimensions, converted {converted}.",
    ),
    Msg::new(
        "cmd.d2g.report.leaders",
        "其中引线标注 {n} 个。",
        " of which {n} were leaders.",
    ),
    Msg::new(
        "cmd.d2g.report.marks",
        "其中智能圆心标记 {n} 个（已换成 `3中心线层` 中心线）。",
        " of which {n} were smart center marks (replaced with `3中心线层` center lines).",
    ),
    Msg::new(
        "cmd.d2g.report.skipped",
        "跳过 {n} 个：",
        " skipped {n}:",
    ),
    Msg::new("cmd.d2g.report.none_skipped", "无跳过。", " none skipped."),
    Msg::new(
        "cmd.d2g.report.undo_hint",
        "（可用一次 Ctrl+Z 全部撤销）",
        "(one Ctrl+Z undoes them all)",
    ),
    Msg::new("cmd.d2g.report.sep", "；", "; "),
    Msg::new(
        "cmd.d2g.err.unsupported_request",
        "dim2gb 收集器不支持该请求: {req}",
        "dim2gb collector does not support this request: {req}",
    ),
    Msg::new("cmd.d2g.type.aligned", "对齐标注", "aligned dimension"),
    Msg::new("cmd.d2g.type.linear", "线性标注", "linear dimension"),
    Msg::new("cmd.d2g.type.radius", "半径标注", "radius dimension"),
    Msg::new("cmd.d2g.type.diameter", "直径标注", "diameter dimension"),
    Msg::new("cmd.d2g.type.angular", "角度标注", "angular dimension"),
    Msg::new("cmd.d2g.type.arc", "弧长标注", "arc-length dimension"),
    Msg::new("cmd.d2g.type.ordinate", "坐标标注", "ordinate dimension"),
    Msg::new("cmd.d2g.type.large_radial", "折弯半径标注", "bend-radius dimension"),
    Msg::new("cmd.d2g.err.zero_radius", "半径为零", "radius is zero"),
    Msg::new(
        "cmd.d2g.err.arc_zero_radius",
        "弧长标注半径为零",
        "arc-length dimension has zero radius",
    ),
    Msg::new(
        "cmd.d2g.err.temp_arc_block",
        "临时引导弧创建失败: {e}",
        "failed to create temporary guide arc: {e}",
    ),
    Msg::new(
        "cmd.d2g.err.diameter_coincident",
        "直径两点重合",
        "the two diameter points coincide",
    ),
    Msg::new(
        "cmd.d2g.err.arc_invalid",
        "弧长标注尺寸无效（可能来自旧文件解析）",
        "invalid arc-length dimension (possibly from parsing an old file)",
    ),
    Msg::new(
        "cmd.d2g.err.ordinate_unsupported",
        "坐标标注本期不转换（OCSM 无对应构件）",
        "ordinate dimensions are not converted yet (no OCSM equivalent)",
    ),
    Msg::new(
        "cmd.d2g.err.large_radial_unsupported",
        "折弯半径本期不转换（OCSM 无对应构件）",
        "bend-radius dimensions are not converted yet (no OCSM equivalent)",
    ),
    Msg::new(
        "cmd.d2g.skip.multileader",
        "多重引线（本期不转）",
        "multileader (not converted yet)",
    ),
    Msg::new(
        "cmd.d2g.skip.non_dimension",
        "非标注对象（{kind}）",
        "non-dimension object ({kind})",
    ),
    Msg::new(
        "cmd.d2g.err.leader_vertices",
        "顶点不足（需 箭头点→拐点→肩线末端）",
        "too few vertices (need arrow point → bend point → shoulder end)",
    ),
    Msg::new(
        "cmd.d2g.err.text_lines",
        "文字 {n} 行（>2 行不转换）",
        "text has {n} lines (>2 lines not converted)",
    ),
    Msg::new(
        "cmd.d2g.err.block_create",
        "建块失败：{e}",
        "failed to create block: {e}",
    ),
    Msg::new(
        "cmd.d2g.err.add_entity",
        "加实体失败：{e}",
        "failed to add entity: {e}",
    ),
    Msg::new(
        "cmd.d2g.skip.handle_missing",
        "handle {h} 不存在",
        "handle {h} does not exist",
    ),
    Msg::new(
        "cmd.d2g.err.geometry_check",
        "几何自检未通过（原 {before} → 新 {after}）",
        "geometry self-check failed (original {before} → new {after})",
    ),
    Msg::new(
        "cmd.d2g.info.none_found",
        "OCSMDIM2GB：未找到可转换的原生标注（仅扫描模型空间）{tail}",
        "OCSMDIM2GB: no convertible native dimensions found (model space only){tail}",
    ),
    Msg::new(
        "cmd.d2g.info.none_found_tail",
        "（{list}）",
        " ({list})",
    ),
    Msg::new(
        "cmd.d2g.undo",
        "OCSMDIM2GB 原生标注转 GB",
        "OCSMDIM2GB: native dimensions → GB",
    ),
    Msg::new(
        "cmd.d2g.err.block_create_named",
        "OCSMDIM2GB：建块 {name} 失败：{e}",
        "OCSMDIM2GB: failed to create block {name}: {e}",
    ),
    Msg::new(
        "cmd.d2g.skip.dim",
        "{label}（{e}）",
        "{label} ({e})",
    ),
    Msg::new(
        "cmd.d2g.skip.leader",
        "引线标注（{e}）",
        "leader ({e})",
    ),
    Msg::new(
        "cmd.d2g.skip.mark_bad_radius",
        "圆心标记（记录里的半径无效，已跳过）",
        "center mark (invalid radius in record, skipped)",
    ),
    Msg::new("cmd.d2g.ent.point", "点", "point"),
    Msg::new("cmd.d2g.ent.line", "直线", "line"),
    Msg::new("cmd.d2g.ent.circle", "圆", "circle"),
    Msg::new("cmd.d2g.ent.arc", "圆弧", "arc"),
    Msg::new("cmd.d2g.ent.polyline", "多段线", "polyline"),
    Msg::new("cmd.d2g.ent.text", "文字", "text"),
    Msg::new("cmd.d2g.ent.mtext", "多行文字", "mtext"),
    Msg::new("cmd.d2g.ent.insert", "块参照", "block reference"),
    Msg::new("cmd.d2g.ent.dimension", "标注", "dimension"),
    Msg::new("cmd.d2g.ent.solid", "实体填充", "solid fill"),
    Msg::new("cmd.d2g.ent.other", "其它", "other"),

    // ── 命令输出/报错：明细表 BOM 系列（bom.rs / bom_xlsx.rs）───────────
    Msg::new("cmd.bom.sep", "、", ", "),
    Msg::new(
        "cmd.bom.undo.table",
        "OCSM 明细表",
        "OCSM BOM table",
    ),
    Msg::new("cmd.bom.undo.qty_lock", "OCSM 数量锁", "OCSM quantity lock"),
    Msg::new("cmd.bom.undo.export", "OCSM 明细表导出", "OCSM BOM export"),
    Msg::new("cmd.bom.undo.import", "OCSM 明细表导入", "OCSM BOM import"),
    Msg::new(
        "cmd.bom.err.template_missing",
        "OCSMBOM: 找不到明细表模板块（{head} / {row}）。请把 OCSM_BOMHEAD.dwg、OCSM_BOMROW.dwg 放进插件目录 bom/（或设 OCSM_BOM_DIR）。",
        "OCSMBOM: BOM template blocks not found ({head} / {row}). Put OCSM_BOMHEAD.dwg and OCSM_BOMROW.dwg into the plugin's bom/ directory (or set OCSM_BOM_DIR).",
    ),
    Msg::new(
        "cmd.bom.err.bad_attdefs",
        "OCSMBOM: 行块 {block} 应有 {expect} 个 ATTDEF，实际 {got} 个 → 模板不对。",
        "OCSMBOM: row block {block} should have {expect} ATTDEFs but has {got} → wrong template.",
    ),
    Msg::new(
        "cmd.bom.err.layout_first_col",
        "图框/配置不对：首列连一行都放不下（检查 sheet_top / first_col_bottom）",
        "Frame/config problem: not even one row fits in the first column (check sheet_top / first_col_bottom)",
    ),
    Msg::new(
        "cmd.bom.err.layout_cont_col",
        "图框/配置不对：续列连一行都放不下（检查 sheet_top / sheet_bottom）",
        "Frame/config problem: not even one row fits in a continuation column (check sheet_top / sheet_bottom)",
    ),
    Msg::new(
        "cmd.bom.err.layout_columns",
        "明细表需要 {cols} 列（共 {rows} 行：首列 {first} 行/上限 {cap_first}，续列每列上限 {cap_cont}），当前图幅只放得下 {fit} 列 → 请换更大图幅，或改用多页明细表。",
        "The BOM needs {cols} columns ({rows} rows total: first column {first} rows / limit {cap_first}, continuation columns limit {cap_cont}) but the current sheet fits only {fit} columns → use a larger sheet or a multi-page BOM.",
    ),
    Msg::new(
        "cmd.bom.col.desc",
        "第{col}列 {n} 行",
        "column {col}: {n} rows",
    ),
    Msg::new(
        "cmd.bom.info.built",
        "OCSMBOM: {rows} 件 → {cols} 列（{desc}）；旧表元 {removed} 个已替换（Ctrl+Z 可整体撤销）；现有行里手改过的列与 BOMLOCK 数量锁已保留。",
        "OCSMBOM: {rows} parts → {cols} columns ({desc}); {removed} old table entities replaced (Ctrl+Z undoes all); manually edited cells and BOMLOCK quantity locks were kept.",
    ),
    Msg::new(
        "cmd.bom.info.count_hint",
        "OCSMBOM: 提示——数量按「图中插入件数」统计，同一零件画在多个视图里会重复计数。",
        "OCSMBOM: note — quantities count inserted occurrences in the drawing, so the same part drawn in several views is counted more than once.",
    ),
    Msg::new(
        "cmd.bom.info.untracked",
        "OCSMBOM: 注意——图中有 {n} 个 OCSM_ 零件块引用但没有 OCSM_PART 台账记录（多为离线生成/早期版本的文件），它们不会进明细表。用 XL 重新放置，或后续用表格导入补录。",
        "OCSMBOM: warning — {n} OCSM_ part block references have no OCSM_PART ledger record (usually from offline/older files); they will not appear in the BOM. Re-place them with XL, or add them later via table import.",
    ),
    Msg::new(
        "cmd.bom.err.sync_nothing",
        "OCSMBOMSYNC: 没有可排的内容（图上没有序号球标，也没有零件台账）。",
        "OCSMBOMSYNC: nothing to list (no balloon items and no part ledger in the drawing).",
    ),
    Msg::new(
        "cmd.bom.err.sync_failed",
        "OCSMBOMSYNC: {e}",
        "OCSMBOMSYNC: {e}",
    ),
    Msg::new(
        "cmd.bom.info.sync",
        "OCSMBOMSYNC: {rows} 行（序号 {nos}）→ {cols} 列；旧表元 {removed} 个已替换。",
        "OCSMBOMSYNC: {rows} rows (item numbers {nos}) → {cols} columns; {removed} old table entities replaced.",
    ),
    Msg::new(
        "cmd.bom.locked.item",
        "{no}（数量 {qty}）",
        "{no} (qty {qty})",
    ),
    Msg::new(
        "cmd.bom.info.sync_locked",
        "OCSMBOMSYNC: {n} 行数量已锁定，未覆盖：{list}。要重算用 `BOMLOCK <序号> off`。",
        "OCSMBOMSYNC: quantity locked on {n} rows, not overwritten: {list}. Use `BOMLOCK <item no> off` to unlock.",
    ),
    Msg::new(
        "cmd.bom.info.squeezed",
        "OCSMBOM: {n} 格文字超宽 → 已自动横向压缩（字高统一不动；压太扁的另点名）。",
        "OCSMBOM: {n} cells were too wide → compressed horizontally (text height unchanged; badly flattened ones are listed separately).",
    ),
    Msg::new(
        "cmd.bom.info.squeezed_hard",
        "OCSMBOM: 下面 {n} 格横向压得偏扁（已压进格内、不会到邻格，但可读性下降）：{list}（建议加宽该列 / 缩短文本 / 把标准号写进名称列）。",
        "OCSMBOM: {n} cells were flattened hard (still inside their cells but less readable): {list} (widen the column, shorten the text, or put the standard number in the name column).",
    ),
    Msg::new(
        "cmd.bom.squeezed_hard.item",
        "{tag}「{value}」{pct}%",
        "{tag} \"{value}\" {pct}%",
    ),
    Msg::new(
        "cmd.bom.lock.usage",
        "OCSMBOMLOCK / BOMLOCK 用法：BOMLOCK <序号|图号> [数量|off]（不给数量 = 按行上现值锁；off = 解锁）",
        "OCSMBOMLOCK / BOMLOCK usage: BOMLOCK <item no|drawing no> [qty|off] (no qty = lock at the current value; off = unlock)",
    ),
    Msg::new(
        "cmd.bom.lock.err.row_missing",
        "OCSMBOMLOCK: 表里找不到序号/图号为「{key}」的行。",
        "OCSMBOMLOCK: no row with item/drawing number \"{key}\" in the table.",
    ),
    Msg::new(
        "cmd.bom.lock.info.unlocked",
        "OCSMBOMLOCK: 序号 {key}→已解锁（数量 {qty}）——下次同步会按件数/引用次数重算。",
        "OCSMBOMLOCK: {key} → unlocked (qty {qty}); the next sync will recompute it from part counts/references.",
    ),
    Msg::new(
        "cmd.bom.lock.err.qty_invalid",
        "OCSMBOMLOCK: 数量「{v}」不是正整数（或写 off 解锁）。",
        "OCSMBOMLOCK: quantity \"{v}\" is not a positive integer (or write off to unlock).",
    ),
    Msg::new(
        "cmd.bom.lock.info.locked",
        "OCSMBOMLOCK: 序号 {key} 数量锁定为 {qty}（同步不再重算；BOMLOCK {key} off 可解锁）。",
        "OCSMBOMLOCK: {key} quantity locked to {qty} (sync will not recompute; BOMLOCK {key} off unlocks).",
    ),
    Msg::new(
        "cmd.bom.err.row_no_empty",
        "有行的序号为空（每行都需要序号）",
        "A row has an empty item number (every row needs one)",
    ),
    Msg::new(
        "cmd.bom.err.row_qty_not_positive_int",
        "序号 {no} 的数量「{qty}」不是正整数",
        "Row {no}: quantity \"{qty}\" is not a positive integer",
    ),
    Msg::new(
        "cmd.bom.err.row_qty_zero",
        "序号 {no} 的数量不能为 0（要删行就删行）",
        "Row {no}: quantity cannot be 0 (delete the row to remove it)",
    ),
    Msg::new(
        "cmd.bom.info.links_stamped",
        "OCSMBOM: 已把 {changed} 个表块链接指向明细表编辑页（Ctrl+点击打开；或运行 `BOMEDIT` 在新窗口打开）。",
        "OCSMBOM: {changed} table blocks now link to the BOM edit page (Ctrl+click, or run `BOMEDIT`).",
    ),
    Msg::new(
        "cmd.bom.err.export_empty",
        "OCSMBOMXLSX: 图上还没有明细表行（先 `BOM` 或 `BOMSYNC` 建表）。",
        "OCSMBOMXLSX: the drawing has no BOM rows yet (run `BOM` or `BOMSYNC` first).",
    ),
    Msg::new(
        "cmd.bom.err.write_failed",
        "写 {path} 失败: {e}",
        "Failed to write {path}: {e}",
    ),
    Msg::new(
        "cmd.bom.err.export_failed",
        "OCSMBOMXLSX: {e}",
        "OCSMBOMXLSX: {e}",
    ),
    Msg::new(
        "cmd.bom.info.export_temp_hint",
        "OCSMBOMXLSX: 提示——本图还没存过盘，所以文件落在**临时目录**（{dir}）。临时目录会被系统清理，要长期保存请先 Ctrl+S 存盘再导出（xlsx 就会生成在图纸同目录、同名-明细表.xlsx）。",
        "OCSMBOMXLSX: note — the drawing has not been saved, so the file went to a **temporary directory** ({dir}). Temp files get cleaned up; for long-term storage press Ctrl+S first, then the xlsx is created next to the drawing as <same name>-明细表.xlsx.",
    ),
    Msg::new(
        "cmd.bom.info.exported",
        "OCSMBOMXLSX: {rows} 行已导出 → {path}（{n} 行记下导出基线；表块已挂编辑页链接，Ctrl+点击打开网页）。",
        "OCSMBOMXLSX: {rows} rows exported → {path} ({n} rows recorded as export baseline; table blocks link to the edit page, Ctrl+click to open).",
    ),
    Msg::new(
        "cmd.bom.info.export_columns",
        "OCSMBOMXLSX: 列 = {cols}（「锁定数量」只在此文件里，不进图纸表格；空=不锁，Y/是=锁为同行数量，数字=锁为该数）。",
        "OCSMBOMXLSX: columns = {cols} (the \"locked quantity\" column exists only in this file, not in the drawing table; empty = unlocked, Y/是 = lock at the row quantity, a number = lock to that value).",
    ),
    Msg::new(
        "cmd.bom.err.import_missing",
        "OCSMBOMXLSXI: 找不到文件 {path}（先 `BOMXLSX` 导出，或给个路径）。",
        "OCSMBOMXLSXI: file not found: {path} (run `BOMXLSX` first, or give a path).",
    ),
    Msg::new(
        "cmd.bom.err.import_csv",
        "OCSMBOMXLSXI: 读 CSV 失败：{e}",
        "OCSMBOMXLSXI: failed to read CSV: {e}",
    ),
    Msg::new(
        "cmd.bom.err.import_empty",
        "OCSMBOMXLSXI: 文件里没有数据行。",
        "OCSMBOMXLSXI: the file has no data rows.",
    ),
    Msg::new(
        "cmd.bom.err.import_failed",
        "OCSMBOMXLSXI: {e}",
        "OCSMBOMXLSXI: {e}",
    ),
    Msg::new(
        "cmd.bom.info.imported",
        "OCSMBOMXLSXI: 从 {path} 导入 {n} 行 → 表 {rows} 行 / {cols} 列（{locked} 行数量已锁定，{kept} 行文件里没有、按图纸保留）。",
        "OCSMBOMXLSXI: imported {n} rows from {path} → {rows} rows / {cols} columns ({locked} rows keep quantity locks; {kept} rows absent from the file were kept from the drawing).",
    ),
    Msg::new(
        "cmd.bom.cfg.info.set",
        "OCSMBOMCFG: 首列行数已设为 {n}（{path}）。",
        "OCSMBOMCFG: first-column row count set to {n} ({path}).",
    ),
    Msg::new(
        "cmd.bom.cfg.err.save",
        "OCSMBOMCFG: 写配置失败：{e}",
        "OCSMBOMCFG: failed to save config: {e}",
    ),
    Msg::new(
        "cmd.bom.cfg.info.show",
        "OCSMBOMCFG: 首列行数 {n}（配置 {path}）；用法：BOMCFG 30 改默认，BOM 30 只改本次。",
        "OCSMBOMCFG: first-column row count {n} (config {path}); usage: BOMCFG 30 changes the default, BOM 30 changes only this run.",
    ),
    Msg::new(
        "cmd.bomxlsx.err.too_many_cols",
        "第 {row} 行有 {have} 列，超过 {max} 列（多出来的列无法识别）",
        "Row {row} has {have} columns, more than the {max} recognized (extra columns are ignored)",
    ),
    Msg::new(
        "cmd.bomxlsx.err.compress",
        "压缩 {name} 失败: {e}",
        "Failed to compress {name}: {e}",
    ),
    Msg::new(
        "cmd.bomxlsx.err.no_eocd",
        "不是有效的 xlsx/zip：找不到 EOCD",
        "Not a valid xlsx/zip: EOCD not found",
    ),
    Msg::new(
        "cmd.bomxlsx.err.zip_central_dir",
        "zip 中央目录损坏",
        "corrupt zip central directory",
    ),
    Msg::new(
        "cmd.bomxlsx.err.zip_local_header",
        "zip 条目 {name} 局部头损坏",
        "zip entry {name} has a corrupt local header",
    ),
    Msg::new(
        "cmd.bomxlsx.err.zip_out_of_bounds",
        "zip 条目 {name} 数据越界",
        "zip entry {name} data out of bounds",
    ),
    Msg::new(
        "cmd.bomxlsx.err.unzip",
        "解压 {name} 失败: {e}",
        "Failed to decompress {name}: {e}",
    ),
    Msg::new(
        "cmd.bomxlsx.err.unsupported_method",
        "zip 条目 {name} 用了不支持的压缩方法 {m}",
        "zip entry {name} uses unsupported compression method {m}",
    ),
    Msg::new(
        "cmd.bomxlsx.err.make_dir",
        "建目录失败: {e}",
        "Failed to create directory: {e}",
    ),
    Msg::new(
        "cmd.bomxlsx.err.parse_sheet",
        "解析 sheet 失败: {e}",
        "Failed to parse sheet: {e}",
    ),
    Msg::new(
        "cmd.bomxlsx.err.read_file",
        "读 {path} 失败: {e}",
        "Failed to read {path}: {e}",
    ),
    Msg::new(
        "cmd.bomxlsx.err.no_sheet",
        "xlsx 里找不到工作表",
        "no worksheet found in the xlsx",
    ),
    Msg::new(
        "cmd.bomxlsx.err.sheet_read",
        "工作表读取失败",
        "failed to read the worksheet",
    ),

    // ── 命令输出/报错：标准件库 / 结构要素（OCSMPART / XL；lib.rs + partgen*.rs + guide_server）──
    Msg::new(
        "cmd.parts.usage",
        "OCSMPART 参数无效。用法：标准件 `OCSMPART <族> <d> <l> [view <视图>] [at x,y] [rot 度]`（例：OCSMPART hex_bolt_c 10 95 at 150,30 rot 0）；结构要素 `OCSMPART detail_grind_od <d> [b1 <值>] [at x,y] [rot 度]`（b1 缺省 = 该 d 档默认行）；矩形花键 `OCSMPART detail_spline_rect <规格代号> L<满齿段长> [de <滚刀外径>] [view front|side|section]`（例：OCSMPART detail_spline_rect 6x23x26x6 L30 view side）；外螺纹退刀槽 `OCSMPART detail_thread_relief <d> P <螺距> [g1 值 g2 值 dg 值 r 值 alpha 值] [at x,y] [rot 度]`（P 必给）；毂槽 `OCSMPART detail_hub_keyway <d> [len 毂长] [view main|side]`（例：OCSMPART detail_hub_keyway 25 len 30 view main；b/t₂/r 由 d 查表，len 缺省 30）；平键 `OCSMPART key_1096_{a|b|c} <b> <L> [view main|top|section]`、`OCSMPART key_1097_{a|b} <b> <L> [view main|top]`（例：OCSMPART key_1096_a 4 8、OCSMPART key_1097_a 8 25；L 省略/0 = 该档默认，L 须 ∈ 标准系列且 L<10b，1097 的 L1/L2/L3 由 L 查长度系列表派生）；不带参数则打开零件库窗口。",
        "Invalid OCSMPART arguments. Usage: standard part `OCSMPART <family> <d> <l> [view <view>] [at x,y] [rot deg]` (e.g. OCSMPART hex_bolt_c 10 95 at 150,30 rot 0); feature `OCSMPART detail_grind_od <d> [b1 <value>] [at x,y] [rot deg]` (b1 omitted = the default row for that d); rectangular spline `OCSMPART detail_spline_rect <spec code> L<full-tooth length> [de <hob OD>] [view front|side|section]` (e.g. OCSMPART detail_spline_rect 6x23x26x6 L30 view side); external thread relief `OCSMPART detail_thread_relief <d> P <pitch> [g1 v g2 v dg v r v alpha v] [at x,y] [rot deg]` (P required); hub keyway `OCSMPART detail_hub_keyway <d> [len hub length] [view main|side]` (e.g. OCSMPART detail_hub_keyway 25 len 30 view main; b/t₂/r looked up from d, len defaults to 30); parallel keys `OCSMPART key_1096_{a|b|c} <b> <L> [view main|top|section]`, `OCSMPART key_1097_{a|b} <b> <L> [view main|top]` (e.g. OCSMPART key_1096_a 4 8, OCSMPART key_1097_a 8 25; L omitted/0 = default for that b, L must be a standard series value with L<10b, and key_1097 derives L1/L2/L3 from the length series table); no arguments opens the parts library window.",
    ),
    Msg::new(
        "cmd.parts.window_opened",
        "OCSM 标准件库：已打开零件库窗口。选零件点「零件出库」→ 回到图纸点击定位基点 → 移动光标旋转 → 再点击落定（可连续，Esc 结束）。",
        "OCSM parts library: window opened. Pick a part and click \"Produce part\" → back in the drawing click to set the base point → move the cursor to rotate → click to place (repeatable; Esc to finish).",
    ),
    Msg::new(
        "cmd.parts.window_exists",
        "OCSM 标准件库：零件库窗口已打开（Alt+Tab 切换过去）。",
        "OCSM parts library: the window is already open (Alt+Tab to switch to it).",
    ),
    Msg::new(
        "cmd.parts.err.no_service",
        "OCSM: 无法启动零件库服务（宿主不支持 worker 请求）。",
        "OCSM: cannot start the parts library service (the host does not support worker requests).",
    ),
    Msg::new(
        "cmd.parts.err.no_worker",
        "OCSM 标准件：宿主不支持 worker 请求，无法参数化插入。",
        "OCSM parts: the host does not support worker requests; parametric insert is unavailable.",
    ),
    Msg::new("cmd.parts.inserted_prefix", "OCSM 标准件：{msg}", "OCSM parts: {msg}"),
    Msg::new(
        "cmd.parts.err.insert_failed",
        "OCSM 标准件插入失败：{e}",
        "OCSM part insert failed: {e}",
    ),
    Msg::new("cmd.parts.undo_insert", "零件插入", "insert part"),
    Msg::new(
        "cmd.parts.inserted_item",
        "已插入 {name} {spec}（{code}）",
        "inserted {name} {spec} ({code})",
    ),
    Msg::new("cmd.req.err.bad_json", "请求 JSON 无效: {e}", "Invalid request JSON: {e}"),
    Msg::new("cmd.parts.prompt.what", "OCSM 标准件", "OCSM part"),
    Msg::new(
        "cmd.parts.prompt.where",
        "请在零件库窗口里点「零件出库」",
        "click \"Produce part\" in the parts library window",
    ),
    Msg::new(
        "cmd.parts.err.no_m",
        "{table} 数据表里没有 M{d}",
        "{table} data table has no M{d}",
    ),
    Msg::new(
        "cmd.parts.err.no_dia",
        "{table} 数据表里没有 Ø{d}",
        "{table} data table has no Ø{d}",
    ),
    Msg::new(
        "cmd.parts.err.len_range_std",
        "M{d} 的长度应在 {lo}~{hi}（标准范围）",
        "M{d} length must be within {lo}~{hi} (standard range)",
    ),
    Msg::new(
        "cmd.parts.err.len_range",
        "M{d} 的长度应在 {lo}~{hi} 之间",
        "M{d} length must be between {lo} and {hi}",
    ),
    Msg::new(
        "cmd.parts.err.len_range_ab",
        "M{d} 的长度应在 {lo}~{hi} 之间（GB/T 5782 A/B级）",
        "M{d} length must be between {lo} and {hi} (GB/T 5782 grade A/B)",
    ),
    Msg::new(
        "cmd.parts.err.len_range_series",
        "{std} Ø{d} 的长度应在 {lo} ~ {hi} 之间（表内系列：{series}）",
        "{std} Ø{d} length must be between {lo} and {hi} (in-table series: {series})",
    ),
    Msg::new(
        "cmd.parts.err.len_range_kit",
        "M{d} 的长度范围是 {lo}…{hi}（收到 {l}）",
        "M{d} length range is {lo}…{hi} (got {l})",
    ),
    Msg::new(
        "cmd.parts.err.view_not_offered",
        "{family} 不提供视图 {view}（可用：{avail}）",
        "{family} does not provide view {view} (available: {avail})",
    ),
    Msg::new(
        "cmd.parts.err.view_absent",
        "{family} 没有视图 {view}（可用：{avail}）",
        "{family} has no view {view} (available: {avail})",
    ),
    Msg::new(
        "cmd.parts.err.view_absent_bare",
        "{family} 没有视图 {view}",
        "{family} has no view {view}",
    ),
    Msg::new(
        "cmd.parts.err.only_main",
        "{family} 只有主视图（模板只有这一个视图）",
        "{family} only has the main view (the template has just this one)",
    ),
    Msg::new(
        "cmd.parts.err.only_main_received",
        "{family} 只有主视图（模板只有这一个视图），收到 {view}",
        "{family} only has the main view (the template has just this one); got {view}",
    ),
    Msg::new("cmd.parts.err.unknown_family", "未知族 {other}", "unknown family {other}"),
    Msg::new(
        "cmd.parts.err.unknown_nut_family",
        "未知螺母族 {other}",
        "unknown nut family {other}",
    ),
    Msg::new(
        "cmd.parts.err.family_todo",
        "零件族 {other} 尚未实现",
        "part family {other} is not implemented yet",
    ),
    Msg::new(
        "cmd.parts.err.nut6170_todo",
        "1型六角螺母 GB/T 6170-2015 暂未提供（画法待模板确认，数据表已备好）",
        "Hex nut style 1 GB/T 6170-2015 is not available yet (drawing method pending template confirmation; the data table is ready)",
    ),
    Msg::new(
        "cmd.parts.err.key_no_b",
        "{table} 数据表里没有 b={b}",
        "{table} data table has no b={b}",
    ),
    Msg::new(
        "cmd.parts.err.key_l_positive",
        "GB/T {std} b={b}：L 必须是正数（收到 {l}）",
        "GB/T {std} b={b}: L must be a positive number (got {l})",
    ),
    Msg::new(
        "cmd.parts.err.key_l_range",
        "GB/T {std} b={b} 的 L 取值范围是 {lo}…{hi}（L 系列值且 L<10b），收到 {l}",
        "GB/T {std} b={b}: L must be within {lo}…{hi} (a series value with L<10b); got {l}",
    ),
    Msg::new(
        "cmd.parts.err.key_l_range_1097",
        "GB/T 1097 b={b} 的 L 取值范围是 {lo}…{hi}（L 系列值且 L<10b，GB/T 1097-2003 注③），收到 {l}",
        "GB/T 1097 b={b}: L must be within {lo}…{hi} (a series value with L<10b, GB/T 1097-2003 note 3); got {l}",
    ),
    Msg::new(
        "cmd.parts.err.key_l_series",
        "GB/T 1096 b={b} 的 L 须取标准系列值（6, 8, 10, …, 400；L<10b），收到 {l}",
        "GB/T 1096 b={b}: L must be a standard series value (6, 8, 10, …, 400; L<10b); got {l}",
    ),
    Msg::new(
        "cmd.parts.err.key_series_missing",
        "GB/T 1097-2003 长度系列表里没有 L={l}（不插值/不外推）；可选 L（{n} 档）：{list}",
        "GB/T 1097-2003 length series has no L={l} (no interpolation/extrapolation); available L ({n} entries): {list}",
    ),
    Msg::new(
        "cmd.parts.err.key_chain",
        "GB/T 1097-2003 长度系列表 L={l} 尺寸链不一致：L1={l1}、L2={l2}、L3={l3}（应满足 L2=L/2、L1+2L3=L）",
        "GB/T 1097-2003 length series L={l} dimension chain inconsistent: L1={l1}, L2={l2}, L3={l3} (expected L2=L/2 and L1+2L3=L)",
    ),
    Msg::new(
        "cmd.parts.err.corner_b",
        "b={b} 不在倒角档位表内",
        "b={b} is not in the chamfer table",
    ),
    Msg::new(
        "cmd.parts.err.key1097_no_c",
        "GB/T 1097 没有 C 型",
        "GB/T 1097 has no type C",
    ),
    Msg::new(
        "cmd.parts.err.json_parse_keys",
        "平键数据表 JSON 解析失败",
        "failed to parse the parallel-key data table JSON",
    ),
    Msg::new(
        "cmd.parts.err.json_parse_parts",
        "零件数据表 JSON 解析失败",
        "failed to parse the part data table JSON",
    ),
    Msg::new(
        "cmd.parts.err.missing_param",
        "缺少参数 {name}",
        "missing parameter {name}",
    ),
    Msg::new(
        "cmd.parts.err.param_not_number",
        "{name} 不是数字",
        "{name} is not a number",
    ),
    Msg::new(
        "cmd.parts.err.thread_len",
        "M{d} 的螺纹长度 l={want}（收到 {got}）",
        "M{d} thread length l={want} (got {got})",
    ),
    Msg::new(
        "cmd.parts.err.no_d1_od",
        "GB/T 13871.1 数据表里没有 d1={d1} D={od}",
        "GB/T 13871.1 data table has no d1={d1} D={od}",
    ),
    Msg::new(
        "cmd.parts.err.no_d_bearing",
        "GB/T 276 数据表里没有 内径 d={d} 宽度 B={b}",
        "GB/T 276 data table has no bore d={d} width B={b}",
    ),
    Msg::new(
        "cmd.parts.err.no_dt",
        "{table} 数据表里没有 d={d} T={t} 的规格",
        "{table} data table has no spec with d={d} T={t}",
    ),
    Msg::new(
        "cmd.parts.err.no_db",
        "{table} 数据表里没有 d={d} B={b} 的规格",
        "{table} data table has no spec with d={d} B={b}",
    ),
    Msg::new(
        "cmd.parts.err.view_absent_no_family",
        "没有视图 {view}（可用：{avail}）",
        "no view {view} (available: {avail})",
    ),

    // ── 命令输出/报错：表面粗糙度（OCSMRGH / CC；guide_server::apply_roughness）──
    Msg::new("cmd.rough.undo_insert", "表面粗糙度", "roughness symbol"),
    Msg::new(
        "cmd.rough.prompt",
        "OCSM 表面粗糙度：指定符号插入点。",
        "OCSM surface roughness: specify the symbol insertion point.",
    ),
    Msg::new(
        "cmd.rough.err.bad_base",
        "无效的基础体 {base}（应为 C1..C4）",
        "invalid base symbol {base} (expected C1..C4)",
    ),
    Msg::new(
        "cmd.rough.err.bad_extra",
        "无效的附加区 {extra}（应为 R1..R5）",
        "invalid additional area {extra} (expected R1..R5)",
    ),

    // ── 命令输出/报错：OCSMCARD 卡类型分派（guide_server / card_report 同句）──
    Msg::new(
        "cmd.card.err.unknown_type_usage",
        "智能卡片：不认识的卡类型「{name}」。\n{usage}",
        "Smart card: unknown card type \"{name}\".\n{usage}",
    ),
    Msg::new(
        "cmd.card.err.unknown_type",
        "智能卡片：不认识的卡类型「{card_id}」（卡类型表见 /api/spline_options 的 card_types）",
        "Smart card: unknown card type \"{card_id}\" (see card_types under /api/spline_options)",
    ),
    Msg::new(
        "cmd.card.err.bad_model_spline",
        "花键参数表：请求字段无效：{e}",
        "Spline table: invalid request field: {e}",
    ),
    Msg::new(
        "cmd.card.err.bad_model_spline_lite",
        "GB 花键精简卡：请求字段无效：{e}",
        "GB spline lite card: invalid request field: {e}",
    ),
    Msg::new(
        "cmd.card.err.bad_model_gear",
        "齿轮参数表：请求字段无效：{e}",
        "Gear table: invalid request field: {e}",
    ),
    Msg::new(
        "cmd.card.err.bad_model_ansi",
        "ANSI 花键参数表：请求字段无效：{e}",
        "ANSI spline table: invalid request field: {e}",
    ),
    Msg::new(
        "cmd.card.err.bad_model_nf",
        "NF 内花键参数表：请求字段无效：{e}",
        "NF internal spline table: invalid request field: {e}",
    ),
    Msg::new(
        "cmd.card.err.bad_model_nf_ext",
        "NF 外花键参数表：请求字段无效：{e}",
        "NF external spline table: invalid request field: {e}",
    ),
    Msg::new(
        "cmd.card.err.bad_model_din",
        "DIN 花键参数表：请求字段无效：{e}",
        "DIN spline table: invalid request field: {e}",
    ),
    Msg::new(
        "cmd.card.err.no_lite_def",
        "精简卡：卡类型「{id}」没有精简定义",
        "Lite card: card type \"{id}\" has no lite definition",
    ),

    // ── 命令输出/报错：OCSMCARD 命令实现（lib.rs）──
    Msg::new(
        "cmd.card.err.no_worker",
        "OCSMCARD: 无法启动智能卡片服务（宿主不支持 worker 请求）。",
        "OCSMCARD: cannot start the smart-card service (host does not support worker requests).",
    ),
    Msg::new(
        "cmd.card.info.opened",
        "OCSM 智能卡片：已打开窗口（22 张卡，一卡一方向：GB 花键内/外、齿轮、ANSI 内/外×中/英、NF 内/外、DIN 内/外；下拉「精简版」分组 = GB/NF/DIN/ANSI×4/齿轮共 11 张只列基本参数+主要测量量的卡）。齿形表达式反解 + 实时结果。点「出表」→ 回到图纸点击定位基点 → 移动光标旋转 → 再点击落定（可连续，Esc 结束）。",
        "OCSM smart card: window opened (22 cards, one direction each: GB spline int/ext, gear, ANSI int/ext × CN/EN, NF int/ext, DIN int/ext; the \"Lite\" group = 11 cards listing only basic parameters plus main measurements for GB/NF/DIN/ANSI×4/gear). Tooth-profile expression resolution + live results. Click \"Export\" → click in the drawing to set the base point → move to rotate → click again to place (repeatable; Esc to finish).",
    ),
    Msg::new(
        "cmd.card.info.already_open",
        "OCSM 智能卡片：窗口已打开（Alt+Tab 切换过去）。",
        "OCSM smart card: window is already open (switch to it with Alt+Tab).",
    ),
    Msg::new("cmd.card.place.what", "OCSM 智能卡片", "OCSM smart card"),
    Msg::new(
        "cmd.card.place.where",
        "请在智能卡片窗口里点「出表」",
        "click \"Export\" in the smart card window",
    ),
    Msg::new(
        "cmd.card.xlt.removed",
        "`XLT` 已移除：请改用 `OCSMCARD 花键参数表 …`（智能卡片）。",
        "`XLT` was removed: use `OCSMCARD 花键参数表 …` (smart card) instead.",
    ),
    Msg::new(
        "cmd.card.err.spline_prefix",
        "花键参数表：{e}",
        "Spline table: {e}",
    ),
    Msg::new(
        "cmd.card.err.spline_block",
        "花键参数表：建块 {block} 失败：{e}",
        "Spline table: failed to create block {block}: {e}",
    ),
    Msg::new(
        "cmd.card.err.spline_no_handle",
        "花键参数表：插入失败（宿主未返回句柄）",
        "Spline table: insert failed (host returned no handle)",
    ),
    Msg::new("cmd.card.undo.spline", "花键参数表插入", "spline table insert"),
    Msg::new("cmd.card.side.internal", "内花键", "internal spline"),
    Msg::new("cmd.card.side.external", "外花键", "external spline"),
    Msg::new(
        "cmd.card.info.spline_inserted",
        "智能卡片：已插入{kind}参数表（{grade}，体系 {system}，表达式 {expr}）于 ({x}, {y}) rot {rot}°{dp_note}。",
        "Smart card: inserted {kind} parameter table ({grade}, system {system}, expression {expr}) at ({x}, {y}) rot {rot}°{dp_note}.",
    ),
    Msg::new(
        "cmd.card.dp_note.calc",
        "；量棒 Dp={dp}（计算值 D'={dp_calc}，备选 {c1}/ {c2}/ {c3}）",
        "; pin Dp={dp} (calculated D'={dp_calc}, candidates {c1}/ {c2}/ {c3})",
    ),
    Msg::new(
        "cmd.card.dp_note.manual",
        "；量棒 Dp={dp}（按所填手算，Md 已重算）",
        "; pin Dp={dp} (manual entry; Md recomputed)",
    ),
    Msg::new(
        "cmd.card.err.lite_prefix",
        "GB 花键精简卡：{e}",
        "GB spline lite card: {e}",
    ),
    Msg::new(
        "cmd.card.err.lite_block",
        "GB 花键精简卡：建块 {block} 失败：{e}",
        "GB spline lite card: failed to create block {block}: {e}",
    ),
    Msg::new(
        "cmd.card.err.lite_no_handle",
        "GB 花键精简卡：插入失败（宿主未返回句柄）",
        "GB spline lite card: insert failed (host returned no handle)",
    ),
    Msg::new(
        "cmd.card.undo.lite",
        "GB 花键精简卡插入",
        "GB spline lite card insert",
    ),
    Msg::new(
        "cmd.card.info.lite_inserted",
        "智能卡片：已插入{kind}精简卡（{grade}，表达式 {expr}）于 ({x}, {y}) rot {rot}°。",
        "Smart card: inserted {kind} lite card ({grade}, expression {expr}) at ({x}, {y}) rot {rot}°.",
    ),
    Msg::new(
        "cmd.card.err.lite_card_prefix",
        "{id}：{e}",
        "{id}: {e}",
    ),
    Msg::new(
        "cmd.card.err.lite_card_block",
        "{id}：建块 {block} 失败：{e}",
        "{id}: failed to create block {block}: {e}",
    ),
    Msg::new(
        "cmd.card.err.lite_card_no_handle",
        "{id}：插入失败（宿主未返回句柄）",
        "{id}: insert failed (host returned no handle)",
    ),
    Msg::new("cmd.card.undo.lite_card", "{label}插入", "insert {label}"),
    Msg::new(
        "cmd.card.info.lite_card_inserted",
        "智能卡片：已插入{label}（{n} 个精简项，不含公差列）于 ({x}, {y}) rot {rot}°。",
        "Smart card: inserted {label} ({n} lite items, no tolerance columns) at ({x}, {y}) rot {rot}°.",
    ),
    Msg::new(
        "cmd.card.gear.err_block",
        "齿轮参数表：建块 {block} 失败：{e}",
        "Gear table: failed to create block {block}: {e}",
    ),
    Msg::new("cmd.card.gear.err_prefix", "齿轮参数表：{e}", "Gear table: {e}"),
    Msg::new("cmd.card.gear.undo", "齿轮参数表插入", "gear table insert"),
    Msg::new(
        "cmd.card.gear.err_no_handle",
        "齿轮参数表：插入失败（宿主未返回句柄）",
        "Gear table: insert failed (host returned no handle)",
    ),
    Msg::new(
        "cmd.card.gear.info",
        "智能卡片：已插入齿轮参数表（{note}）于 ({x}, {y}) rot {rot}°。\nGB/T 10095-88 公差（Fr/FW/ff/fpt/Fβ）与中心距极限偏差本仓未收 —— 表内显示「—」。",
        "Smart card: inserted gear table ({note}) at ({x}, {y}) rot {rot}°.\nGB/T 10095-88 tolerances (Fr/FW/ff/fpt/Fβ) and center-distance limit deviations are not collected in this repo — shown as \"—\" in the table.",
    ),
    Msg::new(
        "cmd.card.ansi.err_block",
        "ANSI 花键参数表：建块 {block} 失败：{e}",
        "ANSI spline table: failed to create block {block}: {e}",
    ),
    Msg::new(
        "cmd.card.ansi.err_prefix",
        "ANSI 花键参数表：{e}",
        "ANSI spline table: {e}",
    ),
    Msg::new(
        "cmd.card.ansi.undo",
        "ANSI 花键参数表插入",
        "ANSI spline table insert",
    ),
    Msg::new(
        "cmd.card.ansi.err_no_handle",
        "ANSI 花键参数表：插入失败（宿主未返回句柄）",
        "ANSI spline table: insert failed (host returned no handle)",
    ),
    Msg::new(
        "cmd.card.ansi.info",
        "智能卡片：已插入 ANSI B92.1 {side}参数表（{lang}，P/Ps={pair}，N={n}）于 ({x}, {y}) rot {rot}°。\n配合/公差/量棒/公法线表本仓未收 —— 相关格显示「—」。",
        "Smart card: inserted ANSI B92.1 {side} table ({lang}, P/Ps={pair}, N={n}) at ({x}, {y}) rot {rot}°.\nFit/tolerance/pin/base-tangent tables are not collected in this repo — the related cells show \"—\".",
    ),
    Msg::new(
        "cmd.card.nf.err_block",
        "NF 内花键参数表：建块 {block} 失败：{e}",
        "NF internal spline table: failed to create block {block}: {e}",
    ),
    Msg::new(
        "cmd.card.nf.err_prefix",
        "NF 内花键参数表：{e}",
        "NF internal spline table: {e}",
    ),
    Msg::new("cmd.card.nf.undo", "NF 内花键参数表插入", "NF internal spline table insert"),
    Msg::new(
        "cmd.card.nf.err_no_handle",
        "NF 内花键参数表：插入失败（宿主未返回句柄）",
        "NF internal spline table: insert failed (host returned no handle)",
    ),
    Msg::new(
        "cmd.card.nf.info",
        "智能卡片：已插入{note}于 ({x}, {y}) rot {rot}°。\n公差按 p28（大径 R7 / 小径 H7，ISO 286）+ p29（跨棒距 = 内花键 E 偏差）；表外 V/G/ri 与 p29 表外偏差显示「—」。",
        "Smart card: inserted {note} at ({x}, {y}) rot {rot}°.\nTolerances follow p28 (major R7 / minor H7, ISO 286) + p29 (span = internal-spline E deviation); off-table V/G/ri and off-table p29 deviations show \"—\".",
    ),
    Msg::new(
        "cmd.card.nfext.err_block",
        "NF 外花键参数表：建块 {block} 失败：{e}",
        "NF external spline table: failed to create block {block}: {e}",
    ),
    Msg::new(
        "cmd.card.nfext.err_prefix",
        "NF 外花键参数表：{e}",
        "NF external spline table: {e}",
    ),
    Msg::new(
        "cmd.card.nfext.undo",
        "NF 外花键参数表插入",
        "NF external spline table insert",
    ),
    Msg::new(
        "cmd.card.nfext.err_no_handle",
        "NF 外花键参数表：插入失败（宿主未返回句柄）",
        "NF external spline table: insert failed (host returned no handle)",
    ),
    Msg::new(
        "cmd.card.nfext.info",
        "智能卡片：已插入{note}于 ({x}, {y}) rot {rot}°。\n公差按模板实测口径（大径 h12 / 小径 H7，ISO 286）+ p29（公法线 = 外花键 E 偏差，按配合）；K/W 与 p29 表外偏差显示「—」。",
        "Smart card: inserted {note} at ({x}, {y}) rot {rot}°.\nTolerances follow the template-measured convention (major h12 / minor H7, ISO 286) + p29 (base tangent = external-spline E deviation, per fit); off-table K/W and p29 deviations show \"—\".",
    ),
    Msg::new(
        "cmd.card.din.err_block",
        "DIN 花键参数表：建块 {block} 失败：{e}",
        "DIN spline table: failed to create block {block}: {e}",
    ),
    Msg::new(
        "cmd.card.din.err_prefix",
        "DIN 花键参数表：{e}",
        "DIN spline table: {e}",
    ),
    Msg::new("cmd.card.din.undo", "DIN 花键参数表插入", "DIN spline table insert"),
    Msg::new(
        "cmd.card.din.err_no_handle",
        "DIN 花键参数表：插入失败（宿主未返回句柄）",
        "DIN spline table: insert failed (host returned no handle)",
    ),
    Msg::new(
        "cmd.card.din.info",
        "智能卡片：已插入{note}于 ({x}, {y}) rot {rot}°（整表缩放 0.17）。\nTable 7 缺口（>400 侧偏差列、≤12 细档、非 6–9 级公差、无检验表行的 D_M/M2/M1）显示「—」。",
        "Smart card: inserted {note} at ({x}, {y}) rot {rot}° (table scaled 0.17).\nTable 7 gaps (>400 flank-deviation columns, ≤12 fine steps, non-6–9 grade tolerances, D_M/M2/M1 without inspection-table rows) show \"—\".",
    ),

    // ── 命令输出/报错：齿形表达式反解（card_expr.rs）──
    Msg::new(
        "cmd.cardexpr.err.module",
        "表达式模数 m={m} 非法，换算不出径节 P",
        "expression module m={m} is invalid; cannot derive diametral pitch P",
    ),
    Msg::new(
        "cmd.cardexpr.err.not_finite",
        "表达式反解出非有限数",
        "the expression resolves to a non-finite number",
    ),
    Msg::new(
        "cmd.cardexpr.err.mark",
        "{card}：表达式 MARK 写的是 {mark}，与卡片体系「{want}」不一致",
        "{card}: expression MARK is {mark}, inconsistent with the card system \"{want}\"",
    ),
    Msg::new(
        "cmd.cardexpr.err.kind",
        "{card}：表达式 KIND 写的是 {kind}，与卡片方向「{want}」不一致",
        "{card}: expression KIND is {kind}, inconsistent with the card direction \"{want}\"",
    ),
    Msg::new("cmd.cardexpr.kind.in", "IN（内）", "IN (internal)"),
    Msg::new("cmd.cardexpr.kind.ex", "EX（外）", "EX (external)"),
    Msg::new("cmd.cardexpr.dir.int", "内", "internal"),
    Msg::new("cmd.cardexpr.dir.ext", "外", "external"),
    Msg::new(
        "cmd.cardexpr.err.alpha",
        "{card}：表达式压力角 {alpha}° 不在卡片体系允许的 α={allowed}°（表达式与卡片体系不一致）",
        "{card}: expression pressure angle {alpha}° is outside the card system's allowed α={allowed}° (expression and card system mismatch)",
    ),
    Msg::new(
        "cmd.cardexpr.err.beta",
        "{card}：表达式螺旋角 β={beta}° 与卡片（只做直齿）不一致",
        "{card}: expression helix angle β={beta}° is inconsistent with the card (spur only)",
    ),
    Msg::new(
        "cmd.cardexpr.err.shift",
        "{card}：表达式径向变位 X={x} 在卡片里没有对应输入（该体系不收变位；请用 X0 的表达式）",
        "{card}: expression profile shift X={x} has no matching input on the card (this system does not accept shift; use an expression with X0)",
    ),

    // ── 命令输出/报错：GB 花键公差/查表（spline_tol.rs）──
    Msg::new(
        "cmd.spline.err.alpha_invalid",
        "花键参数表：压力角「{other}」非法（GB/T 3478.1 只有 30° / 37.5° / 45°）",
        "Spline table: invalid pressure angle \"{other}\" (GB/T 3478.1 only has 30° / 37.5° / 45°)",
    ),
    Msg::new(
        "cmd.spline.err.fit_invalid",
        "花键参数表：齿侧配合「{other}」非法（GB/T 3478.1 只有 H/k、H/js、H/h、H/f、H/e、H/d）",
        "Spline table: invalid flank fit \"{other}\" (GB/T 3478.1 only has H/k, H/js, H/h, H/f, H/e, H/d)",
    ),
    Msg::new(
        "cmd.spline.err.num",
        "{what}：{v} 不是数字（{e}）",
        "{what}: {v} is not a number ({e})",
    ),
    Msg::new(
        "cmd.spline.err.csv_header_missing",
        "{file} 缺表头",
        "{file} is missing its header",
    ),
    Msg::new(
        "cmd.spline.err.csv_header_bad",
        "{file} 表头异常：{header}",
        "{file}: unexpected header: {header}",
    ),
    Msg::new(
        "cmd.spline.err.csv_cols",
        "{file} 列数异常：{line}",
        "{file}: unexpected column count: {line}",
    ),
    Msg::new("cmd.spline.err.fit_row", "fit 行：{e}", "fit row: {e}"),
    Msg::new(
        "cmd.spline.err.m_not_t26",
        "花键参数表：m={m} 不在表 26（GB/T 3478.1 的 15 档模数）",
        "Spline table: m={m} is not in Table 26 (the 15 module steps of GB/T 3478.1)",
    ),
    Msg::new(
        "cmd.spline.err.grade_invalid",
        "花键参数表：公差等级 {grade} 非法（GB/T 3478.1 只有 4/5/6/7）",
        "Spline table: invalid tolerance grade {grade} (GB/T 3478.1 only has 4/5/6/7)",
    ),
    Msg::new(
        "cmd.spline.err.fit_len",
        "花键参数表：配合长度 g={g} 必须 >0",
        "Spline table: fit length g={g} must be > 0",
    ),
    Msg::new(
        "cmd.spline.err.d_not_t23",
        "花键参数表：D={d} 不在表 23 的分度圆直径档（≤6…800~1000），表外不插值",
        "Spline table: D={d} is not in a pitch-diameter band of Table 23 (≤6…800~1000); no interpolation outside the table",
    ),
    Msg::new(
        "cmd.spline.err.d_not_t24",
        "花键参数表：D={d} 不在表 24 的分度圆直径档（≤6…800~1000），表外不插值",
        "Spline table: D={d} is not in a pitch-diameter band of Table 24 (≤6…800~1000); no interpolation outside the table",
    ),
    Msg::new(
        "cmd.spline.err.m_range",
        "花键参数表：模数 m={m} 不在 0.25…10（GB/T 3478.1 的 15 种模数系列）",
        "Spline table: module m={m} is not within 0.25…10 (the 15-module series of GB/T 3478.1)",
    ),
    Msg::new(
        "cmd.spline.err.it_missing",
        "花键参数表：GB/T 1800 IT{it} 在 D={d} 无值（表外）",
        "Spline table: GB/T 1800 IT{it} has no value at D={d} (outside the table)",
    ),
    Msg::new(
        "cmd.spline.err.inv_range",
        "花键参数表：invα={y} 超出可解范围（α>85°）",
        "Spline table: invα={y} is beyond the solvable range (α>85°)",
    ),
    Msg::new(
        "cmd.spline.err.dri_base",
        "花键参数表：D'_Ri 的 Db/D_ee max/D_ii min 必须 >0",
        "Spline table: Db/D_ee max/D_ii min for D'_Ri must be > 0",
    ),
    Msg::new(
        "cmd.spline.err.dri_dci",
        "花键参数表：D_ci={d_ci} ≤ Db={db}（接触点已在基圆内，无法算 D'_Ri）",
        "Spline table: D_ci={d_ci} ≤ Db={db} (contact point is inside the base circle; cannot compute D'_Ri)",
    ),
    Msg::new(
        "cmd.spline.err.dri_le0",
        "花键参数表：D'_Ri={dp} ≤0（E_max 或几何异常）",
        "Spline table: D'_Ri={dp} ≤ 0 (E_max or geometry anomaly)",
    ),
    Msg::new(
        "cmd.spline.err.dre_dce",
        "花键参数表：D_ce={d_ce} ≤ Db={db}（无法算 D'_Re）",
        "Spline table: D_ce={d_ce} ≤ Db={db} (cannot compute D'_Re)",
    ),
    Msg::new(
        "cmd.spline.err.dre_le0",
        "花键参数表：D'_Re={dp} ≤0（S_min 或几何异常）",
        "Spline table: D'_Re={dp} ≤ 0 (S_min or geometry anomaly)",
    ),
    Msg::new(
        "cmd.spline.err.dp_over_series",
        "花键参数表：D'={dp} 超出 GB/T 3478.9 量棒系列上限 {max}",
        "Spline table: D'={dp} exceeds the upper limit {max} of the GB/T 3478.9 pin series",
    ),
    Msg::new(
        "cmd.spline.err.m_range15",
        "花键参数表：模数 m={m} 不在 0.25…10（15 种模数系列）",
        "Spline table: module m={m} is not within 0.25…10 (15-module series)",
    ),
    Msg::new(
        "cmd.spline.err.z_too_small",
        "花键参数表：齿数 z={z} 太小（至少 6）",
        "Spline table: tooth count z={z} is too small (at least 6)",
    ),
    Msg::new(
        "cmd.spline.err.internal_h",
        "花键参数表：内花键是基孔制 H（收到「{dev}」）",
        "Spline table: an internal spline is always H (hole-basis); got \"{dev}\"",
    ),
    Msg::new(
        "cmd.spline.err.internal_received_ext",
        "花键参数表：internal_table 收到外花键输入",
        "Spline table: internal_table received external-spline input",
    ),
    Msg::new(
        "cmd.spline.err.external_received_int",
        "花键参数表：external_table 收到内花键输入",
        "Spline table: external_table received internal-spline input",
    ),
    Msg::new(
        "cmd.spline.err.dp_positive",
        "花键参数表：量棒直径 Dp={v} 必须 >0",
        "Spline table: pin diameter Dp={v} must be > 0",
    ),
    Msg::new(
        "cmd.spline.err.dp_not_series",
        "花键参数表：Dp={v} 不在 GB/T 3478.9 表 1 量棒系列（0.56…25.00，67 档）",
        "Spline table: Dp={v} is not in the GB/T 3478.9 Table 1 pin series (0.56…25.00, 67 steps)",
    ),

    // ── 卡类型显示名（报错前缀；仅用于错误/回执前缀，不动卡面/GUI 标签）──
    Msg::new("card.name.spline", "花键参数表", "Spline table"),
    Msg::new("card.name.gear", "齿轮参数表", "Gear table"),
    Msg::new("card.name.ansi", "ANSI 花键参数表", "ANSI spline table"),
    Msg::new("card.name.nf", "NF 内花键参数表", "NF internal spline table"),
    Msg::new("card.name.nf_ext", "NF 外花键参数表", "NF external spline table"),
    Msg::new("card.name.din", "DIN 花键参数表", "DIN spline table"),

    // ── 命令报错：五卡共用的九字段表达式解析（spline_table::parse_gear_expr）──
    Msg::new(
        "cmd.card.err.expr_missing",
        "{card}：缺九字段齿形表达式（形如 `SPLINE IN M3 Z20 ALPHA30 X0 DA65.4 DF57.3436 BETA0 H30`；轴/齿轮生成器 GUI 可直接复制）",
        "{card}: missing the nine-field tooth-profile expression (like `SPLINE IN M3 Z20 ALPHA30 X0 DA65.4 DF57.3436 BETA0 H30`; copy it from the shaft/gear generator GUI)",
    ),
    Msg::new(
        "cmd.card.err.expr_parse",
        "{card}：齿形表达式无法解析：{e}",
        "{card}: cannot parse the tooth-profile expression: {e}",
    ),
    Msg::new(
        "cmd.card.err.expr_multi",
        "{card}：表达式应只有一段齿形（收到 {n} 段）——请只粘生成器复制的那一行齿形表达式",
        "{card}: the expression must contain exactly one tooth-profile segment (got {n}) — paste only the single line copied from the generator",
    ),
    Msg::new(
        "cmd.card.err.expr_no_gear",
        "{card}：表达式里没有齿形段（应以 `GEAR`/`SPLINE` + M/Z/ALPHA… 开头）",
        "{card}: the expression has no tooth-profile segment (it should start with `GEAR`/`SPLINE` + M/Z/ALPHA…)",
    ),

    // ── 命令输出/报错：GB 花键参数表命令行（spline_table.rs）──
    Msg::new(
        "cmd.spline.err.values_tag_missing",
        "花键参数表：模板 tag「{tag}」没有取值映射",
        "Spline table: template tag \"{tag}\" has no value mapping",
    ),
    Msg::new(
        "cmd.spline.err.std_missing_system",
        "花键参数表：std 缺少体系（本期只有 GB）",
        "Spline table: std is missing a system (only GB for now)",
    ),
    Msg::new(
        "cmd.spline.err.side_first",
        "花键参数表：第一个参数应为「内」或「外」（收到「{other}」）。\n{usage}",
        "Spline table: the first argument must be \"内\" or \"外\"/int/ext (got \"{other}\").\n{usage}",
    ),
    Msg::new(
        "cmd.spline.err.internal_h_mark",
        "花键参数表：内花键是基孔制 H（收到「{mark}」；写法如 `内 6H`）",
        "Spline table: an internal spline is always H (hole-basis); got \"{mark}\" (write e.g. `内 6H`)",
    ),
    Msg::new(
        "cmd.spline.err.kind_mismatch",
        "花键参数表：表达式 KIND 写的是 {kind}，与卡片方向「{want}」不一致",
        "Spline table: expression KIND is {kind}, inconsistent with the card direction \"{want}\"",
    ),
    Msg::new(
        "cmd.spline.err.need_value",
        "花键参数表：{what} 缺少数值/选项",
        "Spline table: {what} is missing a value/option",
    ),
    Msg::new("cmd.spline.err.dp_num", "dp={v} 不是数字：{e}", "dp={v} is not a number: {e}"),
    Msg::new(
        "cmd.spline.err.dp_parse_positive",
        "花键参数表：dp={dp} 必须 >0",
        "Spline table: dp={dp} must be > 0",
    ),
    Msg::new(
        "cmd.spline.err.root_invalid",
        "花键参数表：齿根形式「{other}」非法（只有 平/圆）",
        "Spline table: invalid root form \"{other}\" (only 平/flat or 圆/fillet)",
    ),
    Msg::new(
        "cmd.spline.err.at_format",
        "花键参数表：at「{v}」应为 `x,y`",
        "Spline table: at \"{v}\" must be `x,y`",
    ),
    Msg::new("cmd.spline.err.at_x", "at x={x} 不是数字：{e}", "at x={x} is not a number: {e}"),
    Msg::new("cmd.spline.err.at_y", "at y={y} 不是数字：{e}", "at y={y} is not a number: {e}"),
    Msg::new("cmd.spline.err.rot", "rot={v} 不是数字：{e}", "rot={v} is not a number: {e}"),
    Msg::new(
        "cmd.spline.err.unknown_param",
        "花键参数表：不认识的参数「{other}」。\n{usage}",
        "Spline table: unrecognized argument \"{other}\".\n{usage}",
    ),
    Msg::new(
        "cmd.spline.err.mark_format",
        "花键参数表：「{mark}」应形如 `6H` / `5f`",
        "Spline table: \"{mark}\" must look like `6H` / `5f`",
    ),
    Msg::new(
        "cmd.spline.err.grade_num",
        "花键参数表：等级「{g}」不是数字：{e}",
        "Spline table: grade \"{g}\" is not a number: {e}",
    ),
    Msg::new(
        "cmd.spline.err.grade_invalid_card",
        "花键参数表：公差等级「{grade}」非法（GB/T 3478.1 只有 4/5/6/7）",
        "Spline table: invalid tolerance grade \"{grade}\" (GB/T 3478.1 only has 4/5/6/7)",
    ),
    Msg::new(
        "cmd.spline.err.mark_no_fit",
        "花键参数表：「{mark}」缺配合类别（内如 `6H`、外如 `5f`）",
        "Spline table: \"{mark}\" is missing the fit class (e.g. `6H` for internal, `5f` for external)",
    ),
    Msg::new(
        "cmd.spline.usage",
        "智能卡片「花键参数表」用法：`OCSMCARD 花键参数表 [std GB] 内 6H <九字段表达式> [dp 4.5] [root 平|圆] [at x,y] [rot 度]`；外花键把 `内 6H` 换成 `外 5f`（表达式 KIND 用 EX）。表达式形如 `SPLINE IN M3 Z20 ALPHA30 X0 DA65.4 DF57.3436 BETA0 H30`（轴/齿轮生成器 GUI 可直接复制）。等级 4/5/6/7；配合内 H、外 d/e/f/h/js/k；不填 dp = 按标准 R40 自动选；不填 root = 由表达式 DA/DF 反解（30° 可分辨平/圆），再退到按 αD 默认。",
        "Smart card \"Spline table\" usage: `OCSMCARD 花键参数表 [std GB] 内 6H <nine-field expression> [dp 4.5] [root 平|圆] [at x,y] [rot deg]`; for external splines replace `内 6H` with `外 5f` (use KIND EX in the expression). Example expression: `SPLINE IN M3 Z20 ALPHA30 X0 DA65.4 DF57.3436 BETA0 H30` (copy it from the shaft/gear generator GUI). Grades 4/5/6/7; internal fit H, external d/e/f/h/js/k; omit dp = auto-pick per standard R40; omit root = resolve from DA/DF in the expression (30° distinguishes flat/fillet), then fall back to the αD default.",
    ),

    // ── 命令输出/报错：齿轮参数表命令行（gear_table.rs）──
    Msg::new(
        "cmd.geartab.err.m_invalid",
        "齿轮参数表：模数 m={m} 非法",
        "Gear table: invalid module m={m}",
    ),
    Msg::new(
        "cmd.geartab.err.missing_da",
        "齿轮参数表：表达式缺 `DA`（大径）—— 齿顶高系数要从它反解（轴/齿轮生成器 GUI 可复制完整九字段）",
        "Gear table: the expression is missing `DA` (major diameter) — the addendum coefficient is back-solved from it (copy the full nine fields from the shaft/gear generator GUI)",
    ),
    Msg::new(
        "cmd.geartab.err.missing_df",
        "齿轮参数表：表达式缺 `DF`（小径）—— 全齿高与顶隙系数要从它反解",
        "Gear table: the expression is missing `DF` (minor diameter) — the whole depth and clearance coefficient are back-solved from it",
    ),
    Msg::new(
        "cmd.geartab.err.params_invalid",
        "齿轮参数表：表达式反解的齿形参数不合法：{e}",
        "Gear table: the tooth-profile parameters back-solved from the expression are invalid: {e}",
    ),
    Msg::new(
        "cmd.geartab.err.closed_loop",
        "齿轮参数表：表达式 DA/DF 与 M/Z/ALPHA/X 不自洽（反解 ha*={ha}、c*={c} 后引擎给 Da={da}、Df={df}，表达式写 Da={da_in}、Df={df_in}）",
        "Gear table: expression DA/DF is inconsistent with M/Z/ALPHA/X (after back-solving ha*={ha}, c*={c}, the engine gives Da={da}, Df={df}, but the expression says Da={da_in}, Df={df_in})",
    ),
    Msg::new(
        "cmd.geartab.err.tag_order",
        "齿轮参数表：取值映射顺序与模板属性不一致：{got} != {want}",
        "Gear table: value-mapping order differs from the template attributes: {got} != {want}",
    ),
    Msg::new(
        "cmd.geartab.err.mate_z",
        "齿轮参数表：配对齿轮齿数 z₂={z} 超出范围（2–1000）",
        "Gear table: mating-gear tooth count z₂={z} is out of range (2–1000)",
    ),
    Msg::new(
        "cmd.geartab.err.center",
        "齿轮参数表：中心距 center={a} 必须是正数",
        "Gear table: center distance center={a} must be positive",
    ),
    Msg::new(
        "cmd.geartab.err.need",
        "齿轮参数表：{what} 缺少数值/字符串",
        "Gear table: {what} is missing a value/string",
    ),
    Msg::new(
        "cmd.geartab.err.mate_num",
        "齿轮参数表：mate={v} 不是整数：{e}",
        "Gear table: mate={v} is not an integer: {e}",
    ),
    Msg::new(
        "cmd.geartab.err.center_num",
        "齿轮参数表：center={v} 不是数字：{e}",
        "Gear table: center={v} is not a number: {e}",
    ),
    Msg::new(
        "cmd.geartab.err.at_format",
        "齿轮参数表：at「{v}」应为 `x,y`",
        "Gear table: at \"{v}\" must be `x,y`",
    ),
    Msg::new("cmd.geartab.err.at_x", "齿轮参数表：at x={x} 不是数字：{e}", "Gear table: at x={x} is not a number: {e}"),
    Msg::new("cmd.geartab.err.at_y", "齿轮参数表：at y={y} 不是数字：{e}", "Gear table: at y={y} is not a number: {e}"),
    Msg::new("cmd.geartab.err.rot", "齿轮参数表：rot={v} 不是数字：{e}", "Gear table: rot={v} is not a number: {e}"),
    Msg::new(
        "cmd.geartab.err.unknown_param",
        "齿轮参数表：不认识的参数「{other}」。\n{usage}",
        "Gear table: unrecognized argument \"{other}\".\n{usage}",
    ),
    Msg::new(
        "cmd.geartab.usage",
        "智能卡片「齿轮参数表」用法：`OCSMCARD 齿轮参数表 <九字段表达式> [mate z₂] [dwg 图号] [grade 精度等级] [center a] [at x,y] [rot 度]`。表达式形如 `GEAR EX M3 Z20 ALPHA20 X0 DA66 DF52.5 BETA0 H30`（轴/齿轮生成器 GUI 可直接复制）；DA/DF 用来反解 ha*/c*，内/外齿由表达式 KIND 决定。GB/T 10095-88 公差（Fr/FW/ff/fpt/Fβ）与中心距极限偏差本仓未收 —— 卡片显示「—」，不臆造。",
        "Smart card \"Gear table\" usage: `OCSMCARD 齿轮参数表 <nine-field expression> [mate z₂] [dwg drawing-no] [grade precision-grade] [center a] [at x,y] [rot deg]`. Example expression: `GEAR EX M3 Z20 ALPHA20 X0 DA66 DF52.5 BETA0 H30` (copy it from the shaft/gear generator GUI); DA/DF are used to back-solve ha*/c*, and internal/external teeth are determined by KIND in the expression. GB/T 10095-88 tolerances (Fr/FW/ff/fpt/Fβ) and center-distance limit deviations are not collected in this repo — the card shows \"—\" instead of guessing.",
    ),
    Msg::new("cmd.geartab.kind.internal", "内齿轮", "internal gear"),
    Msg::new("cmd.geartab.kind.external", "外齿轮", "external gear"),
    Msg::new(
        "cmd.geartab.echo",
        "{kind} z{z} m{m}（ha*={ha}，c*={c}）公法线 k={k} W={w}",
        "{kind} z{z} m{m} (ha*={ha}, c*={c}) base tangent k={k} W={w}",
    ),

    // ── 命令输出/报错：ANSI 花键参数表命令行（ansi_table.rs）──
    Msg::new(
        "cmd.ansi.err.need_value",
        "ANSI 花键参数表：{what} 缺少数值/选项",
        "ANSI spline table: {what} is missing a value/option",
    ),
    Msg::new(
        "cmd.ansi.err.z_num",
        "ANSI 花键参数表：齿数={v} 不是整数：{e}",
        "ANSI spline table: tooth count={v} is not an integer: {e}",
    ),
    Msg::new(
        "cmd.ansi.err.at_format",
        "ANSI 花键参数表：at「{v}」应为 `x,y`",
        "ANSI spline table: at \"{v}\" must be `x,y`",
    ),
    Msg::new(
        "cmd.ansi.err.at_x",
        "ANSI 花键参数表：at x={x} 不是数字：{e}",
        "ANSI spline table: at x={x} is not a number: {e}",
    ),
    Msg::new(
        "cmd.ansi.err.at_y",
        "ANSI 花键参数表：at y={y} 不是数字：{e}",
        "ANSI spline table: at y={y} is not a number: {e}",
    ),
    Msg::new(
        "cmd.ansi.err.rot",
        "ANSI 花键参数表：rot={v} 不是数字：{e}",
        "ANSI spline table: rot={v} is not a number: {e}",
    ),
    Msg::new(
        "cmd.ansi.err.unknown_param",
        "ANSI 花键参数表：不认识的参数「{t}」。\n{usage}",
        "ANSI spline table: unrecognized argument \"{t}\".\n{usage}",
    ),
    Msg::new(
        "cmd.ansi.err.p_mismatch",
        "ANSI 花键参数表：显式径节 P={p} 与表达式反解的 P/Ps={ep} 不一致",
        "ANSI spline table: explicit diametral pitch P={p} differs from the expression's P/Ps={ep}",
    ),
    Msg::new(
        "cmd.ansi.err.n_mismatch",
        "ANSI 花键参数表：显式齿数 N={given} 与表达式反解的 N={ez} 不一致",
        "ANSI spline table: explicit tooth count N={given} differs from the expression's N={ez}",
    ),
    Msg::new(
        "cmd.ansi.err.missing_p",
        "ANSI 花键参数表：缺径节 P（写法 `P16` / `P 16/32` / `径节 16`，或直接给九字段表达式）。\n{usage}",
        "ANSI spline table: missing diametral pitch P (write `P16` / `P 16/32` / `径节 16`, or supply the nine-field expression).\n{usage}",
    ),
    Msg::new(
        "cmd.ansi.err.missing_z",
        "ANSI 花键参数表：缺齿数 Z（写法 `Z20` / `齿数 20`；表达式反解时无需再给）。\n{usage}",
        "ANSI spline table: missing tooth count Z (write `Z20` / `齿数 20`; not needed when the expression is back-solved).\n{usage}",
    ),
    Msg::new(
        "cmd.ansi.err.no_profile",
        "ANSI 花键参数表：没有压力角 {alpha}° 的 Table 2 齿廓（本仓仅 30/37.5/45）",
        "ANSI spline table: no Table 2 profile for pressure angle {alpha}° (this repo only has 30/37.5/45)",
    ),
    Msg::new(
        "cmd.ansi.err.map_missing_p",
        "ANSI 花键参数表：表达式映射表缺径节 P（内部错误）",
        "ANSI spline table: the expression mapping has no diametral pitch P (internal error)",
    ),
    Msg::new(
        "cmd.ansi.err.map_missing_n",
        "ANSI 花键参数表：表达式映射表缺齿数 N（内部错误）",
        "ANSI spline table: the expression mapping has no tooth count N (internal error)",
    ),
    Msg::new(
        "cmd.ansi.err.side_invalid",
        "ANSI 花键参数表：方向「{other}」非法（可选项 int/ext）",
        "ANSI spline table: invalid direction \"{other}\" (allowed: int/ext)",
    ),
    Msg::new(
        "cmd.ansi.usage",
        "智能卡片「{card}」用法：`OCSMCARD {card} 内|外 <九字段齿形表达式> [profile 齿廓] [at x,y] [rot 度]`（如 `OCSMCARD {card} 内 SPLINE IN M1.5875 Z20 ALPHA30 X0 BETA0 H30`；也可沿用 `P<径节> Z<齿数>` 写法，如 `OCSMCARD {card} 内 P16 Z20`）。表达式反解 P=25.4/m、N=z，齿廓按 α 保持同角列或选默认列（α 只收 30/37.5/45，直齿、不收变位）；齿廓五列：`ANSI30P`/`ANSI30PM`/`ANSI30R`/`ANSI375R`/`ANSI45R`（缺省列 A）；ANSI B92.1 的配合/公差/量棒/公法线表本仓未收 —— 相关格显示「—」，不臆造。",
        "Smart card \"{card}\" usage: `OCSMCARD {card} 内|外 <nine-field expression> [profile PROFILE] [at x,y] [rot deg]` (e.g. `OCSMCARD {card} 内 SPLINE IN M1.5875 Z20 ALPHA30 X0 BETA0 H30`; the `P<pitch> Z<teeth>` form still works, e.g. `OCSMCARD {card} 内 P16 Z20`). The expression is back-solved to P=25.4/m, N=z; the profile keeps the same-angle column or falls back to the default column (only α 30/37.5/45, spur only, no shift); profile tokens: `ANSI30P`/`ANSI30PM`/`ANSI30R`/`ANSI375R`/`ANSI45R` (default column A). ANSI B92.1 fit/tolerance/pin/base-tangent tables are not collected in this repo — the related cells show \"—\" instead of guessing.",
    ),
    Msg::new("cmd.ansi.side.internal", "内花键", "internal spline"),
    Msg::new("cmd.ansi.side.external", "外花键", "external spline"),
    Msg::new(
        "cmd.ansi.echo",
        "{side}，P/Ps={pair}，N={n}，α={alpha}°",
        "{side}, P/Ps={pair}, N={n}, α={alpha}°",
    ),

    // ── 命令输出/报错：NF 内花键参数表命令行（nf_table.rs）──
    Msg::new(
        "cmd.nf.err.a_positive",
        "NF 内花键参数表：公称直径 A={a} 必须是正数",
        "NF internal spline table: nominal diameter A={a} must be positive",
    ),
    Msg::new(
        "cmd.nf.err.m_positive",
        "NF 内花键参数表：模数 m={m} 必须是正数",
        "NF internal spline table: module m={m} must be positive",
    ),
    Msg::new(
        "cmd.nf.err.z_range",
        "NF 内花键参数表：齿数 z={z} 超出范围（3–1000）",
        "NF internal spline table: tooth count z={z} is out of range (3–1000)",
    ),
    Msg::new(
        "cmd.nf.err.z_table_mismatch",
        "NF 内花键参数表：齿数 z={z_in} 与 NF E22-141 表行（A={a} m={m}）的 N={z_tab} 不一致；表值行请去掉 z 或改为 N={z_tab}",
        "NF internal spline table: tooth count z={z_in} differs from N={z_tab} in the NF E22-141 table row (A={a} m={m}); drop z or set N={z_tab} for a table row",
    ),
    Msg::new(
        "cmd.nf.err.tag_order",
        "NF 内花键参数表：取值映射顺序与模板属性不一致：{got} != {want}",
        "NF internal spline table: value-mapping order differs from the template attributes: {got} != {want}",
    ),
    Msg::new(
        "cmd.nf.err.centering_invalid",
        "NF 内花键参数表：定心方式「{t}」非法（可用 外径 / 齿面）",
        "NF internal spline table: invalid centering \"{t}\" (allowed: 外径/major-diameter or 齿面/flank)",
    ),
    Msg::new(
        "cmd.nf.err.root_invalid",
        "NF 内花键参数表：齿根样式「{t}」非法（可用 平 / 圆）",
        "NF internal spline table: invalid root style \"{t}\" (allowed: 平/flat or 圆/fillet)",
    ),
    Msg::new(
        "cmd.nf.err.fit_invalid",
        "NF 内花键参数表：配合类别「{t}」非法（可用 松动 / 滑动 / 固定 / 压）",
        "NF internal spline table: invalid fit class \"{t}\" (allowed: 松动/loose, 滑动/sliding, 固定/fixed, 压/press)",
    ),
    Msg::new(
        "cmd.nf.err.map_missing_a",
        "NF 内花键参数表：表达式映射表缺 A（内部错误）",
        "NF internal spline table: the expression mapping has no A (internal error)",
    ),
    Msg::new(
        "cmd.nf.err.map_missing_m",
        "NF 内花键参数表：表达式映射表缺 m（内部错误）",
        "NF internal spline table: the expression mapping has no m (internal error)",
    ),
    Msg::new(
        "cmd.nf.err.map_missing_z",
        "NF 内花键参数表：表达式映射表缺 z（内部错误）",
        "NF internal spline table: the expression mapping has no z (internal error)",
    ),
    Msg::new(
        "cmd.nf.err.need",
        "NF 内花键参数表：{what} 缺少数值/选项",
        "NF internal spline table: {what} is missing a value/option",
    ),
    Msg::new(
        "cmd.nf.err.at_format",
        "NF 内花键参数表：at「{v}」应为 `x,y`",
        "NF internal spline table: at \"{v}\" must be `x,y`",
    ),
    Msg::new(
        "cmd.nf.err.at_x",
        "NF 内花键参数表：at x={x} 不是数字：{e}",
        "NF internal spline table: at x={x} is not a number: {e}",
    ),
    Msg::new(
        "cmd.nf.err.at_y",
        "NF 内花键参数表：at y={y} 不是数字：{e}",
        "NF internal spline table: at y={y} is not a number: {e}",
    ),
    Msg::new(
        "cmd.nf.err.rot",
        "NF 内花键参数表：rot={v} 不是数字：{e}",
        "NF internal spline table: rot={v} is not a number: {e}",
    ),
    Msg::new(
        "cmd.nf.err.num",
        "NF 内花键参数表：{name}={v} 不是数字：{e}",
        "NF internal spline table: {name}={v} is not a number: {e}",
    ),
    Msg::new(
        "cmd.nf.err.z_num",
        "NF 内花键参数表：{name}={v} 不是整数：{e}",
        "NF internal spline table: {name}={v} is not an integer: {e}",
    ),
    Msg::new(
        "cmd.nf.err.unknown_param",
        "NF 内花键参数表：不认识的参数「{t}」。\n{usage}",
        "NF internal spline table: unrecognized argument \"{t}\".\n{usage}",
    ),
    Msg::new(
        "cmd.nf.err.explicit_mismatch",
        "NF 内花键参数表：显式 {name}={given} 与表达式反解的 {name}={resolved} 不一致",
        "NF internal spline table: explicit {name}={given} differs from the expression's {name}={resolved}",
    ),
    Msg::new(
        "cmd.nf.err.missing_a",
        "NF 内花键参数表：缺公称直径 A（写法 `A300` / `直径 300`，或直接给九字段表达式）。\n{usage}",
        "NF internal spline table: missing nominal diameter A (write `A300` / `直径 300`, or supply the nine-field expression).\n{usage}",
    ),
    Msg::new(
        "cmd.nf.err.missing_m",
        "NF 内花键参数表：缺模数 m（写法 `M7.5` / `模数 7.5`；同一 A 可对应不同模数，必填）。\n{usage}",
        "NF internal spline table: missing module m (write `M7.5` / `模数 7.5`; the same A can match different modules, so it is required).\n{usage}",
    ),
    Msg::new(
        "cmd.nf.usage",
        "智能卡片「NF内花键参数表」用法：`OCSMCARD NF内花键参数表 <九字段齿形表达式> [中心 外径|齿面] [根 平|圆] [配合 松动|滑动|固定|压] [at x,y] [rot 度]`（如 `OCSMCARD NF内花键参数表 SPLINE IN M7.5 Z38 ALPHA20 X0.8 BETA0 H30`；也可沿用 `A300 M7.5 [Z38]` 写法）。表达式反解 A=m(z+0.4+2x)、m、z（NF 压力角恒 20°）；A/m 取 NF E22-141 表值（p18 尺寸表，同一直径可对应不同模数）；定心方式缺省「外径定心」（Az=A；齿面定心 Az=A+0.3m）；加工方法照 p18 = 拉削；V/V1/G/G1 取 p23–p25 检查表、ri 取 p22 —— 表外显示「—」，不外推；公差：大径 R7 / 小径 H7（p28，数值按 ISO 286），跨棒距 = p29 内花键 E 偏差；配合类别（缺省固定）只影响预览里配对外花键的 E/xm 偏差读数。",
        "Smart card \"NF internal spline table\" usage: `OCSMCARD NF内花键参数表 <nine-field expression> [中心 外径|齿面] [根 平|圆] [配合 松动|滑动|固定|压] [at x,y] [rot deg]` (e.g. `OCSMCARD NF内花键参数表 SPLINE IN M7.5 Z38 ALPHA20 X0.8 BETA0 H30`; the `A300 M7.5 [Z38]` form still works). The expression is back-solved to A=m(z+0.4+2x), m, z (NF pressure angle is always 20°); A/m come from the NF E22-141 table (p18 dimension table; the same diameter can match different modules); centering defaults to major-diameter (Az=A; flank centering Az=A+0.3m); the process follows p18 = broaching; V/V1/G/G1 come from the p23–p25 inspection table and ri from p22 — off-table values show \"—\" without extrapolation; tolerances: major R7 / minor H7 (p28, values per ISO 286), span = p29 internal-spline E deviation; the fit class (default fixed) only affects the mated external-spline E/xm deviation readings in the preview.",
    ),
    Msg::new("cmd.nf.centering.outer", "外径定心", "major-diameter centering"),
    Msg::new("cmd.nf.centering.flank", "齿面定心", "flank centering"),
    Msg::new("cmd.nf.fit.loose", "松动", "loose"),
    Msg::new("cmd.nf.fit.sliding", "滑动", "sliding"),
    Msg::new("cmd.nf.fit.fixed", "固定", "fixed"),
    Msg::new("cmd.nf.fit.press", "压", "press"),
    Msg::new(
        "cmd.nf.echo",
        "NF E22-141 内花键 A={a} m={m} z={z}（{centering}，拉削；{fit}配合；V/G/G1 {vg}）",
        "NF E22-141 internal spline A={a} m={m} z={z} ({centering}, broached; {fit} fit; V/G/G1 {vg})",
    ),

    // ── 命令输出/报错：NF 外花键参数表命令行（nf_ext_table.rs）──
    Msg::new(
        "cmd.nfext.err.a_positive",
        "NF 外花键参数表：公称直径 A={a} 必须是正数",
        "NF external spline table: nominal diameter A={a} must be positive",
    ),
    Msg::new(
        "cmd.nfext.err.m_positive",
        "NF 外花键参数表：模数 m={m} 必须是正数",
        "NF external spline table: module m={m} must be positive",
    ),
    Msg::new(
        "cmd.nfext.err.z_range",
        "NF 外花键参数表：齿数 z={z} 超出范围（3–1000）",
        "NF external spline table: tooth count z={z} is out of range (3–1000)",
    ),
    Msg::new(
        "cmd.nfext.err.z_table_mismatch",
        "NF 外花键参数表：齿数 z={z_in} 与 NF E22-141 表行（A={a} m={m}）的 N={z_tab} 不一致；表值行请去掉 z 或改为 N={z_tab}",
        "NF external spline table: tooth count z={z_in} differs from N={z_tab} in the NF E22-141 table row (A={a} m={m}); drop z or set N={z_tab} for a table row",
    ),
    Msg::new(
        "cmd.nfext.err.tag_order",
        "NF 外花键参数表：取值映射顺序与模板属性不一致：{got} != {want}",
        "NF external spline table: value-mapping order differs from the template attributes: {got} != {want}",
    ),
    Msg::new(
        "cmd.nfext.err.centering_invalid",
        "NF 外花键参数表：定心方式「{t}」非法（可用 齿面 / 外径）",
        "NF external spline table: invalid centering \"{t}\" (allowed: 齿面/flank or 外径/major-diameter)",
    ),
    Msg::new(
        "cmd.nfext.err.root_invalid",
        "NF 外花键参数表：齿根样式「{t}」非法（可用 平 / 圆）",
        "NF external spline table: invalid root style \"{t}\" (allowed: 平/flat or 圆/fillet)",
    ),
    Msg::new(
        "cmd.nfext.err.fit_invalid",
        "NF 外花键参数表：配合类别「{t}」非法（可用 松动 / 滑动 / 固定 / 压）",
        "NF external spline table: invalid fit class \"{t}\" (allowed: 松动/loose, 滑动/sliding, 固定/fixed, 压/press)",
    ),
    Msg::new(
        "cmd.nfext.err.map_missing_a",
        "NF 外花键参数表：表达式映射表缺 A（内部错误）",
        "NF external spline table: the expression mapping has no A (internal error)",
    ),
    Msg::new(
        "cmd.nfext.err.map_missing_m",
        "NF 外花键参数表：表达式映射表缺 m（内部错误）",
        "NF external spline table: the expression mapping has no m (internal error)",
    ),
    Msg::new(
        "cmd.nfext.err.map_missing_z",
        "NF 外花键参数表：表达式映射表缺 z（内部错误）",
        "NF external spline table: the expression mapping has no z (internal error)",
    ),
    Msg::new(
        "cmd.nfext.err.need",
        "NF 外花键参数表：{what} 缺少数值/选项",
        "NF external spline table: {what} is missing a value/option",
    ),
    Msg::new(
        "cmd.nfext.err.at_format",
        "NF 外花键参数表：at「{v}」应为 `x,y`",
        "NF external spline table: at \"{v}\" must be `x,y`",
    ),
    Msg::new(
        "cmd.nfext.err.at_x",
        "NF 外花键参数表：at x={x} 不是数字：{e}",
        "NF external spline table: at x={x} is not a number: {e}",
    ),
    Msg::new(
        "cmd.nfext.err.at_y",
        "NF 外花键参数表：at y={y} 不是数字：{e}",
        "NF external spline table: at y={y} is not a number: {e}",
    ),
    Msg::new(
        "cmd.nfext.err.rot",
        "NF 外花键参数表：rot={v} 不是数字：{e}",
        "NF external spline table: rot={v} is not a number: {e}",
    ),
    Msg::new(
        "cmd.nfext.err.num",
        "NF 外花键参数表：{name}={v} 不是数字：{e}",
        "NF external spline table: {name}={v} is not a number: {e}",
    ),
    Msg::new(
        "cmd.nfext.err.z_num",
        "NF 外花键参数表：{name}={v} 不是整数：{e}",
        "NF external spline table: {name}={v} is not an integer: {e}",
    ),
    Msg::new(
        "cmd.nfext.err.unknown_param",
        "NF 外花键参数表：不认识的参数「{t}」。\n{usage}",
        "NF external spline table: unrecognized argument \"{t}\".\n{usage}",
    ),
    Msg::new(
        "cmd.nfext.err.explicit_mismatch",
        "NF 外花键参数表：显式 {name}={given} 与表达式反解的 {name}={resolved} 不一致",
        "NF external spline table: explicit {name}={given} differs from the expression's {name}={resolved}",
    ),
    Msg::new(
        "cmd.nfext.err.missing_a",
        "NF 外花键参数表：缺公称直径 A（写法 `A300` / `直径 300`，或直接给九字段表达式）。\n{usage}",
        "NF external spline table: missing nominal diameter A (write `A300` / `直径 300`, or supply the nine-field expression).\n{usage}",
    ),
    Msg::new(
        "cmd.nfext.err.missing_m",
        "NF 外花键参数表：缺模数 m（写法 `M7.5` / `模数 7.5`；同一 A 可对应不同模数，必填）。\n{usage}",
        "NF external spline table: missing module m (write `M7.5` / `模数 7.5`; the same A can match different modules, so it is required).\n{usage}",
    ),
    Msg::new(
        "cmd.nfext.usage",
        "智能卡片「NF外花键参数表」用法：`OCSMCARD NF外花键参数表 <九字段齿形表达式> [中心 齿面|外径] [根 平|圆] [配合 松动|滑动|固定|压] [at x,y] [rot 度]`（如 `OCSMCARD NF外花键参数表 SPLINE EX M7.5 Z38 ALPHA20 X0.8 BETA0 H30`；也可沿用 `A300 M7.5 [Z38]` 写法）。表达式反解 A=m(z+0.4+2x)、m、z（NF 压力角恒 20°、方向恒外 EX）；定心方式缺省「齿面定心」（Dee=A−0.2m，模板口径；外径定心 Dee=A）；加工方法照模板 = 滚齿；小径 Die 平齿根 A−2.4m / 圆齿根 A−2.694m；K/W 取 p23–p25 检查表 —— 表外显示「—」，不外推；公差：大径 h12 / 小径 H7（ISO 286，模板实测口径），公法线 = p29 外花键 E 偏差（按配合类别，缺省固定）。",
        "Smart card \"NF external spline table\" usage: `OCSMCARD NF外花键参数表 <nine-field expression> [中心 齿面|外径] [根 平|圆] [配合 松动|滑动|固定|压] [at x,y] [rot deg]` (e.g. `OCSMCARD NF外花键参数表 SPLINE EX M7.5 Z38 ALPHA20 X0.8 BETA0 H30`; the `A300 M7.5 [Z38]` form still works). The expression is back-solved to A=m(z+0.4+2x), m, z (NF pressure angle is always 20°, direction always external EX); centering defaults to flank (Dee=A−0.2m, template convention; major-diameter centering Dee=A); the process follows the template = hobbing; minor diameter Die is A−2.4m for flat root / A−2.694m for fillet root; K/W come from the p23–p25 inspection table — off-table values show \"—\" without extrapolation; tolerances: major h12 / minor H7 (ISO 286, template-measured convention), base tangent = p29 external-spline E deviation (per fit class, default fixed).",
    ),
    Msg::new(
        "cmd.nfext.echo",
        "NF E22-141 外花键 A={a} m={m} z={z}（{centering}，滚齿；{fit}配合；K/W {kw}）",
        "NF E22-141 external spline A={a} m={m} z={z} ({centering}, hobbed; {fit} fit; K/W {kw})",
    ),

    // ── 命令输出/报错：DIN 5480 花键参数表命令行（din_table.rs）──
    Msg::new("cmd.din.err.prefix", "DIN 花键参数表：{e}", "DIN spline table: {e}"),
    Msg::new(
        "cmd.din.err.m_positive",
        "DIN 花键参数表：模数 m={m} 必须是正数",
        "DIN spline table: module m={m} must be positive",
    ),
    Msg::new(
        "cmd.din.err.z_range",
        "DIN 花键参数表：齿数 z={z} 超出范围（3–1000）",
        "DIN spline table: tooth count z={z} is out of range (3–1000)",
    ),
    Msg::new(
        "cmd.din.err.db_positive",
        "DIN 花键参数表：基准直径 d_B={d_b} 必须是正数",
        "DIN spline table: base diameter d_B={d_b} must be positive",
    ),
    Msg::new(
        "cmd.din.err.grade_range",
        "公差等级 {grade} 超出范围（1–12）",
        "tolerance grade {grade} is out of range (1–12)",
    ),
    Msg::new(
        "cmd.din.err.dev_letter",
        "{side}偏差系列「{letter}」非法（{allowed}）",
        "{side}deviation series \"{letter}\" is invalid ({allowed})",
    ),
    Msg::new("cmd.din.side.hole", "孔 ", "hole "),
    Msg::new("cmd.din.side.shaft", "轴 ", "shaft "),
    Msg::new(
        "cmd.din.dev.hub",
        "F/G/H/J/K/M（DIN 5480 §10.3 六个）",
        "F/G/H/J/K/M (six, DIN 5480 §10.3)",
    ),
    Msg::new(
        "cmd.din.dev.shaft",
        "v/u/t/s/r/p/n/m/k/js/h/g/f/e/d/c/b/a（十八个）",
        "v/u/t/s/r/p/n/m/k/js/h/g/f/e/d/c/b/a (eighteen)",
    ),
    Msg::new(
        "cmd.din.err.tag_order",
        "DIN 花键参数表：取值映射顺序与模板属性不一致：{got} != {want}",
        "DIN spline table: value-mapping order differs from the template attributes: {got} != {want}",
    ),
    Msg::new(
        "cmd.din.err.need",
        "DIN 花键参数表：{what} 缺少数值",
        "DIN spline table: {what} is missing a value",
    ),
    Msg::new(
        "cmd.din.err.body_invalid",
        "DIN 花键参数表：代号体「{t}」非法。\n{usage}",
        "DIN spline table: invalid designation body \"{t}\".\n{usage}",
    ),
    Msg::new(
        "cmd.din.err.code_invalid",
        "DIN 花键参数表：代号「{t}」非法（应如 `N120×3×38×9H`）。\n{usage}",
        "DIN spline table: invalid designation \"{t}\" (should look like `N120×3×38×9H`).\n{usage}",
    ),
    Msg::new(
        "cmd.din.err.key_missing",
        "DIN 花键参数表：「{key}」缺少数值",
        "DIN spline table: \"{key}\" is missing a value",
    ),
    Msg::new(
        "cmd.din.err.at_format",
        "DIN 花键参数表：at「{v}」应为 `x,y`",
        "DIN spline table: at \"{v}\" must be `x,y`",
    ),
    Msg::new("cmd.din.err.at_x", "at x={x} 不是数字：{e}", "at x={x} is not a number: {e}"),
    Msg::new("cmd.din.err.at_y", "at y={y} 不是数字：{e}", "at y={y} is not a number: {e}"),
    Msg::new("cmd.din.err.rot", "rot={v} 不是数字：{e}", "rot={v} is not a number: {e}"),
    Msg::new(
        "cmd.din.err.num",
        "{name}={v} 不是数字：{e}",
        "{name}={v} is not a number: {e}",
    ),
    Msg::new(
        "cmd.din.err.z_num",
        "{name}={v} 不是整数：{e}",
        "{name}={v} is not an integer: {e}",
    ),
    Msg::new(
        "cmd.din.err.unknown_param_bare",
        "不认识的参数「{t}」",
        "unrecognized argument \"{t}\"",
    ),
    Msg::new(
        "cmd.din.err.wrapped",
        "DIN 花键参数表：{e}。\n{usage}",
        "DIN spline table: {e}.\n{usage}",
    ),
    Msg::new(
        "cmd.din.err.pending_len",
        "DIN 花键参数表：`N`/`W` 后缺长度代号（如 `N 120×3×38×9H`）。\n{usage}",
        "DIN spline table: `N`/`W` is missing a length designation (e.g. `N 120×3×38×9H`).\n{usage}",
    ),
    Msg::new(
        "cmd.din.err.map_missing_m",
        "DIN 花键参数表：表达式映射表缺 m（内部错误）",
        "DIN spline table: the expression mapping has no m (internal error)",
    ),
    Msg::new(
        "cmd.din.err.map_missing_z",
        "DIN 花键参数表：表达式映射表缺 z（内部错误）",
        "DIN spline table: the expression mapping has no z (internal error)",
    ),
    Msg::new(
        "cmd.din.err.map_missing_db",
        "DIN 花键参数表：表达式映射表缺 d_B（内部错误）",
        "DIN spline table: the expression mapping has no d_B (internal error)",
    ),
    Msg::new(
        "cmd.din.err.explicit_mismatch",
        "DIN 花键参数表：显式 {name}={given} 与表达式反解的 {name}={resolved} 不一致",
        "DIN spline table: explicit {name}={given} differs from the expression's {name}={resolved}",
    ),
    Msg::new(
        "cmd.din.err.missing_fields",
        "DIN 花键参数表：缺 {what}（写法 `M3 Z38 B120` / 代号 `N 120×3×38×9H` / 九字段表达式）。\n{usage}",
        "DIN spline table: missing {what} (write `M3 Z38 B120` / designation `N 120×3×38×9H` / nine-field expression).\n{usage}",
    ),
    Msg::new("cmd.din.what.mzdb", "模数 m、齿数 z、基准直径 d_B", "module m, tooth count z, base diameter d_B"),
    Msg::new("cmd.din.what.m", "模数 m", "module m"),
    Msg::new("cmd.din.what.z", "齿数 z", "tooth count z"),
    Msg::new("cmd.din.what.db", "基准直径 d_B", "base diameter d_B"),
    Msg::new(
        "cmd.din.err.fit_grade_digits",
        "缺等级数字（如 `9H`/`8f`）",
        "missing grade digits (e.g. `9H`/`8f`)",
    ),
    Msg::new(
        "cmd.din.err.fit_grade_invalid",
        "等级「{digits}」非法",
        "invalid grade \"{digits}\"",
    ),
    Msg::new(
        "cmd.din.err.fit_letter_missing",
        "缺偏差字母（孔 F/G/H/J/K/M；轴 v…a）",
        "missing deviation letter (hole F/G/H/J/K/M; shaft v…a)",
    ),
    Msg::new(
        "cmd.din.usage",
        "智能卡片「DIN 5480 内/外花键参数表」用法（一卡一方向）：`OCSMCARD DIN花键参数表 <九字段表达式> [N<等级><字母>] [e2 …] [ae …] [as …] [tactn …] [teffn …] [at x,y]`（外卡：`OCSMCARD DIN花键参数表_外 <九字段表达式> [W<等级><字母>] [tactw …] [teffw …] …`；旧写法 `M3 Z38 B120 N9H W8f` 兼容：旧 id 默认内卡，N/W 都收、只取本侧）。表达式反解 d_B=m(z+1.1+2x)、m、z（DIN 5480 压力角恒 30°），KIND 须与卡方向一致（内 IN / 外 EX）。版面 = DIN 5480-1:2006 Bild 6 单栏 13 行（内 = Nabe、外 = Welle），整表 INSERT 缩放 0.17；孔缺省 9H、轴缺省 8f；Ae/As 取 Table 7，Tact/Teff 取 Table 7 公差锚 —— 缺口显示「—」，可用 ae/as/e2/tactn/teffn/tactw/teffw 覆盖。",
        "Smart card \"DIN 5480 internal/external spline table\" usage (one direction per card): `OCSMCARD DIN花键参数表 <nine-field expression> [N<grade><letter>] [e2 …] [ae …] [as …] [tactn …] [teffn …] [at x,y]` (external card: `OCSMCARD DIN花键参数表_外 <nine-field expression> [W<grade><letter>] [tactw …] [teffw …] …`; the old form `M3 Z38 B120 N9H W8f` still works: the old id defaults to the internal card, accepts both N/W and uses only this side). The expression is back-solved to d_B=m(z+1.1+2x), m, z (DIN 5480 pressure angle is always 30°), and KIND must match the card direction (IN for internal / EX for external). Layout = DIN 5480-1:2006 Bild 6 single-column 13 rows (internal = Nabe, external = Welle), the whole INSERT is scaled 0.17; hole default 9H, shaft default 8f; Ae/As come from Table 7 and Tact/Teff from the Table 7 tolerance anchors — gaps show \"—\" and can be overridden with ae/as/e2/tactn/teffn/tactw/teffw.",
    ),
    Msg::new(
        "cmd.din.echo.anchor",
        "，Bild 6 示例原印值",
        ", Bild 6 example printed values",
    ),
    Msg::new(
        "cmd.din.echo",
        "DIN 5480 {designation}（dB={db} m={m} z={z}{anchor}）",
        "DIN 5480 {designation} (dB={db} m={m} z={z}{anchor})",
    ),

    // ── 命令输出/报错：精简卡引擎（spline_lite / card_lite）──
    Msg::new(
        "cmd.splinelite.err.side_mismatch",
        "GB 花键精简卡：方向与计算结果不一致",
        "GB spline lite card: direction does not match the computed result",
    ),
    Msg::new(
        "cmd.cardlite.err.serialize",
        "{id}：请求序列化失败：{e}",
        "{id}: failed to serialize the request: {e}",
    ),
    Msg::new(
        "cmd.cardlite.err.preview",
        "{id}：{e}",
        "{id}: {e}",
    ),
    Msg::new(
        "cmd.cardlite.err.preview_parse",
        "{id}：完整卡预览 JSON 解析失败：{e}",
        "{id}: failed to parse the full-card preview JSON: {e}",
    ),
    Msg::new(
        "cmd.cardlite.err.gear_model",
        "齿轮精简卡：请求字段无效：{e}",
        "Gear lite card: invalid request field: {e}",
    ),
    Msg::new(
        "cmd.cardlite.err.dir_mismatch",
        "{id}：方向记号与卡片方向不一致（卡片固定{side}）",
        "{id}: direction token conflicts with the card direction (card is fixed to {side})",
    ),
    Msg::new(
        "cmd.cardlite.err.extra_missing",
        "{id}：补算项 {tag} 缺失",
        "{id}: computed extra {tag} is missing",
    ),
    Msg::new(
        "cmd.cardlite.err.full_item_missing",
        "{id}：完整卡项「{key}」不在取值表里",
        "{id}: full-card item \"{key}\" is not in the value table",
    ),
    Msg::new(
        "cmd.cardlite.err.preview_missing",
        "{id}：预览缺项 {tag}",
        "{id}: preview is missing item {tag}",
    ),

    // ── 命令输出/报错：OCSMCARD 用法 + 卡导出回执（card.rs / guide_server / spline_gui）──
    Msg::new(
        "cmd.card.label.spline",
        "花键参数表",
        "spline table",
    ),
    Msg::new(
        "cmd.card.export.inserted",
        "已插入{kind}参数表（{echo}）于 ({x}, {y}) rot {rot}°。",
        "Inserted {kind} parameter table ({echo}) at ({x}, {y}) rot {rot}°.",
    ),
    Msg::new(
        "cmd.card.export.pending_note",
        "{kind}参数表（{echo}）",
        "{kind} parameter table ({echo})",
    ),
    Msg::new(
        "cmd.card.export.pending",
        "已生成{kind}参数表（{echo}）：切回图纸，鼠标上已带这张表，左键点击定位基点 → 移动光标旋转 → 再点击落定（可连续，Esc 结束）。",
        "Generated {kind} parameter table ({echo}): switch back to the drawing; the table is attached to the cursor. Click to set the base point → move to rotate → click again to place (repeatable; Esc to finish).",
    ),
    Msg::new(
        "cmd.card.export.inserted_card",
        "已插入{kind}（{echo}）于 ({x}, {y}) rot {rot}°。",
        "Inserted {kind} ({echo}) at ({x}, {y}) rot {rot}°.",
    ),
    Msg::new(
        "cmd.card.export.pending_note_card",
        "{kind}（{echo}）",
        "{kind} ({echo})",
    ),
    Msg::new(
        "cmd.card.export.pending_card",
        "已生成{kind}（{echo}）：切回图纸，鼠标上已带这张表，左键点击定位基点 → 移动光标旋转 → 再点击落定（可连续，Esc 结束）。",
        "Generated {kind} ({echo}): switch back to the drawing; the table is attached to the cursor. Click to set the base point → move to rotate → click again to place (repeatable; Esc to finish).",
    ),
    Msg::new(
        "cmd.spline.echo.dp",
        "；量棒 Dp={dp}（{mode}，D'={dp_calc}，备选 {c1} / {c2} / {c3}）",
        "; pin Dp={dp} ({mode}, D'={dp_calc}, candidates {c1} / {c2} / {c3})",
    ),
    Msg::new(
        "cmd.spline.echo.dp_manual",
        "手填，Md 已重算",
        "manual entry, Md recomputed",
    ),
    Msg::new(
        "cmd.spline.echo.dp_auto",
        "标准 R40 自动选",
        "auto-picked per standard R40",
    ),

    // ── 命令输出/报错：轴生成器命令行解析（shaft.rs）──
    Msg::new(
        "cmd.shaft.label.multi",
        "第 {line} 行第 {chunk} 段",
        "line {line}, segment {chunk}",
    ),
    Msg::new(
        "cmd.shaft.label.single",
        "第 {line} 行（第 {chunk} 段）",
        "line {line} (segment {chunk})",
    ),
    Msg::new(
        "cmd.shaft.err.unknown_keyword",
        "{label}：不识别的关键字「{token}」（本期支持 S/E/L/CH/OV/M/TL/RO/SD/RL/GEAR/SPLINE/KEY/VIEW；齿形段子关键字 M/Z/H/BETA/ALPHA/EX/IN/X/DA/DF（SPLINE + 齿形关键字 = 渐开线花键）；RL 的尺寸参数 P/g1/g2/dg/r 跟在 RL 后面；轴槽子关键字 = 键型 A/B/C + 键尺寸 b…/h…（可连写 `b8h7`）+ 键长（裸数字或 KL…）+ @中/@端（可选 t1）+ 导向（GB/T 1097，可选 `双槽`））",
        "{label}: unrecognized keyword \"{token}\" (supported: S/E/L/CH/OV/M/TL/RO/SD/RL/GEAR/SPLINE/KEY/VIEW; tooth-profile sub-keywords M/Z/H/BETA/ALPHA/EX/IN/X/DA/DF (SPLINE + tooth keywords = involute spline); RL size parameters P/g1/g2/dg/r follow RL; shaft-key sub-keywords = key style A/B/C + key size b…/h… (may be joined as `b8h7`) + key length (bare number or KL…) + @中/@端 (optional t1) + guided (GB/T 1097, optional `双槽`))",
    ),
    Msg::new(
        "cmd.shaft.err.view_unknown",
        "视图名无法识别：`{name}`。可用：normal|常规、section|剖视。",
        "unrecognized view name: `{name}`. Available: normal|常规, section|剖视.",
    ),
    Msg::new(
        "cmd.shaft.err.view_missing",
        "{label}：关键字 VIEW 缺少视图名（常规/剖视）",
        "{label}: keyword VIEW is missing a view name (常规/剖视)",
    ),
    Msg::new(
        "cmd.shaft.err.view_conflict",
        "{label}：视图 VIEW 重复且冲突（已给「{prev}」，又给「{now}」）",
        "{label}: VIEW is repeated with a conflict (already \"{prev}\", now \"{now}\")",
    ),
    Msg::new(
        "cmd.shaft.err.tooth_radii_pos",
        "{label}：齿形段的大径 DA={da}、小径 DF={df} 必须是正数",
        "{label}: tooth-profile segment major DA={da} and minor DF={df} must be positive",
    ),
    Msg::new(
        "cmd.shaft.err.tooth_radii_order",
        "{label}：齿形段的大径 DA={da} 必须大于小径 DF={df}（外齿：DA=齿顶圆；内齿：DA=外侧齿根）",
        "{label}: tooth-profile segment major DA={da} must exceed minor DF={df} (external: DA=tip circle; internal: DA=outer root)",
    ),
    Msg::new(
        "cmd.shaft.err.end_missing",
        "{label}：关键字 {what} 的端别缺省（@ 后要 L 或 R）",
        "{label}: keyword {what} is missing an end (after @ use L or R)",
    ),
    Msg::new(
        "cmd.shaft.err.end_bad",
        "{label}：端别「{bad}」非法（只能用 L 或 R）",
        "{label}: invalid end \"{bad}\" (only L or R)",
    ),
    Msg::new(
        "cmd.shaft.err.key_value_not_num",
        "{label}：关键字 {what} 的值「{value}」不是数字",
        "{label}: value \"{value}\" of keyword {what} is not a number",
    ),
    Msg::new(
        "cmd.shaft.err.key_no_value",
        "{label}：关键字 {what} 缺少数值",
        "{label}: keyword {what} is missing a value",
    ),
    Msg::new(
        "cmd.shaft.err.ch_no_value",
        "{label}：关键字 CH 缺少数值（写法 CH2 / CH2@L / CH2@R）",
        "{label}: keyword CH is missing a value (write CH2 / CH2@L / CH2@R)",
    ),
    Msg::new(
        "cmd.shaft.err.ch_positive",
        "{label}：关键字 CH 的 C={c} 非法（必须 > 0）",
        "{label}: keyword CH C={c} is invalid (must be > 0)",
    ),
    Msg::new(
        "cmd.shaft.err.ov_not_num",
        "{label}：关键字 OV 的值「{value}」不是数字",
        "{label}: value \"{value}\" of keyword OV is not a number",
    ),
    Msg::new(
        "cmd.shaft.err.ov_positive",
        "{label}：关键字 OV 的 b1={b1} 非法（必须 > 0）",
        "{label}: keyword OV b1={b1} is invalid (must be > 0)",
    ),
    Msg::new(
        "cmd.shaft.err.es_cancelled",
        "{label}：`ES` 已取消 —— 退刀槽请用段级 `RL`（例 `S25 E25 L32 RL@L P1.5`）或一小段小直径轴段表示，例如 `S24 E24 L5`",
        "{label}: `ES` was removed — use the segment-level `RL` for relief grooves (e.g. `S25 E25 L32 RL@L P1.5`), or a short small-diameter segment such as `S24 E24 L5`",
    ),
    Msg::new(
        "cmd.shaft.err.m_pitch",
        "{label}：关键字 M 的螺距 P={pitch} 非法（必须 > 0；不写值 = 小径 0.85d 简化画法）",
        "{label}: pitch P={pitch} of keyword M is invalid (must be > 0; omit the value to draw the simplified 0.85d minor diameter)",
    ),
    Msg::new(
        "cmd.shaft.err.ro_invalid_t3",
        "{label}：收尾档位 RO「{other}」非法（GB/T 3 表 1 只有 一般 / 短，没有 长）",
        "{label}: invalid run-out class RO \"{other}\" (GB/T 3 Table 1 only has 一般/normal or 短/short, not 长/long)",
    ),
    Msg::new(
        "cmd.shaft.err.sd_invalid_t3",
        "{label}：肩距档位 SD「{other}」非法（GB/T 3 表 1 只有 一般 / 长 / 短）",
        "{label}: invalid shoulder-distance class SD \"{other}\" (GB/T 3 Table 1 only has 一般/normal, 长/long, 短/short)",
    ),
    Msg::new(
        "cmd.shaft.err.tl_no_value",
        "{label}：关键字 TL 缺少数值（写法 TL20）",
        "{label}: keyword TL is missing a value (write TL20)",
    ),
    Msg::new(
        "cmd.shaft.err.tl_positive",
        "{label}：完整螺纹长度 TL={value} 非法（必须 > 0；不给 TL = 整段全线程）",
        "{label}: full thread length TL={value} is invalid (must be > 0; omit TL for a fully threaded segment)",
    ),
    Msg::new(
        "cmd.shaft.err.rl_missing_params",
        "{label}：RL 退刀槽缺参 —— 表 2 以螺距为键，请给 P（例 `RL@L P1.5`）或显式 g1/g2/dg/r（例 `RL@L g1 2.5 g2 4.5 dg 22.7 r 0.8`）",
        "{label}: RL relief groove is missing parameters — Table 2 is keyed by pitch, so supply P (e.g. `RL@L P1.5`) or explicit g1/g2/dg/r (e.g. `RL@L g1 2.5 g2 4.5 dg 22.7 r 0.8`)",
    ),
    Msg::new(
        "cmd.shaft.err.key_no_type",
        "{label}：KEY 缺少键型（A/B/C；例 `KEY A 18`）",
        "{label}: KEY is missing the key style (A/B/C; e.g. `KEY A 18`)",
    ),
    Msg::new(
        "cmd.shaft.err.key_dup",
        "{label}：关键字 {what} 重复",
        "{label}: keyword {what} is repeated",
    ),
    Msg::new(
        "cmd.shaft.err.key_1097_no_c",
        "{label}：导向平键（GB/T 1097）只有 A/B 型，没有 C 型（用户 2026-09-25 更正）",
        "{label}: guided parallel keys (GB/T 1097) only have types A/B, not C (corrected 2026-09-25)",
    ),
    Msg::new(
        "cmd.shaft.err.key_no_place",
        "{label}：{kind}只有{places}（{tag}）",
        "{label}: {kind} only has {places} ({tag})",
    ),
    Msg::new(
        "cmd.shaft.err.key_no_double",
        "{label}：{kind}不支持双槽（固定键只有一个槽）",
        "{label}: {kind} does not support double keyways (a fixed key has only one slot)",
    ),
    Msg::new(
        "cmd.shaft.err.key_1097_b",
        "{label}：GB/T 1097 导向平键表里没有 b={b}（b=8…45，14 档）",
        "{label}: the GB/T 1097 guided-key table has no b={b} (b=8…45, 14 steps)",
    ),
    Msg::new(
        "cmd.shaft.err.key_1096_b",
        "{label}：{kind} 的平键族表里没有 b={b}（GB/T 1096 表 b=2…50）",
        "{label}: {kind} parallel-key table has no b={b} (GB/T 1096 table b=2…50)",
    ),
    Msg::new(
        "cmd.shaft.err.key_bh_pair",
        "{label}：键尺寸 b{b}×h{h} 不是标准配对（{kind} 的 b={b} 应配 h={std_h}）；h 跟 b 走，不能自由组合",
        "{label}: key size b{b}×h{h} is not a standard pair ({kind} b={b} should pair with h={std_h}); h follows b and cannot be combined freely",
    ),
    Msg::new(
        "cmd.shaft.err.key_no_len",
        "{label}：KEY 缺少键长（写法：裸数字 `18` 或 `KL18`；`L…` 是段长）",
        "{label}: KEY is missing the key length (bare number `18` or `KL18`; `L…` is the segment length)",
    ),
    Msg::new(
        "cmd.shaft.err.key_la_lc",
        "{label}：KEY 的 LA/LC 是旧「槽长」写法（已改口径，不静默兼容）—— 现在按 键型 + 键长 L + 位置：中置 `KEY A 18`；端置 `KEY C 14 @端`（端置槽长 = 键长 + t1，即模板 LC 口径）",
        "{label}: KEY LA/LC is the old \"slot length\" form (removed without silent compatibility) — now use key style + key length L + position: centered `KEY A 18`; end `KEY C 14 @端` (end slot length = key length + t1, the template LC convention)",
    ),
    Msg::new(
        "cmd.shaft.err.key_param_no_value",
        "{label}：KEY 参数 {name} 缺少数值",
        "{label}: KEY parameter {name} is missing a value",
    ),
    Msg::new(
        "cmd.shaft.err.key_param_dup",
        "{label}：KEY 参数 {what} 重复",
        "{label}: KEY parameter {what} is repeated",
    ),
    Msg::new(
        "cmd.shaft.err.key_place_conflict",
        "{label}：KEY 位置重复且冲突（已给「{prev}」，又给「{now}」）",
        "{label}: KEY position is repeated with a conflict (already \"{prev}\", now \"{now}\")",
    ),
    Msg::new(
        "cmd.shaft.err.key_type_conflict",
        "{label}：KEY 键型重复且冲突（已给「{prev}」，又给「{now}」）",
        "{label}: KEY style is repeated with a conflict (already \"{prev}\", now \"{now}\")",
    ),
    Msg::new(
        "cmd.shaft.err.key_type_invalid",
        "{label}：KEY 键型「{value}」非法（可选：{options}）",
        "{label}: invalid KEY style \"{value}\" (available: {options})",
    ),
    Msg::new(
        "cmd.shaft.err.key_guide_conflict",
        "{label}：键型「{kind}」与「导向」冲突（该键型不是导向平键）",
        "{label}: key style \"{kind}\" conflicts with \"导向\" (that style is not a guided parallel key)",
    ),
    Msg::new(
        "cmd.shaft.err.key_param_positive",
        "{label}：KEY 参数 {what}={value} 必须 > 0",
        "{label}: KEY parameter {what}={value} must be > 0",
    ),
    Msg::new(
        "cmd.shaft.err.ch_dup_end",
        "{label}：关键字 CH 在{end}端重复",
        "{label}: keyword CH is repeated at the {end} end",
    ),
    Msg::new(
        "cmd.shaft.err.ov_dup_end",
        "{label}：关键字 OV 在{end}端重复",
        "{label}: keyword OV is repeated at the {end} end",
    ),
    Msg::new(
        "cmd.shaft.err.ro_invalid",
        "{label}：收尾档位 RO「{rest}」非法（只有 一般 / 短）",
        "{label}: invalid run-out class RO \"{rest}\" (only 一般/normal or 短/short)",
    ),
    Msg::new(
        "cmd.shaft.err.sd_invalid",
        "{label}：肩距档位 SD「{rest}」非法（只有 一般 / 长 / 短）",
        "{label}: invalid shoulder-distance class SD \"{rest}\" (only 一般/normal, 长/long, 短/short)",
    ),
    Msg::new(
        "cmd.shaft.err.rl_no_value",
        "{label}：关键字 RL 不带值（写法 RL / RL@L / RL@R；尺寸参数跟在 RL 后面，如 `RL@L P1.5`）",
        "{label}: keyword RL takes no value (write RL / RL@L / RL@R; size parameters follow RL, e.g. `RL@L P1.5`)",
    ),
    Msg::new(
        "cmd.shaft.err.rl_dup_end",
        "{label}：关键字 RL 重复（在{end}端）",
        "{label}: keyword RL is repeated (at the {end} end)",
    ),
    Msg::new(
        "cmd.shaft.err.rl_param_order",
        "{label}：参数 {key} 要跟在 RL 后面（例 `RL@L P1.5` / `RL@L g1 2.5 …`）",
        "{label}: parameter {key} must follow RL (e.g. `RL@L P1.5` / `RL@L g1 2.5 …`)",
    ),
    Msg::new(
        "cmd.shaft.err.rl_param_no_value",
        "{label}：RL 参数 {key} 缺少数值",
        "{label}: RL parameter {key} is missing a value",
    ),
    Msg::new(
        "cmd.shaft.err.rl_param_positive",
        "{label}：RL 参数 {key}={value} 非法（必须 > 0）",
        "{label}: RL parameter {key}={value} is invalid (must be > 0)",
    ),
    Msg::new(
        "cmd.shaft.err.z_not_int",
        "{label}：关键字 Z 的值「{rest}」不是正整数（齿数 z 必须是整数）",
        "{label}: value \"{rest}\" of keyword Z is not a positive integer (tooth count z must be an integer)",
    ),
    Msg::new(
        "cmd.shaft.err.combo_spline_ch",
        "{label}：花键段不能与倒角 CH 同段（不自动画引入倒角；请在相邻轴段上写 CH）",
        "{label}: a spline segment cannot share a segment with chamfer CH (no lead-in chamfer is added automatically; write CH on a neighbouring segment)",
    ),
    Msg::new(
        "cmd.shaft.err.combo_spline_ov",
        "{label}：花键段不能与越程槽 OV 同段",
        "{label}: a spline segment cannot share a segment with an overtravel groove OV",
    ),
    Msg::new(
        "cmd.shaft.err.combo_spline_rl",
        "{label}：花键段不能与退刀槽 RL 同段",
        "{label}: a spline segment cannot share a segment with a relief groove RL",
    ),
    Msg::new(
        "cmd.shaft.err.combo_spline_m",
        "{label}：花键段不能与螺纹段 M/TL/RO/SD 同段",
        "{label}: a spline segment cannot share a segment with thread M/TL/RO/SD",
    ),
    Msg::new(
        "cmd.shaft.err.spline_no_se",
        "{label}：花键段不给 S/E（直径由规格代号导出）",
        "{label}: a spline segment takes no S/E (diameters come from the spec code)",
    ),
    Msg::new(
        "cmd.shaft.err.spline_no_spec",
        "{label}：关键字 SPLINE 缺少规格代号（写法 SPLINE 6x23x26x6 L30）",
        "{label}: keyword SPLINE is missing the spec code (write SPLINE 6x23x26x6 L30)",
    ),
    Msg::new(
        "cmd.shaft.err.spline_no_len",
        "{label}：花键段缺少 L（满齿段长，例 `SPLINE 6x23x26x6 L30`）",
        "{label}: spline segment is missing L (full-tooth segment length, e.g. `SPLINE 6x23x26x6 L30`)",
    ),
    Msg::new(
        "cmd.shaft.err.tlro_needs_m",
        "{label}：关键字 TL/RO/SD 是螺纹参数，需要与 M 同段（例 `M1.5 TL20 RO短 SD长`）",
        "{label}: TL/RO/SD are thread parameters and must share a segment with M (e.g. `M1.5 TL20 RO短 SD长`)",
    ),
    Msg::new(
        "cmd.shaft.err.m_rl_override",
        "{label}：M 段右端的 RL（螺纹收尾）按螺距查表，不支持 P/g1/g2/dg/r 覆盖；段级退刀槽请写在别的圆柱段上（或 `M…RL@L`）",
        "{label}: RL on the right of an M segment (thread run-out) is looked up by pitch and does not accept P/g1/g2/dg/r overrides; put a segment-level relief groove on another cylindrical segment (or use `M…RL@L`)",
    ),
    Msg::new(
        "cmd.shaft.err.rl_ro_exclusive",
        "{label}：RL（表 2 退刀槽收尾）与 RO/SD（螺尾/肩距）互斥，二选一",
        "{label}: RL (Table 2 relief-groove run-out) and RO/SD (thread run-out/shoulder distance) are mutually exclusive; pick one",
    ),
    Msg::new(
        "cmd.shaft.err.m_ov_conflict",
        "{label}：螺纹段 M 不能与越程槽 OV 同段",
        "{label}: a thread segment M cannot share a segment with an overtravel groove OV",
    ),
    Msg::new(
        "cmd.shaft.err.key_mrl_conflict",
        "{label}：轴槽 KEY 不能与螺纹 M / 越程槽 OV / 退刀槽 RL 同段（只能挂在光圆柱段上）",
        "{label}: a shaft key KEY cannot share a segment with thread M / overtravel groove OV / relief groove RL (it can only sit on a plain cylindrical segment)",
    ),
    Msg::new(
        "cmd.shaft.err.missing_s",
        "{label}：缺少 S（起始直径）",
        "{label}: missing S (start diameter)",
    ),
    Msg::new(
        "cmd.shaft.err.missing_l",
        "{label}：缺少 L（段长）",
        "{label}: missing L (segment length)",
    ),
    Msg::new(
        "cmd.shaft.err.gear_no_se",
        "{label}：齿轮段不给 S/E（直径由 M·Z 导出）",
        "{label}: a gear segment takes no S/E (diameters come from M·Z)",
    ),
    Msg::new(
        "cmd.shaft.err.gear_use_h",
        "{label}：齿轮段长度用 H（省略 = 10m），不要再给 L",
        "{label}: use H for gear-segment length (default 10m); do not give L",
    ),
    Msg::new(
        "cmd.shaft.err.gear_no_ch",
        "{label}：齿轮段不能与倒角 CH 同段（齿形用 OCSMGEAR 单独出）",
        "{label}: a gear segment cannot share a segment with chamfer CH (draw the tooth profile separately with OCSMGEAR)",
    ),
    Msg::new(
        "cmd.shaft.err.gear_no_ov",
        "{label}：齿轮段不能与越程槽 OV 同段",
        "{label}: a gear segment cannot share a segment with an overtravel groove OV",
    ),
    Msg::new(
        "cmd.shaft.err.gear_no_m",
        "{label}：齿轮段不能与螺纹段 M 同段",
        "{label}: a gear segment cannot share a segment with thread M",
    ),
    Msg::new(
        "cmd.shaft.err.gear_no_tlro",
        "{label}：齿轮段不能与螺纹参数 TL/RO/SD 同段（TL/RO/SD 只属于 M）",
        "{label}: a gear segment cannot share a segment with TL/RO/SD (those belong to M)",
    ),
    Msg::new(
        "cmd.shaft.err.gear_no_rl",
        "{label}：齿轮段不能与退刀槽 RL 同段",
        "{label}: a gear segment cannot share a segment with a relief groove RL",
    ),
    Msg::new(
        "cmd.shaft.err.gear_no_missing",
        "{label}：关键字 GEAR 缺少 M（模数）",
        "{label}: keyword GEAR is missing M (module)",
    ),
    Msg::new(
        "cmd.shaft.err.gear_no_z",
        "{label}：关键字 GEAR 缺少 Z（齿数）",
        "{label}: keyword GEAR is missing Z (tooth count)",
    ),
    Msg::new(
        "cmd.shaft.err.beta_spur",
        "{label}：关键字 BETA 的斜齿（β={beta}°）本期只做直齿（斜齿未实现）",
        "{label}: the BETA helical value (β={beta}°) is backed by spur gears only in this release (helical not implemented)",
    ),
    Msg::new(
        "cmd.shaft.err.gear_seg",
        "{label}：齿轮段：{e}",
        "{label}: gear segment: {e}",
    ),
    Msg::new(
        "cmd.shaft.err.place_dup",
        "{label}：放置参数 at/rot 重复",
        "{label}: placement parameter at/rot is repeated",
    ),
    Msg::new(
        "cmd.shaft.err.at_no_y",
        "{label}：at 缺 y 坐标（写 at x,y）",
        "{label}: at is missing the y coordinate (write at x,y)",
    ),
    Msg::new(
        "cmd.shaft.err.rot_no_value",
        "{label}：rot 缺少数值",
        "{label}: rot is missing a value",
    ),
    Msg::new(
        "cmd.shaft.err.place_unknown",
        "{label}：无法识别的词「{word}」（放置段只认 at x,y rot 度）",
        "{label}: unrecognized word \"{word}\" (a placement segment only accepts at x,y rot deg)",
    ),
    Msg::new(
        "cmd.shaft.err.prefix",
        "{label}：{e}",
        "{label}: {e}",
    ),
    Msg::new(
        "cmd.shaft.err.field_not_number",
        "{label}：{what}「{text}」不是数字",
        "{label}: {what} \"{text}\" is not a number",
    ),
    Msg::new(
        "cmd.shaft.err.key_place_invalid",
        "KEY 位置「{other}」非法（只有 @中 / @端，或 mid / end）",
        "invalid KEY position \"{other}\" (only @中/@端, or mid/end)",
    ),
    Msg::new(
        "cmd.shaft.err.key_kind_invalid",
        "KEY 键型「{other}」非法（只有 A / B / C；对应 GB/T 1096 平键族 A/B/C 型）",
        "invalid KEY style \"{other}\" (only A / B / C; the GB/T 1096 parallel-key types A/B/C)",
    ),
    Msg::new(
        "cmd.shaft.err.gb1095_no_d",
        "轴径 d={d} 不在 GB/T 1095 的 d 选型表（6…500）里，无法按轴径确定键尺寸 b×h",
        "shaft diameter d={d} is not in the GB/T 1095 d selection table (6…500), so the key size b×h cannot be determined from the shaft diameter",
    ),
    Msg::new(
        "cmd.shaft.err.gb1095_b_out",
        "轴径 d={d} 按 GB/T 1095 应配 b={b}，但超出平键族表范围（GB/T 1096 表 b=2…50）",
        "shaft diameter d={d} should pair with b={b} per GB/T 1095, but that is outside the parallel-key table (GB/T 1096 table b=2…50)",
    ),
    Msg::new(
        "cmd.shaft.err.csv_header_missing",
        "keyway_gb1095.csv 缺表头",
        "keyway_gb1095.csv is missing its header",
    ),
    Msg::new(
        "cmd.shaft.err.csv_header_bad",
        "keyway_gb1095.csv 表头异常：{header}",
        "keyway_gb1095.csv: unexpected header: {header}",
    ),
    Msg::new(
        "cmd.shaft.err.csv_missing_col",
        "keyway_gb1095.csv 缺列 {what}",
        "keyway_gb1095.csv is missing column {what}",
    ),
    Msg::new(
        "cmd.shaft.err.csv_col_num",
        "keyway_gb1095.csv 第 {i} 列 {what} 不是数字：{e}",
        "keyway_gb1095.csv column {i} ({what}) is not a number: {e}",
    ),
    Msg::new(
        "cmd.shaft.err.gb1095_table_prefix",
        "GB/T 1095 表：{e}",
        "GB/T 1095 table: {e}",
    ),
    Msg::new(
        "cmd.shaft.err.gb1095_no_b",
        "GB/T 1095 表里没有 b={b} 这一档（标准 b×h 档：2×2…100×50）",
        "the GB/T 1095 table has no b={b} step (standard b×h steps: 2×2…100×50)",
    ),
    Msg::new(
        "cmd.shaft.err.no_segments",
        "没有解析到任何轴段（至少给一段 `S… L…`）",
        "no shaft segments parsed (give at least one `S… L…` segment)",
    ),
    Msg::new(
        "cmd.shaft.err.rl_param_dup",
        "{label}：RL 参数 {key} 重复",
        "{label}: RL parameter {key} is repeated",
    ),
    Msg::new(
        "cmd.shaft.err.spline_de_dup",
        "{label}：花键的 de 覆盖重复",
        "{label}: spline de override is repeated",
    ),
    Msg::new(
        "cmd.shaft.err.key_gear_conflict",
        "{label}：轴槽 KEY 不能与齿轮/花键段同段",
        "{label}: a shaft key KEY cannot share a segment with a gear/spline segment",
    ),
    Msg::new(
        "cmd.shaft.err.key_1097_screw",
        "{label}：GB/T 1097 固定螺钉 M{d0}：{e}",
        "{label}: GB/T 1097 set screw M{d0}: {e}",
    ),
    Msg::new(
        "cmd.shaft.err.key_end_place_only",
        "{label}：端置轴槽只能开在首段（左端）或末段（右端）",
        "{label}: an end-placed shaft key can only be cut on the first (left) or last (right) segment",
    ),
    Msg::new(
        "cmd.shaft.err.m_gear_dup",
        "{label}：关键字 M 重复（GEAR 段里的 M 是模数；螺纹 M 不能与 GEAR 同段）",
        "{label}: keyword M is repeated (M in a GEAR segment is the module; a thread M cannot share a segment with GEAR)",
    ),
    Msg::new(
        "cmd.shaft.err.no_service",
        "OCSMSHAFT: 无法启动轴服务（宿主不支持 worker 请求）。",
        "OCSMSHAFT: cannot start the shaft service (host does not support worker requests).",
    ),
    Msg::new(
        "cmd.shaft.info.run_init",
        "OCSMSHAFT：先运行 OCSM 初始化，再打开轴生成器窗口。",
        "OCSMSHAFT: run the OCSM initializer first, then open the shaft generator window.",
    ),
    Msg::new(
        "cmd.shaft.info.run_init_direct",
        "OCSMSHAFT：先运行 OCSM（或点功能区「图幅」组里的 OCSM 初始化），再直接插轴。",
        "OCSMSHAFT: run OCSM first (or click the OCSM initializer in the \"图幅\" ribbon group), then insert the shaft directly.",
    ),
    Msg::new(
        "cmd.shaft.info.opened",
        "OCSM 轴生成器：已打开窗口（段表 + 行文本双向同步 + 实时预览）。点「生成到图纸」→ 回到图纸点击定位基点 → 移动光标旋转 → 再点击落定（可连续，Esc 结束）。",
        "OCSM shaft generator: window opened (segment table + two-way text sync + live preview). Click \"Generate to drawing\" → click in the drawing to set the base point → move to rotate → click again to place (repeatable; Esc to finish).",
    ),
    Msg::new(
        "cmd.shaft.info.already_open",
        "OCSM 轴生成器：窗口已打开（Alt+Tab 切换过去）。",
        "OCSM shaft generator: window is already open (switch to it with Alt+Tab).",
    ),
    Msg::new("cmd.shaft.place.what", "OCSM 轴", "OCSM shaft"),
    Msg::new(
        "cmd.shaft.place.where",
        "请在轴生成器窗口里点「生成到图纸」",
        "click \"Generate to drawing\" in the shaft generator window",
    ),
    Msg::new(
        "cmd.shaft.err.params_invalid",
        "OCSMSHAFT 参数无效：{message}",
        "OCSMSHAFT: invalid arguments: {message}",
    ),
    Msg::new(
        "cmd.shaft.info.report_written",
        "OCSMSHAFT：计算书已写入 {path}",
        "OCSMSHAFT: report written to {path}",
    ),
    Msg::new(
        "cmd.shaft.err.report_write",
        "OCSMSHAFT 计算书写文件失败：{e}",
        "OCSMSHAFT: failed to write the report file: {e}",
    ),
    Msg::new(
        "cmd.shaft.err.report",
        "OCSMSHAFT 计算书：{e}",
        "OCSMSHAFT report: {e}",
    ),
    Msg::new(
        "cmd.shaft.err.geometry",
        "OCSMSHAFT 几何非法：{message}",
        "OCSMSHAFT: invalid geometry: {message}",
    ),
    Msg::new("cmd.shaft.undo", "OCSMSHAFT 轴", "OCSMSHAFT shaft"),
    Msg::new(
        "cmd.shaft.info.generated",
        "OCSMSHAFT：已生成轴（{segments} 段，总长 {length}，最大 Ø{max}，{view} 视图，{count} 个图元）→ {layers}",
        "OCSMSHAFT: shaft generated ({segments} segments, total length {length}, max Ø{max}, {view} view, {count} entities) → {layers}",
    ),
    Msg::new("cmd.shaft.view.normal", "常规", "normal"),
    Msg::new("cmd.shaft.view.section", "剖视", "section"),
    Msg::new(
        "cmd.shaft.ready.missing_layers",
        "这张图还没跑过 OCSM 初始化（缺图层：{list}）—— 先执行 OCSM（建 10 个图层 + 线型 + 文字/标注样式），再生成轴；不然中心线会是实线白线。",
        "this drawing has not been through OCSM initialization (missing layers: {list}) — run OCSM first (it creates the 10 layers + linetypes + text/dimension styles), otherwise the centerline will be a solid white line.",
    ),
    Msg::new(
        "cmd.shaft.ready.center_linetype",
        "这张图还没跑过 OCSM 初始化（{layer} 没挂 CENTER2 点划线）—— 先执行 OCSM（建 10 个图层 + 线型 + 文字/标注样式），再生成轴；不然中心线会是实线白线。",
        "this drawing has not been through OCSM initialization ({layer} does not use the CENTER2 dash-dot linetype) — run OCSM first (it creates the 10 layers + linetypes + text/dimension styles), otherwise the centerline will be a solid white line.",
    ),
    Msg::new(
        "cmd.card.usage_line",
        "OCSMCARD 用法：`OCSMCARD <卡类型> …`（本期卡类型：{types}；不带参数 = 开图形界面）。\n* GB 花键参数表（内/外两张卡，一卡一方向）：`OCSMCARD 花键参数表 [std GB] <九字段表达式> [6H|5f] [dp 4.5] [root 平|圆] [at x,y] [rot 度]`（旧 `内/外` 记号兼容；外卡表下方 `花键参数表_外`）\n* 齿轮参数表：`OCSMCARD 齿轮参数表 <九字段表达式> [mate z₂] [dwg 图号] [grade 精度等级] [center a] [at x,y] [rot 度]`\n* ANSI 花键参数表（内/外 × 中/英 = 四张卡）：`OCSMCARD ANSI花键参数表_中文 <九字段表达式> [profile ANSI30R] [at x,y]`（外卡 `ANSI花键参数表_外_中文`；英文版换 `_英文`；旧写法 `内 P16 Z20` 兼容）\n* NF 内/外花键参数表（两张）：`OCSMCARD NF内花键参数表 <九字段表达式> [中心 外径|齿面] [根 平|圆] [配合 松动|滑动|固定|压] [at x,y]`（外卡 `OCSMCARD NF外花键参数表`；旧写法 `A300 M7.5 Z38` 兼容）\n* DIN 5480 内/外花键参数表（两张，单栏 Bild 6）：`OCSMCARD DIN花键参数表 <九字段表达式> [N9H] [ae …] [as …] [at x,y]`（外卡 `DIN花键参数表_外`；旧写法 `M3 Z38 B120 N9H W8f` 兼容）\n* 精简版（下拉「精简版」分组，11 张：GB/NF/DIN/ANSI×4/齿轮）：只列基本参数 + 主要测量量、**不含公差列**；参数/查表/公式与完整卡同源 —— `OCSMCARD <精简卡名> <对应完整卡参数>`（例 `OCSMCARD NF花键精简表_内 A300 M7.5 Z38`、`OCSMCARD 齿轮精简表 <九字段表达式>`）",
        "OCSMCARD usage: `OCSMCARD <card type> …` (card types for this release: {types}; no argument = open the GUI).\n* GB spline table (internal/external cards, one direction per card): `OCSMCARD 花键参数表 [std GB] <nine-field expression> [6H|5f] [dp 4.5] [root 平|圆] [at x,y] [rot deg]` (the old `内/外` token still works; the external card is `花键参数表_外` below the table)\n* Gear table: `OCSMCARD 齿轮参数表 <nine-field expression> [mate z₂] [dwg drawing-no] [grade precision-grade] [center a] [at x,y] [rot deg]`\n* ANSI spline table (internal/external × CN/EN = four cards): `OCSMCARD ANSI花键参数表_中文 <nine-field expression> [profile ANSI30R] [at x,y]` (external card `ANSI花键参数表_外_中文`; use `_英文` for English; the old `内 P16 Z20` form still works)\n* NF internal/external spline table (two cards): `OCSMCARD NF内花键参数表 <nine-field expression> [中心 外径|齿面] [根 平|圆] [配合 松动|滑动|固定|压] [at x,y]` (external card `OCSMCARD NF外花键参数表`; the old `A300 M7.5 Z38` form still works)\n* DIN 5480 internal/external spline table (two cards, single-column Bild 6): `OCSMCARD DIN花键参数表 <nine-field expression> [N9H] [ae …] [as …] [at x,y]` (external card `DIN花键参数表_外`; the old `M3 Z38 B120 N9H W8f` form still works)\n* Lite cards (the \"Lite\" group in the dropdown, 11 cards: GB/NF/DIN/ANSI×4/gear): only basic parameters + main measurements, **no tolerance columns**; parameters/lookups/formulas are the same source as the full cards — `OCSMCARD <lite card name> <full-card arguments>` (e.g. `OCSMCARD NF花键精简表_内 A300 M7.5 Z38`, `OCSMCARD 齿轮精简表 <nine-field expression>`)",
    ),
    Msg::new(
        "cmd.shaft.usage",
        "OCSMSHAFT 轴生成器：行 DSL / JSON → 单视图侧视图（段拼接 + 端面倒角 + 砂轮越程槽 + 螺纹段 M + 齿轮段 GEAR + 矩形花键段 SPLINE + 轴槽 KEY）。\n用法：OCSMSHAFT <行 DSL 或 JSON>\n  行 DSL：一行一段，从左到右拼接；多段用 | 或换行分隔；大小写不敏感、段内关键字顺序无关\n    S 起始直径（靠左）   E 终点直径（省略 = 圆柱段 E=S）   L 段长（必给；齿轮段用 H 代替）\n    CH2@L / CH2@R   端面倒角 C2（@ 省略默认 R）    OV / OV3 / OV3@L   砂轮越程槽\n    M / M1.5   螺纹段：不写值 = 小径 0.85d；M1.5 = 螺距 P，小径 = d − 1.0825P（只能圆柱段）\n               不给 TL/RL = 整段全螺纹（旧行为）\n    M1.5 TL20  局部螺纹（GB/T 3-1997 图 1 第一种形式）：完整螺纹长 TL，靠段右端台肩，\n               自右向左 = 台肩面 + 锥面（a−x）+ 螺尾（细实线，x）+ 分界竖线 + 完整螺纹 TL\n               RO一般|RO短 收尾档（默认一般）   SD一般|SD长|SD短 肩距档（默认一般）\n    M1.5 TL20 RL  用 GB/T 3-1997 表 2 退刀槽收尾（图 2 画法，与 RO/SD 互斥）\n    RL@L / RL@R   段级退刀槽（任意圆柱段，@ 省略默认 R；同端只能一个 CH/OV/RL）\n               取参：① g1/g2/dg/r 全给（dg 是绝对直径）② 给 P 查表 2 ③ 都不给报错\n               例：S25 E25 L32 RL@L P1.5 / RL@L g1 2.5 g2 4.5 dg 22.7 r 0.8\n               前置：该端相邻段更高（有台肩）、本段圆柱，否则报「第 N 段」\n    GEAR M5 Z10 H20   齿轮段（直齿）：d=m·z 导出、不给 S/E；H = 齿宽（省略 = 10m）\n                       ALPHA25 = 压力角 25°（省略 20°）；齿顶轮廓两端倒角 C=round(0.6m)\n                       常规不画齿根、剖视画齿根\n    SPLINE 6x23x26x6 L30   矩形花键段（GB/T 1144 规格代号）：大径线 + 小径细线\n                      （2细线层）+ 收尾弧 R=de/2（圆心 (L, ±(d/2+R))，末端 x=L+l，\n                      l=√(h(2R−h))，6×23×26×6 → l=9.6047）；段长 = L + l；\n                      不给 S/E；可 `de 71` 覆盖滚刀外径；不能与 CH/OV/RL/M/GEAR 同段\n                      （引入倒角由相邻段的 CH 表达）\n    KEY A 18      轴槽（GB/T 1095-2003 平键键槽，本期只做轴槽，不毂槽）——轴段类型，进段表 KEY 列：\n                  只能挂在光圆柱段上（与 GEAR/SPLINE/M/TL/RL/OV 互斥）\n                  键型（普通平键 A/B/C；导向平键 `导向A`/`导向B`——GUI 合并在一个下拉里）\n                  + 键长 L（所选键型；平键族标准系列，L<10b）+ 位置中置/端置（槽长自动折算）\n                  b×h 由本段直径 d 查 GB/T 1095 d 列自动定（h 跟 b 走）；t1 按 b 查 GB/T 1095 表\n                  显式覆盖写 b8h7：必须落在该轴径档的标准配对上，否则明确报错\n                  显示：中置恒显示 A；端置 B/C 显示 C（C 端弧由铣刀铣出）\n                  折算：中置 B +b / C +b/2；端置 B +b/2 / C 不折算；端置槽长再 +t1\n                  可选 双槽（DOUBLE 别名）：绕轴心 180° 对置，仅剖视图体现（常规侧视不变），\n                  可承受转矩约为单键联接的 1.5 倍；剖视剖面线上下两环各带一个缺口\n                  例：KEY A 18 ／ KEY C 14 @端 ／ KEY A 18 b8h7\n                  导向平键（GB/T 1097）用合并键型记号（推荐 `KEY 导向A 25`；旧 `KEY A 25 导向` 过时但可用）：\n                  只 A/B；L 取 1097 系列 25…450∩L<10b；槽长 = L（只有中置）；\n                  自动画 2 个固定螺钉螺纹孔 d0×L0、孔心距槽两端 L3；与双槽互斥；起键孔属 1096\n                  侧视图叠画键 + 剖视缺口含 sagitta 线；不生成尺寸标注\n    VIEW 常规|剖视   视图：常规（默认，只看外形）/ 剖视（轮廓 + ANSI31 剖面线）\n                     （双视图已于 2026-09-23 移除；旧 `VIEW 双` 明确报错）\n    REPORT          计算书：段末加 `REPORT`（大小写不敏感）—— 不插图，直接输出 Markdown\n                    计算书（段清单 + 总长/最大直径）；\n                    `REPORT=<路径>` / `REPORT-OUT=<路径>` 另写文件\n    at x,y rot 度   放置（不写 = 原点、不转）\n  例：OCSMSHAFT S30 E30 L45 CH2@L | S40 E40 L30 CH2@R OV3 | S50 E30 L20 | S30 E30 L15 CH2@R | S40 E40 L7 M1.5 | S36 E36 L5 | GEAR M3 Z20 VIEW 剖视 at 100,50 rot 30\n  JSON：{\"segments\":[{\"s\":30,\"e\":30,\"l\":45,\"ch\":[{\"c\":2,\"end\":\"L\"}]},{\"s\":30,\"e\":30,\"l\":20,\"thread\":1.5}],\"view\":\"section\",\"at\":[100,50],\"rot\":30}\n轮廓/端面/倒角/槽与边界竖线 → 1轮廓实线层，螺纹小径/螺尾 → 2细线层（OV 不画砂轮细线），轴线/分度线 → 3中心线层，剖视剖面线 → 5剖面线层。",
        "OCSMSHAFT shaft generator: line DSL / JSON → single-view side view (segment concatenation + end chamfers + grinding overtravel grooves + thread segment M + gear segment GEAR + rectangular spline segment SPLINE + shaft key KEY).\nUsage: OCSMSHAFT <line DSL or JSON>\n  Line DSL: one segment per line, concatenated left to right; separate segments with | or newlines; case-insensitive, keyword order within a segment does not matter\n    S start diameter (left side)   E end diameter (omitted = cylindrical segment with E=S)   L segment length (required; gear segments use H instead)\n    CH2@L / CH2@R   end chamfer C2 (@ omitted defaults to R)    OV / OV3 / OV3@L   grinding overtravel groove\n    M / M1.5   thread segment: no value = minor diameter 0.85d; M1.5 = pitch P, minor diameter = d − 1.0825P (cylindrical segments only)\n               no TL/RL = fully threaded segment (old behaviour)\n    M1.5 TL20   partial thread (GB/T 3-1997 Fig. 1, first form): full thread length TL next to the right shoulder,\n                drawn right to left = shoulder face + taper (a−x) + thread run-out (thin line, x) + boundary line + full thread TL\n               RO一般|RO短 run-out class (default 一般)   SD一般|SD长|SD短 shoulder-distance class (default 一般)\n    M1.5 TL20 RL   use the GB/T 3-1997 Table 2 relief-groove run-out (Fig. 2; mutually exclusive with RO/SD)\n    RL@L / RL@R   segment-level relief groove (any cylindrical segment; @ omitted defaults to R; only one CH/OV/RL per end)\n               parameters: ① give all of g1/g2/dg/r (dg is an absolute diameter) ② give P to look up Table 2 ③ give none → error\n               e.g. S25 E25 L32 RL@L P1.5 / RL@L g1 2.5 g2 4.5 dg 22.7 r 0.8\n               precondition: the neighbouring segment on that end is larger (there is a shoulder) and this segment is cylindrical, otherwise it reports \"segment N\"\n    GEAR M5 Z10 H20   gear segment (spur): d=m·z derived, no S/E; H = face width (omitted = 10m)\n                       ALPHA25 = pressure angle 25° (omitted 20°); both ends of the tip profile get a chamfer C=round(0.6m)\n                       normal view omits the root; section view draws the root\n    SPLINE 6x23x26x6 L30   rectangular spline segment (GB/T 1144 spec code): major-diameter line + thin minor-diameter line\n                      (2细线层) + run-out arc R=de/2 (centre (L, ±(d/2+R)), end x=L+l,\n                      l=√(h(2R−h)), 6×23×26×6 → l=9.6047); segment length = L + l;\n                      no S/E; `de 71` overrides the hob outer diameter; cannot share a segment with CH/OV/RL/M/GEAR\n                      (the lead-in chamfer is expressed by CH on a neighbouring segment)\n    KEY A 18      shaft key (GB/T 1095-2003 parallel-key seat; only shaft keys for now, no hub keys) — a shaft-segment type, shown in the segment table KEY column:\n                  only on plain cylindrical segments (mutually exclusive with GEAR/SPLINE/M/TL/RL/OV)\n                  key style (normal parallel keys A/B/C; guided parallel keys `导向A`/`导向B` — merged into one GUI dropdown)\n                  + key length L (per style; standard parallel-key series, L<10b) + position centred/end (slot length auto-converted)\n                  b×h is determined from this segment diameter d via the GB/T 1095 d column (h follows b); t1 from the GB/T 1095 table by b\n                  explicit override b8h7: must be a standard pair for that shaft diameter, otherwise it reports an error\n                  display: centred always shows A; end B/C shows C (the C end arc is milled)\n                  conversion: centred B +b / C +b/2; end B +b/2 / C no conversion; end slot length adds t1\n                  optional 双槽 (alias DOUBLE): two opposed slots 180° apart, only visible in section (normal side view unchanged),\n                  torque capacity about 1.5× a single key; the section hatching has one gap in each of its two rings\n                  e.g. KEY A 18 / KEY C 14 @端 / KEY A 18 b8h7\n                  guided parallel keys (GB/T 1097) use the merged style token (recommended `KEY 导向A 25`; the old `KEY A 25 导向` is outdated but works):\n                  only A/B; L from the 1097 series 25…450∩L<10b; slot length = L (centred only);\n                  automatically draws two set-screw threaded holes d0×L0 with centres L3 from the slot ends; mutually exclusive with 双槽; the key-removal hole belongs to 1096\n                  the side view overlays the key + the section gap includes the sagitta line; no dimensions are generated\n    VIEW 常规|剖视   view: normal (default, outline only) / section (outline + ANSI31 hatching)\n                     (dual view was removed on 2026-09-23; the old `VIEW 双` reports an explicit error)\n    REPORT          report: append `REPORT` at the end of a segment (case-insensitive) — no drawing, just prints the Markdown\n                    report (segment list + total length/max diameter);\n                    `REPORT=<path>` / `REPORT-OUT=<path>` also writes a file\n    at x,y rot deg   placement (omitted = origin, no rotation)\n  Example: OCSMSHAFT S30 E30 L45 CH2@L | S40 E40 L30 CH2@R OV3 | S50 E30 L20 | S30 E30 L15 CH2@R | S40 E40 L7 M1.5 | S36 E36 L5 | GEAR M3 Z20 VIEW 剖视 at 100,50 rot 30\n  JSON: {\"segments\":[{\"s\":30,\"e\":30,\"l\":45,\"ch\":[{\"c\":2,\"end\":\"L\"}]},{\"s\":30,\"e\":30,\"l\":20,\"thread\":1.5}],\"view\":\"section\",\"at\":[100,50],\"rot\":30}\nOutline/end faces/chamfers/grooves and boundary lines → 1轮廓实线层; thread minor diameter/run-out → 2细线层 (OV does not draw the grinding thin line); axis/pitch line → 3中心线层; section hatching → 5剖面线层.",
    ),
    Msg::new(
        "cmd.shaft.err.host_prefix",
        "OCSMSHAFT: {msg}",
        "OCSMSHAFT: {msg}",
    ),
    Msg::new("cmd.shaft.end.l", "左", "left"),
    Msg::new("cmd.shaft.end.r", "右", "right"),

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

/// 卡类型显示名（报错前缀）：中文名 → 当前语言的显示名（catalog `card.name.*`）。
/// 只用于错误/回执前缀，不影响 GUI 卡类型下拉（那张表另有 `label`）。
pub fn card_display(name: &str) -> String {
    let key = match name {
        "花键参数表" => "card.name.spline",
        "齿轮参数表" => "card.name.gear",
        "ANSI 花键参数表" => "card.name.ansi",
        "NF 内花键参数表" => "card.name.nf",
        "NF 外花键参数表" => "card.name.nf_ext",
        "DIN 花键参数表" => "card.name.din",
        _ => return name.to_string(),
    };
    t(key)
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
