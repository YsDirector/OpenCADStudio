//! OCSMechanical（OCSM）机械工具包插件。
//!
//! 三个功能：
//! 1. `OCSM` 初始化：幂等创建 9 个机械制图图层（从 opencad.layers_quick
//!    迁移）+ 防御性线型 + 文字样式 `OCSM_GB`。
//! 2. 数字键 `1`~`10` 快速切层：无选中切当前图层（不记 undo）；有选中把对象
//!    移到目标层（记 undo、保留选择集，锁定层被宿主拒绝）。
//! 3. `TF` / `OCSMFRAMEINIT`：打开宿主侧图框选择窗口，从插件目录 `frame/`
//!    选择 DWG，填写比例 `值1:值2`（int 且其一为 1），点击后以块插入；
//!    插入比例 = 值2/值1，`比例`/`SCALE` 属性填入 `值1:值2`。
//!
//! 标注引导服务：
//! - `OCSMDIMGULIDE` / `GDIM`：选中引导线（`10引导线层` LINE）后弹浏览器
//!   GUI 配置标注参数；`应用` 写 PE_URL 超链接，`应用并刷新` 经标注更新
//!   服务器把引导线替换为真实标注（7标注层 / OCSM_GB）并 REGEN。
//! - `OCSMMCP`：确保标注更新服务器运行（独立 MCP 二进制经 TCP 桥接）。

mod bom;
mod detail_clip;
mod dim2gb;
mod guide_server;
mod guide_url;
mod partgen;
mod partgen_more;
pub mod tolerance;


use ocs_plugin_api::export_plugin;
use ocs_plugin_api::host::acadrust;
use ocs_plugin_api::host::acadrust::entities::Dimension;
use ocs_plugin_api::host::acadrust::types::{Color, LineWeight, Vector3};
use ocs_plugin_api::host::{
    BuiltinPlugin, CommandStep, DimStyleDef, FrameItem, HostApi, ImportFrameBlockRequest,
    InteractiveCommand, LayerDef, LinetypeDef, PluginRequestSender, TextStyleDef,
};
use ocs_plugin_api::manifest::{ApiVersion, PluginManifest};
use ocs_plugin_api::ribbon::{
    CadModule, IconKind, ModuleEvent, RibbonGroup, RibbonItem, ToolDef,
};

static MANIFEST: PluginManifest = PluginManifest {
    id: "opencad.ocsm",
    name: "OCSMechanical 机械工具包",
    version: "0.2.0",
    description: "OCSM 初始化 + 数字键 1-10 快速图层 + 图框插入（TF）+ 智能标注（D）",
    api_version: ApiVersion { major: 5 },
    ribbon_order: 100,
    xdata_apps: &["OCSM_BOM"],
    command_prefixes: &[
        "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "TF", "OCSM",
        "OCSMFRAMEINIT", "OCSMFRAMEINSERT", "D", "OCSMPOWERDIM", "OCSMDIMGULIDE",
        "GDIM", "OCSMMCP", "OCSMRGH", "CC", "OCSMDIM2GB", "D2G", "OCSMEDIT", "ME",
        "OCSMPART", "XL", "OCSMBOM", "BOM", "OCSMBOMCFG", "BOMCFG",
    ],
};

/// 10 层模板（来自 opencad.layers_quick，名称数字后无空格；线型全部在
/// OCS 内置 OpenCADStudio.lin 表中）。第 10 层为引导线层：默认关闭（隐藏），
/// 线宽 0mm（本身不打印），用于承载 AI/用户画的引导线。
pub(crate) fn layer_defs() -> Vec<LayerDef> {
    let lw = |v: i16| LineWeight::from_value(v);
    vec![
        LayerDef {
            name: "1轮廓实线层".into(),
            color: Color::from_index(7),
            linetype: "Continuous".into(),
            lineweight: lw(35),
            plottable: true,
            off: false,
        },
        LayerDef {
            name: "2细线层".into(),
            color: Color::from_index(4),
            linetype: "Continuous".into(),
            lineweight: lw(18),
            plottable: true,
            off: false,
        },
        LayerDef {
            name: "3中心线层".into(),
            color: Color::from_index(1),
            linetype: "CENTER2".into(),
            lineweight: lw(18),
            plottable: true,
            off: false,
        },
        LayerDef {
            name: "4虚线层".into(),
            color: Color::from_index(6),
            linetype: "DASHED2".into(),
            lineweight: lw(18),
            plottable: true,
            off: false,
        },
        LayerDef {
            name: "5剖面线层".into(),
            color: Color::from_index(2),
            linetype: "Continuous".into(),
            lineweight: lw(18),
            plottable: true,
            off: false,
        },
        LayerDef {
            name: "6文字层".into(),
            color: Color::from_index(3),
            linetype: "Continuous".into(),
            lineweight: lw(18),
            plottable: true,
            off: false,
        },
        LayerDef {
            name: "7标注层".into(),
            color: Color::from_index(4),
            linetype: "Continuous".into(),
            lineweight: lw(18),
            plottable: true,
            off: false,
        },
        LayerDef {
            name: "8符号标注层".into(),
            color: Color::from_rgb(255, 191, 127),
            linetype: "Continuous".into(),
            lineweight: lw(18),
            plottable: true,
            off: false,
        },
        LayerDef {
            name: "9双点划线层".into(),
            color: Color::from_index(6),
            linetype: "DIVIDE2".into(),
            lineweight: lw(18),
            plottable: true,
            off: false,
        },
        // 引导线层：默认关闭（off），线宽 0mm 不打印。
        LayerDef {
            name: "10引导线层".into(),
            color: Color::from_index(5),
            linetype: "Continuous".into(),
            lineweight: lw(0),
            plottable: true,
            off: true,
        },
    ]
}

/// 防御性线型（宿主内置表已有则跳过；保证 DWG 写出时层引用的线型在表内）。
pub(crate) fn linetype_defs() -> Vec<LinetypeDef> {
    vec![
        LinetypeDef {
            name: "CENTER2".into(),
            description: "Center (.5x)".into(),
            elements: vec![19.05, -3.175, 3.175, -3.175],
        },
        LinetypeDef {
            name: "DASHED2".into(),
            description: "Dashed (.5x)".into(),
            elements: vec![6.35, -3.175],
        },
        LinetypeDef {
            name: "DIVIDE2".into(),
            description: "Divide (.5x)".into(),
            elements: vec![6.35, -3.175, 0.0, -3.175, 0.0, -3.175],
        },
        // 焊接符号第二基准线（虚线）：对照 焊接符号示例.dxf 的 ACISOWELD（__ __）。
        LinetypeDef {
            name: "ACISOWELD".into(),
            description: "Weld dash __ __ __".into(),
            elements: vec![2.0, -1.0],
        },
    ]
}

/// OCSM_GB 文字样式（固定高度 3.5、宽度因子 0.7、注释性）。
pub(crate) fn text_style_defs() -> Vec<TextStyleDef> {
    vec![TextStyleDef {
        name: "OCSM_GB".into(),
        font_file: "Unicode".into(),
        big_font_file: String::new(),
        true_type_font: "Zhuque Fangsong".into(),
        height: 3.5,
        width_factor: 0.7,
        annotative: true,
        is_shape_file: false,
        is_vertical: false,
    }]
}

/// OCSM_GB 标注样式：参数取自用户示例（标注对比.dxf 的 Mechanical 风格），
/// 但 dimtxsty 换成 OCSM_GB 字体（project.md 要求）。
pub(crate) fn dim_style_defs() -> Vec<DimStyleDef> {
    vec![DimStyleDef {
        name: "OCSM_GB".into(),
        make_current: true,
        dimtxt: 2.5,
        dimasz: 2.5,
        dimcen: 2.5,
        dimexe: 2.0,
        dimexo: 0.625,
        dimgap: 1.0,
        dimdli: 0.38,
        dimdle: 0.0,
        dimscale: 1.0,
        dimtad: 1,
        dimjust: 0,
        dimdec: 2,
        dimlunit: 2,
        dimzin: 8,
        dimaunit: 0,
        dimadec: 0,
        dimtih: false,
        // 文字强制水平（general.dxf DSTYLE XDATA dimtoh=1）。
        dimtoh: true,
        dimtix: false,
        dimsoxd: false,
        dimtofl: true,
        dimtol: false,
        dimtp: 0.0,
        dimtm: 0.0,
        dimtdec: 3,
        dimclrd: 130,
        dimclre: 130,
        dimclrt: 3,
        dimlwd: -1,
        dimlwe: -1,
        dimtxsty: "OCSM_GB".into(),
        dimpost: String::new(),
        annotative: false,
    }]
}

/// 按 `scale` 生成缩放版标注样式（frame 比例感知用）：全部尺寸用 dimscale 表达，
/// 命名 `OCSM_GB_x{scale}`（如 OCSM_GB_x2、OCSM_GB_x0.5）。
pub(crate) fn scaled_dim_style_def(scale: f64) -> DimStyleDef {
    let base = dim_style_defs().into_iter().next().expect("base dim style");
    DimStyleDef {
        name: format!("OCSM_GB_x{}", trim_scale(scale)),
        make_current: false,
        dimscale: scale,
        ..base
    }
}

/// 缩放值转样式名片段：2 → "2"、0.5 → "0.5"、0.25 → "0.25"。
pub(crate) fn trim_scale(scale: f64) -> String {
    let s = format!("{scale:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() { "1".to_string() } else { s.to_string() }
}

/// 图框文件夹：优先 `OCSM_FRAME_DIR` 环境变量（测试/排障），否则为插件
/// 安装目录下的 `frame/`（插件 .so 由宿主以 `--ocs-plugin-runner <socket>
/// <cdylib>` 加载，取其父目录即可）。
fn frame_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("OCSM_FRAME_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            return std::path::PathBuf::from(dir);
        }
    }
    let mut args = std::env::args();
    while let Some(arg) = args.next() {
        if arg == "--ocs-plugin-runner" {
            if let (Some(_socket), Some(lib)) = (args.next(), args.next()) {
                let p = std::path::PathBuf::from(lib);
                if let Some(dir) = p.parent() {
                    return dir.join("frame");
                }
            }
            break;
        }
    }
    std::path::PathBuf::from("frame")
}

/// `OCSMBOM`/`BOM`/`OCSMBOMCFG`/`BOMCFG`（整行可能带参数，如 `BOM 30`）。
fn is_bom_command(cmd: &str) -> bool {
    let name = cmd
        .trim()
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(name.as_str(), "OCSMBOM" | "BOM" | "OCSMBOMCFG" | "BOMCFG")
}

struct OcsmPlugin;

/// 标注更新服务器端口（插件进程级；out-of-process 插件不能存宿主 state，
/// 用进程内 static 保存）。
/// 各插件页面窗口的最后心跳（页面每 5 s ping 一次；用来避免重复开窗）。
static PAGE_PING: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>,
> = std::sync::OnceLock::new();

fn page_ping_map() -> &'static std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>> {
    PAGE_PING.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// 页面心跳：`key` 标识页面（含 handle 这类区分项），`bye=true` 表示窗口正在关闭。
pub(crate) fn page_window_ping(key: &str, bye: bool) {
    let mut m = page_ping_map().lock().unwrap();
    if bye {
        m.remove(key);
    } else {
        m.insert(key.to_string(), std::time::Instant::now());
    }
}

/// 该页面窗口是否还开着（15 s 内有心跳）。
pub(crate) fn page_window_alive(key: &str) -> bool {
    page_ping_map()
        .lock()
        .unwrap()
        .get(key)
        .map(|t| t.elapsed() < std::time::Duration::from_secs(15))
        .unwrap_or(false)
}

/// 打开某个插件页面为 **chromium `--app=` 独立窗口**（沉浸式；页面自关后窗口随之退出）。
/// 已开着（有心跳）则不再重复开；返回 true=已（尝试）打开，false=已开着。
pub(crate) fn open_plugin_page(
    port: u16,
    path_and_query: &str,
    ping_key: &str,
    width: u32,
    height: u32,
) -> bool {
    if page_window_alive(ping_key) {
        return false;
    }
    let url = format!("http://127.0.0.1:{port}{path_and_query}");
    if open_app_window(&url, width, height) {
        return true;
    }
    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
    true
}

/// 打开零件库页面：优先 **chromium `--app=` 独立窗口**（沉浸式，无地址栏/标签栏；
/// 且出库后页面能自己 `window.close()` 关掉它），失败时退回 `xdg-open`。
///
/// 坑：插件进程环境是**精简的**（宿主给 plugin runner 的 environ 里没有
/// `DISPLAY`/`WAYLAND_DISPLAY`），直接 spawn chromium 会以
/// "Missing X server or $DISPLAY" 失败 —— 所以先补齐会话的显示变量。
pub(crate) fn open_parts_window(port: u16) -> bool {
    open_plugin_page(port, "/parts", "parts", 960, 900)
}

/// 用 chromium/chrome 的 `--app=` 打开独立窗口；成功返回 true。
fn open_app_window(url: &str, width: u32, height: u32) -> bool {
    let bin = [
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/usr/bin/google-chrome",
        "/usr/bin/microsoft-edge",
    ]
    .iter()
    .find(|p| std::path::Path::new(p).exists());
    let Some(bin) = bin else { return false };
    let mut cmd = std::process::Command::new(bin);
    cmd.arg(format!("--app={url}"))
        .arg(format!("--window-size={width},{height}"))
        .arg("--class=OCSM-Parts");
    for (k, v) in session_display_env() {
        cmd.env(k, v);
    }
    cmd.spawn().is_ok()
}

/// 取会话的显示相关环境变量（插件自身没有时分两步补齐）：
/// ① 自己环境里已有就用；② `systemctl --user show-environment`（X11/Wayland 会话都登记）；
/// ③ 再兜底按 `XDG_RUNTIME_DIR` 下的 wayland socket / xauth 文件推断。
/// 解析 `systemctl --user show-environment` 输出中的显示相关变量（纯函数，便于测试）。
pub(crate) fn parse_session_env(text: &str) -> Vec<(String, String)> {
    const KEYS: [&str; 6] = [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XAUTHORITY",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
        "XDG_SESSION_TYPE",
    ];
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            if KEYS.contains(&k) && !v.trim().is_empty() && !out.iter().any(|(kk, _)| kk == k) {
                out.push((k.to_string(), v.trim().to_string()));
            }
        }
    }
    out
}

fn session_display_env() -> Vec<(String, String)> {
    const KEYS: [&str; 6] = [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XAUTHORITY",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
        "XDG_SESSION_TYPE",
    ];
    let mut out: Vec<(String, String)> = Vec::new();
    let push = |out: &mut Vec<(String, String)>, k: &str, v: &str| {
        if !v.is_empty() && !out.iter().any(|(kk, _)| kk == k) {
            out.push((k.to_string(), v.to_string()));
        }
    };
    for k in KEYS {
        if let Ok(v) = std::env::var(k) {
            push(&mut out, k, &v);
        }
    }
    fn has_display(out: &[(String, String)]) -> bool {
        out.iter().any(|(k, _)| k == "DISPLAY" || k == "WAYLAND_DISPLAY")
    }
    if !has_display(&out) {
        if let Ok(o) = std::process::Command::new("systemctl")
            .args(["--user", "show-environment"])
            .output()
        {
            for (k, v) in parse_session_env(&String::from_utf8_lossy(&o.stdout)) {
                push(&mut out, &k, &v);
            }
        }
    }
    if !has_display(&out) {
        // 兜底：按运行时目录里的 socket / xauth 文件推断
        let rt = out
            .iter()
            .find(|(k, _)| k == "XDG_RUNTIME_DIR")
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| format!("/run/user/{}", unsafe { libc_getuid() }));
        if let Ok(rd) = std::fs::read_dir(&rt) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("wayland-") && !name.ends_with(".lock") {
                    push(&mut out, "WAYLAND_DISPLAY", &name);
                } else if name.starts_with("xauth_") {
                    push(&mut out, "XAUTHORITY", &format!("{rt}/{name}"));
                }
            }
        }
        push(&mut out, "DISPLAY", ":0");
    }
    out
}

/// 当前用户 uid（Linux）。
unsafe fn libc_getuid() -> u32 {
    extern "C" {
        fn getuid() -> u32;
    }
    getuid()
}

/// 待放置零件（「零件出库」登记，`XL` 放置态读它做光标预览与落件）。
#[derive(Clone)]
pub(crate) struct PendingPart {
    pub block: String,
    pub meta_json: String,
    pub label: String,
}

static PENDING_PART: std::sync::OnceLock<std::sync::Mutex<Option<PendingPart>>> =
    std::sync::OnceLock::new();

fn pending_slot() -> &'static std::sync::Mutex<Option<PendingPart>> {
    PENDING_PART.get_or_init(|| std::sync::Mutex::new(None))
}

pub(crate) fn set_pending_part(p: PendingPart) {
    *pending_slot().lock().unwrap() = Some(p);
}

fn pending_part_label() -> Option<String> {
    pending_slot().lock().unwrap().as_ref().map(|p| p.label.clone())
}

fn pending_block() -> Option<String> {
    pending_slot().lock().unwrap().as_ref().map(|p| p.block.clone())
}

/// 一个待落件（基点位置 + 绕基点的旋转角，弧度）。
#[derive(Clone, Copy)]
pub(crate) struct PlaceTask {
    pub pt: [f64; 3],
    pub rotation: f64,
}

/// 落件通道（`XL` 放置态第二下点击 → 放置 worker 落一个件）。
static PLACE_TX: std::sync::OnceLock<std::sync::mpsc::Sender<PlaceTask>> =
    std::sync::OnceLock::new();

fn place_sender(sender: std::sync::Arc<dyn PluginRequestSender>) -> std::sync::mpsc::Sender<PlaceTask> {
    PLACE_TX
        .get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::channel::<PlaceTask>();
            std::thread::Builder::new()
                .name("ocsm-part-place".into())
                .spawn(move || {
                    for task in rx {
                        let pt = task.pt;
                        // 宿主此刻不在命令回调里，worker 请求可安全下发。
                        // 与宿主交互事件处理的微小时序差：稍等一拍更稳。
                        std::thread::sleep(std::time::Duration::from_millis(120));
                        let p = pending_slot().lock().unwrap().clone();
                        let Some(p) = p else { continue };
                        if let Err(e) = place_one(&sender, &p, task) {
                            use ocs_plugin_api::ipc::protocol::PluginRequest;
                            let _ = sender.request(PluginRequest::PushError(format!(
                                "OCSM 标准件：放置失败 {e}"
                            )));
                        }
                    }
                })
                .ok();
            tx
        })
        .clone()
}

/// 在点处落一个件：INSERT + `OCSM_PART` 记录 + 标脏。
fn place_one(
    sender: &std::sync::Arc<dyn PluginRequestSender>,
    p: &PendingPart,
    task: PlaceTask,
) -> Result<(), String> {
    let pt = task.pt;
    use ocs_plugin_api::ipc::protocol::{PluginRequest, PluginResponse};
    use ocs_plugin_api::host::acadrust::entities::Insert;
    use ocs_plugin_api::host::acadrust::types::{Color, LineWeight, Vector3};
    let mut ins = Insert::new(&p.block, Vector3::new(pt[0], pt[1], pt[2]));
    // Insert.rotation 单位是**弧度**（同 Arc，DXF 读入已转换）
    ins.rotation = task.rotation;
    {
        let c = &mut ins.common;
        c.layer = partgen::LAYER_MAIN.to_string();
        c.color = Color::ByLayer;
        c.linetype = "ByLayer".to_string();
        c.line_weight = LineWeight::ByLayer;
    }
    let resp = sender
        .request(PluginRequest::AddEntities(vec![acadrust::EntityType::Insert(ins)]))
        .map_err(|e| e.to_string())?;
    let handle = match resp {
        PluginResponse::Handles(hs) => hs.first().copied(),
        _ => None,
    };
    if let Some(h) = handle {
        let mut rec = ocs_plugin_api::host::acadrust::xdata::ExtendedDataRecord::new("OCSM_PART");
        rec.values.push(ocs_plugin_api::host::acadrust::xdata::XDataValue::String(
            p.meta_json.clone(),
        ));
        let _ = sender.request(PluginRequest::WriteRecord { handle: h, record: rec });
    }
    let _ = sender.request(PluginRequest::BumpGeometry);
    let _ = sender.request(PluginRequest::SetDirty);
    Ok(())
}

/// `SP` 命令里点选的插入点（旧路径保留）。
static PARTS_POINT: std::sync::OnceLock<std::sync::Mutex<Option<[f64; 3]>>> =
    std::sync::OnceLock::new();

fn parts_point_slot() -> &'static std::sync::Mutex<Option<[f64; 3]>> {
    PARTS_POINT.get_or_init(|| std::sync::Mutex::new(None))
}

pub(crate) fn take_parts_point() -> Option<[f64; 3]> {
    parts_point_slot().lock().unwrap().take()
}

static GUIDE_PORT: std::sync::OnceLock<std::sync::Mutex<Option<u16>>> =
    std::sync::OnceLock::new();

/// 当前标注更新服务器端口（未启动时 None）。guide_server 回写"可编辑链接"要用。
pub(crate) fn current_guide_port() -> Option<u16> {
    GUIDE_PORT.get().and_then(|m| *m.lock().unwrap())
}

/// 上次已刷新过链接的端口（避免每次调用都扫图）。
static EDIT_LINK_PORT: std::sync::OnceLock<std::sync::Mutex<Option<u16>>> =
    std::sync::OnceLock::new();

/// 该实体的 `PE_URL` 是否需要重写成当前端口。
/// 需要：无链接 / 链接指向别的端口 / 链接里的 handle 对不上（实体被复制过）。
fn needs_link_rewrite(stored: Option<&str>, port: u16, handle: acadrust::Handle) -> bool {
    let want = format!("/guide.html?handle={:#X}", u64::from(handle));
    match stored {
        Some(u) => {
            !u.contains(&want)
                || !u
                    .split_once("127.0.0.1:")
                    .map(|(_, rest)| rest.starts_with(&format!("{port}/")))
                    .unwrap_or(false)
        }
        None => true,
    }
}

/// 把图纸里带 `OCSM_EDIT` 的标注的 `PE_URL` 重写到当前端口（只写真正需要改的），
/// 使“Ctrl+点击标注回编辑 GUI”在插件换端口后仍然有效。
fn refresh_edit_links(host: &mut dyn HostApi, port: u16) {
    let slot = EDIT_LINK_PORT.get_or_init(|| std::sync::Mutex::new(None));
    {
        let mut last = slot.lock().unwrap();
        if *last == Some(port) {
            return;
        }
        *last = Some(port);
    }
    // 先收集目标（读、写分开借用 host）。
    let targets: Vec<(acadrust::Handle, Option<String>)> = {
        let doc = host.document();
        doc.entities()
            .filter(|e| e.common().extended_data.get_record("OCSM_EDIT").is_some())
            .map(|e| {
                let h = e.common().handle;
                let url = e
                    .common()
                    .extended_data
                    .get_record("PE_URL")
                    .and_then(|r| {
                        r.values.iter().find_map(|v| match v {
                            acadrust::xdata::XDataValue::String(s) if !s.is_empty() => Some(s.clone()),
                            _ => None,
                        })
                    });
                (h, url)
            })
            .collect()
    };
    let mut changed = 0usize;
    for (h, stored) in targets {
        if !needs_link_rewrite(stored.as_deref(), port, h) {
            continue;
        }
        let gui = format!(
            "http://127.0.0.1:{port}/guide.html?handle={:#X}",
            u64::from(h)
        );
        if host.write_record(h, crate::guide_server::pe_url_record(&gui)) {
            changed += 1;
        }
    }
    if changed > 0 {
        host.set_dirty();
        host.push_info(&format!(
            "OCSM：已更新 {changed} 个标注的编辑链接（端口 {port}）。"
        ));
    }
}

impl BuiltinPlugin for OcsmPlugin {
    fn manifest(&self) -> &'static PluginManifest {
        &MANIFEST
    }

    fn ribbon(&self) -> Box<dyn CadModule> {
        struct OcsmModule;
        impl CadModule for OcsmModule {
            fn id(&self) -> &'static str {
                MANIFEST.id
            }
            fn title(&self) -> &'static str {
                "OCSMechanical 机械工具包"
            }
            fn ribbon_groups(&self) -> &[RibbonGroup] {
                static GROUPS: std::sync::OnceLock<Vec<RibbonGroup>> =
                    std::sync::OnceLock::new();
                GROUPS.get_or_init(|| {
                    vec![
                        RibbonGroup {
                            title: "图框",
                            tools: vec![RibbonItem::LargeTool(ToolDef {
                                id: "OCSMFRAMEINIT",
                                label: "插入图幅",
                                icon: IconKind::Glyph("▣"),
                                event: ModuleEvent::Command("OCSMFRAMEINIT".to_string()),
                            })],
                        },
                        RibbonGroup {
                            title: "标注",
                            tools: vec![
                                RibbonItem::LargeTool(ToolDef {
                                    id: "OCSMPOWERDIM",
                                    label: "智能标注",
                                    icon: IconKind::Glyph("📐"),
                                    event: ModuleEvent::Command("OCSMPOWERDIM".to_string()),
                                }),
                                RibbonItem::LargeTool(ToolDef {
                                    id: "OCSMDIMGULIDE",
                                    label: "尺寸引导",
                                    icon: IconKind::Glyph("🖱️"),
                                    event: ModuleEvent::Command("OCSMDIMGULIDE".to_string()),
                                }),
                                RibbonItem::LargeTool(ToolDef {
                                    id: "OCSMDIM2GB",
                                    label: "标注转GB",
                                    icon: IconKind::Glyph("🔁"),
                                    event: ModuleEvent::Command("OCSMDIM2GB".to_string()),
                                }),
                            ],
                        },
                    ]
                })
            }
        }
        Box::new(OcsmModule)
    }

    fn dispatch(&self, host: &mut dyn HostApi, cmd: &str) -> bool {
        // auto-start：宿主把每条命令（含核心命令）都路由给插件 dispatch，
        // 在这里幂等启动标注更新服务器（GUIDE_PORT 防重），MCP 客户端无需
        // 先手动运行 OCSMMCP/GDIM 即可连接。失败静默，GDIM/OCSMMCP 会给出
        // 明确错误。
        let _ = self.ensure_guide_server(host);
        match cmd.trim().to_ascii_uppercase().as_str() {
            "OCSM" => {
                self.cmd_init(host);
                true
            }
            "TF" | "OCSMFRAMEINIT" => {
                self.cmd_frame_init(host);
                true
            }
            "OCSMFRAMEINSERT" => {
                self.cmd_frame_insert(host);
                true
            }
            "D" | "OCSMPOWERDIM" => {
                self.cmd_powerdim(host);
                true
            }
            "OCSMDIMGULIDE" | "GDIM" => {
                self.cmd_guide(host);
                true
            }
            "OCSMEDIT" | "ME" => {
                self.cmd_edit(host);
                true
            }
            "OCSMRGH" | "CC" => {
                self.cmd_roughness(host);
                true
            }
            "OCSMPART" | "XL" => {
                self.cmd_parts(host);
                true
            }
            // 明细表：建表/刷新（`BOM 30` = 本次首列 30 行）、配置（`BOMCFG 30`）
            _ if is_bom_command(cmd) => {
                let upper = cmd.trim().to_ascii_uppercase();
                let (name, rest) = match upper.split_once(char::is_whitespace) {
                    Some((n, r)) => (n.to_string(), r.trim().to_string()),
                    None => (upper.clone(), String::new()),
                };
                if name == "OCSMBOMCFG" || name == "BOMCFG" {
                    bom::cmd_bom_cfg(host, &rest);
                } else {
                    bom::cmd_bom(host, &rest);
                }
                true
            }
            "OCSMDIM2GB" | "D2G" => {
                self.cmd_dim2gb(host);
                true
            }
            "OCSMMCP" => {
                self.cmd_mcp(host);
                true
            }
            _ => {
                // 数字键 1~10（单字符 + 两位数 10，避免与其它命令冲突；DYN
                // 输入时数字由宿主路由进距离字段，不会到这里）。
                let bytes = cmd.as_bytes();
                let digit = if bytes == b"10" {
                    10
                } else if bytes.len() == 1 && (b'1'..=b'9').contains(&bytes[0]) {
                    (bytes[0] - b'0') as usize
                } else {
                    0
                };
                if digit >= 1 {
                    self.cmd_layer_key(host, digit);
                    return true;
                }
                false
            }
        }
    }
}

impl OcsmPlugin {
    /// `OCSM`：初始化图层 + 线型 + 文字样式 + 标注样式（幂等）。
    fn cmd_init(&self, host: &mut dyn HostApi) {
        // 线型先于图层：DWG 写出时层引用的线型名必须在 line_types 表。
        let lt = host.ensure_linetypes(linetype_defs());
        let layers = host.ensure_layers(layer_defs());
        let styles = host.ensure_text_styles(text_style_defs());
        // 标注样式（参数取自标注示例.dwg 的 Mechanical，字体换成 OCSM_GB）。
        let dim_styles = host.ensure_dim_styles(dim_style_defs());
        host.push_output(&format!(
            "OCSM 初始化完成：新增图层 {layers} 个、线型 {lt} 个、文字样式 {styles} 个（OCSM_GB）、标注样式 {dim_styles} 个（OCSM_GB，当前样式）。数字键 1-10、TF、D 已就绪。"
        ));
    }

    /// `OCSMDIMGULIDE` / `GDIM`：选中引导线 → 启动标注更新服务器 →
    /// 浏览器打开标注配置 GUI。
    fn cmd_guide(&self, host: &mut dyn HostApi) {
        let selected = host.selected_handles();
        let Some(&h) = selected.first() else {
            host.push_error("OCSM: 请先选中一条引导线（10引导线层 的直线或两段多段线）。");
            return;
        };
        let doc = host.document();
        let Some(e) = doc.get_entity(h) else {
            host.push_error("OCSM: 找不到选中的实体。");
            return;
        };
        match &e {
            acadrust::EntityType::Line(_) => {}
            // 局部放大图：圆（CIRCLE）或矩形（闭合 4 顶点无 bulge PLINE = RECTANG 产物）。
            acadrust::EntityType::Circle(_) => {}
            // 弧长标注：ARC 引导。
            acadrust::EntityType::Arc(_) => {}
            acadrust::EntityType::LwPolyline(pl) if !pl.is_closed && pl.vertices.len() >= 2 => {}
            acadrust::EntityType::LwPolyline(pl)
                if pl.is_closed
                    && pl.vertices.len() == 4
                    && pl.vertices.iter().all(|v| v.bulge.abs() < 1e-9) => {}
            acadrust::EntityType::Polyline(pl) if !pl.flags.is_closed() && pl.vertices.len() >= 2 => {}
            acadrust::EntityType::Polyline2D(pl) if !pl.flags.is_closed() && pl.vertices.len() >= 2 => {}
            // 矩形引导也可能以 heavy polyline 形式存在（DXF 读取路径转换）。
            acadrust::EntityType::Polyline(pl)
                if pl.flags.is_closed() && pl.vertices.len() == 4 => {}
            acadrust::EntityType::Polyline2D(pl)
                if pl.flags.is_closed()
                    && pl.vertices.len() == 4
                    && pl.vertices.iter().all(|v| v.bulge.abs() < 1e-9) => {}
            _ => {
                host.push_error("OCSM: 引导线必须是直线（LINE）、多段线（PLINE，2 顶点以上）、圆（CIRCLE）、圆弧（ARC）或矩形（RECTANG）。");
                return;
            }
        }
        let Some(port) = self.ensure_guide_server(host) else {
            host.push_error("OCSM: 无法启动标注更新服务器（宿主不支持 worker 请求）。");
            return;
        };
        // 心跳键与页面一致（页面用 location.search 去掉 '?'：`handle=2A`）
        let key = format!("handle={:X}", u64::from(h));
        let opened = open_plugin_page(
            port,
            &format!("/guide.html?handle={:#X}", u64::from(h)),
            &key,
            1020,
            900,
        );
        host.push_info(if opened {
            "OCSM 尺寸引导：已打开标注配置窗口。应用=写超链接；应用并刷新=生成标注（7标注层/OCSM_GB）。"
        } else {
            "OCSM 尺寸引导：标注配置窗口已打开（Alt+Tab 切换过去）。"
        });
    }

    /// `OCSMEDIT` / `ME`：选中一个 OCSM 生成的标注（带 `OCSM_EDIT` 记录）→
    /// 打开同一个标注配置 GUI 改它：GUI 从 `/api/guide` 读该标注的记录恢复
    /// 全部参数；「应用并刷新」时插件会删旧标注并用原引导几何重生成（替换）。
    /// 注意：命令里**不能**用 sender（会与主线程死锁）——直接读 host.document()。
    fn cmd_edit(&self, host: &mut dyn HostApi) {
        let selected = host.selected_handles();
        let Some(&h) = selected.first() else {
            host.push_error("OCSM: 请先选中一个 OCSM 生成的标注（引导生成的尺寸/焊接/引线/公差等），再执行 OCSMEDIT。");
            return;
        };
        let has_edit = {
            let doc = host.document();
            doc.get_entity(h).is_some_and(|e| {
                e.common()
                    .extended_data
                    .get_record("OCSM_EDIT")
                    .is_some()
            })
        };
        if !has_edit {
            host.push_error(
                "OCSM: 选中的对象不是 OCSM 生成的标注（没有可编辑信息）。若它是旧版生成的标注，请先重画一次。",
            );
            return;
        }
        let Some(port) = self.ensure_guide_server(host) else {
            host.push_error("OCSM: 无法启动标注更新服务器（宿主不支持 worker 请求）。");
            return;
        };
        // 心跳键与页面一致（页面用 location.search 去掉 '?'：`handle=2A`）
        let key = format!("handle={:X}", u64::from(h));
        let opened = open_plugin_page(
            port,
            &format!("/guide.html?handle={:#X}", u64::from(h)),
            &key,
            1020,
            900,
        );
        host.push_info(if opened {
            "OCSM 标注编辑：已打开配置窗口（改完点「应用并刷新」= 替换旧标注）。"
        } else {
            "OCSM 标注编辑：配置窗口已打开（Alt+Tab 切换过去）。"
        });
    }

    /// `OCSMDIM2GB` / `D2G`：把宿主原生标注（模型空间 DIMENSION）**重建**为 OCSM 的
    /// GB 标准版本——OCSM_GB_x{图框比例} 样式 + `7标注层`（尺寸）/ 匿名块（直径、
    /// 半径、角度、弧长），保留原文字覆盖、小数位、样式前后缀与公差。
    /// 另外转**引线标注**（`LEADER` + 绑定 `MTEXT`）：文字 1~2 行→OCSM 引线标注
    /// （第1行→上侧、第2行→下侧；≥３ 行跳过）；多重引线本期不转。
    /// 有选中→只转选中集；无选中→全图扫描。不支持的类型跳过并在结束时报清单，
    /// 整批一次 Ctrl+Z 可全撤。
    fn cmd_dim2gb(&self, host: &mut dyn HostApi) {
        let selected = host.selected_handles();
        let plan = {
            let doc = host.document();
            crate::dim2gb::plan(doc, &selected)
        };
        if plan.seen == 0 {
            let tail = if plan.skipped.is_empty() {
                String::new()
            } else {
                format!("（{}）", plan.skipped.join("；"))
            };
            host.push_info(&format!(
                "OCSMDIM2GB：未找到可转换的原生标注（仅扫描模型空间）{tail}"
            ));
            return;
        }
        host.push_info(&plan.report());
        if plan.converted == 0 {
            return;
        }
        host.push_undo("OCSMDIM2GB 原生标注转 GB");
        // ① 幂等补基建（图层/文字样式/标注样式——图纸可能从没跑过 OCSM 初始化）
        // ② 建匿名块（INSERT 引用它）③ 加新实体 ④ 删原标注。
        host.ensure_layers(layer_defs());
        host.ensure_text_styles(text_style_defs());
        let mut ensured: Vec<String> = Vec::new();
        for def in &plan.styles {
            if ensured.iter().any(|n| n.eq_ignore_ascii_case(&def.name)) {
                continue;
            }
            ensured.push(def.name.clone());
            host.ensure_dim_styles(vec![def.clone()]);
        }
        for (name, ents) in &plan.blocks {
            if let Err(e) = host.add_block_record(name, ents.clone()) {
                host.push_error(&format!("OCSMDIM2GB：建块 {name} 失败：{e}"));
            }
        }
        if !plan.adds.is_empty() {
            let handles = host.add_entities(plan.adds.clone());
            // D2G 产出的标注带 `OCSM_EDIT`（参数 + 引导几何）→ 再补上回编辑 GUI 的
            // `PE_URL`。**必须等实体有了句柄**（URL 里要嵌 handle），所以放在这里。
            // 插件服务没起（端口未知）时跳过——`ME` 命令仍可编辑。
            if let Some(port) = crate::current_guide_port() {
                let targets: Vec<acadrust::Handle> = handles
                    .iter()
                    .copied()
                    .filter(|h| {
                        host.document().get_entity(*h).is_some_and(|e| {
                            e.common()
                                .extended_data
                                .get_record("OCSM_EDIT")
                                .is_some()
                        })
                    })
                    .collect();
                for h in targets {
                    let gui = format!(
                        "http://127.0.0.1:{port}/guide.html?handle={:#X}",
                        u64::from(h)
                    );
                    host.write_record(h, crate::guide_server::pe_url_record(&gui));
                }
            }
        }
        for h in &plan.removes {
            host.remove_entity(*h);
        }
        host.bump_geometry();
        host.set_dirty();
    }

    /// `OCSMRGH` / `CC`：表面粗糙度——交互点选插入点 → 打开粗糙度配置 GUI。
    /// 无引导线：点选位置即符号插入点（URL 带坐标），GUI 里选形态/填属性后
    /// POST /api/rough_apply 生成匿名块（ATTDEF 文字）+ INSERT。
    fn cmd_roughness(&self, host: &mut dyn HostApi) {
        let Some(sender) = host.plugin_request_sender() else {
            host.push_error("OCSM: 无法获取插件请求通道（宿主不支持 worker 请求）。");
            return;
        };
        host.start_interactive(Box::new(RoughnessPlace {
            sender: std::sync::Arc::from(sender),
        }));
    }

    /// `OCSMPART` / `XL`：直接打开标准件库窗口，并进入放置态
    /// （窗口里「零件出库」→ 回到图纸鼠标跟随预览 → 左键点击放置，可连续，Esc 结束）。
    fn cmd_parts(&self, host: &mut dyn HostApi) {
        // 先确保标注更新服务器在跑并打开零件库窗口（不需要先点插入点）
        if let Some(port) = self.ensure_guide_server(host) {
            if open_parts_window(port) {
                host.push_info(
                    "OCSM 标准件库：已打开零件库窗口。选零件点「零件出库」→ 回到图纸点击定位基点 → 移动光标旋转 → 再点击落定（可连续，Esc 结束）。",
                );
            } else {
                host.push_info("OCSM 标准件库：零件库窗口已打开（Alt+Tab 切换过去）。");
            }
        } else {
            host.push_error("OCSM: 无法启动零件库服务（宿主不支持 worker 请求）。");
            return;
        }
        // 同时进入放置态：窗口里出库后，鼠标即跟随预览、左键落件
        let _ = self.ensure_guide_server(host);
        let Some(sender) = host.plugin_request_sender() else {
            return;
        };
        host.start_interactive(Box::new(PartPlace {
            sender: std::sync::Arc::from(sender),
            phase: std::cell::Cell::new(PlacePhase::Follow),
            base: std::cell::Cell::new([0.0, 0.0, 0.0]),
        }));
    }

    /// `OCSMMCP`：确保标注更新服务器运行，打印 MCP 接入信息。
    fn cmd_mcp(&self, host: &mut dyn HostApi) {
        let Some(port) = self.ensure_guide_server(host) else {
            host.push_error("OCSM: 无法启动标注更新服务器（宿主不支持 worker 请求）。");
            return;
        };
        host.push_output(&format!(
            "OCSM MCP：标注更新服务器已就绪 http://127.0.0.1:{port} \
             （浏览器 GUI /api/guide 等）。MCP 服务器为独立二进制，AI 客户端经 \
             stdio 启动后经 TCP 桥接到本端口。"
        ));
    }

    /// 确保标注更新服务器在跑，返回端口；失败返回 None。
    fn ensure_guide_server(&self, host: &mut dyn HostApi) -> Option<u16> {
        let sender = std::sync::Arc::from(host.plugin_request_sender()?);
        let mut port_slot = GUIDE_PORT.get_or_init(|| std::sync::Mutex::new(None)).lock().unwrap();
        let port = if let Some(p) = *port_slot {
            p
        } else {
            let server = crate::guide_server::spawn(sender)?;
            *port_slot = Some(server.port);
            server.port
        };
        drop(port_slot);
        // 端口与上次不同（含首次、含插件重启后端口变了）→ 把图纸里已有 OCSM 标注的
        // “回编辑”链接重写到当前端口，否则老标注 Ctrl+点击会打开失效地址。
        refresh_edit_links(host, port);
        Some(port)
    }

    /// 数字键 1~10：有选中 → 移动对象到目标层；无选中 → 切当前图层。
    fn cmd_layer_key(&self, host: &mut dyn HostApi, digit: usize) {
        let prefix = digit.to_string();
        let Some(def) = layer_defs()
            .iter()
            .find(|d| d.name.starts_with(&prefix))
            .cloned()
        else {
            host.push_error("OCSM: 图层模板缺失。请先运行 OCSM 初始化。");
            return;
        };
        let target = def.name.clone();

        let selected = host.selected_handles();
        if selected.is_empty() {
            if host.set_current_layer(&target) {
                host.push_info(&format!("当前图层已切换到「{target}」。"));
            } else {
                host.push_error(&format!(
                    "图层「{target}」不存在。请先运行 OCSM 初始化。"
                ));
            }
            return;
        }

        // 有选中对象：移动（不切当前层），记 undo、保留选择集。
        host.push_undo(&format!("移动到图层{target}"));
        let mut moved = 0usize;
        let mut refused = 0usize;
        for h in selected {
            let Some(mut entity) = host.document().get_entity(h).cloned() else {
                continue;
            };
            entity.common_mut().layer = target.clone();
            if host.update_entity(entity) {
                moved += 1;
            } else {
                refused += 1; // 锁定层对象被宿主拒绝
            }
        }
        let mut msg = format!("已将 {moved} 个对象移动到「{target}」。");
        if refused > 0 {
            msg.push_str(&format!("（{refused} 个在锁定图层上，已跳过）"));
        }
        host.push_info(&msg);
    }

    /// `TF`：扫描图框文件夹并打开宿主侧选择窗口。
    fn cmd_frame_init(&self, host: &mut dyn HostApi) {
        let dir = frame_dir();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            host.push_error(&format!(
                "OCSMFRAMEINIT: 找不到图框文件夹 {}。请在插件目录的 frame/ 中放入 DWG。",
                dir.display()
            ));
            return;
        };
        let mut frames: Vec<FrameItem> = entries
            .flatten()
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("dwg"))
                    .unwrap_or(false)
            })
            .filter_map(|e| {
                let path = e.path();
                let label = path.file_stem()?.to_string_lossy().into_owned();
                Some(FrameItem {
                    path: path.to_string_lossy().into_owned(),
                    label,
                })
            })
            .collect();
        frames.sort_by(|a, b| a.label.cmp(&b.label));
        if frames.is_empty() {
            host.push_error(&format!(
                "OCSMFRAMEINIT: {} 下没有 DWG 图框文件。",
                dir.display()
            ));
            return;
        }
        host.push_info(&format!(
            "OCSMFRAMEINIT: 找到 {} 个图框，正在打开选择窗口…",
            frames.len()
        ));
        if !host.show_frame_picker(frames) {
            host.push_error("OCSMFRAMEINIT: 无法打开图框选择窗口。");
        }
    }

    /// `OCSMFRAMEINSERT`：取走宿主选择结果，定义图框块并进入交互插入。
    fn cmd_frame_insert(&self, host: &mut dyn HostApi) {
        let Some(sel) = host.take_pending_frame_selection() else {
            host.push_error("OCSMFRAMEINSERT: 没有待处理的图框选择。请先运行 TF。");
            return;
        };
        // 复校比例：正整数且其一为 1。
        if sel.scale_v1 <= 0
            || sel.scale_v2 <= 0
            || (sel.scale_v1 != 1 && sel.scale_v2 != 1)
        {
            host.push_error("OCSMFRAMEINSERT: 非法的比例（必须为两个正整数且其一为 1）。");
            return;
        }
        let scale = sel.scale_v2 as f64 / sel.scale_v1 as f64;
        let scale_text = format!("{}:{}", sel.scale_v1, sel.scale_v2);
        let block_name = std::path::Path::new(&sel.path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "frame".to_string());

        // 图框比例感知：为本次缩放创建对应标注样式（如 OCSM_GB_x2），
        // POWERDIM 在 frame 内标注时按此样式放大文字/箭头。
        if (scale - 1.0).abs() > 1e-9 {
            host.ensure_dim_styles(vec![scaled_dim_style_def(scale)]);
        }

        // 先在宿主侧定义块（幂等），并取回 ATTDEF 供插入时构造属性。
        let attdefs = match host.import_frame_block(ImportFrameBlockRequest {
            path: sel.path.clone(),
            block_name: block_name.clone(),
        }) {
            Ok(attdefs) => attdefs,
            Err(e) => {
                host.push_error(&format!("OCSMFRAMEINSERT: {e}"));
                return;
            }
        };

        host.push_info(&format!(
            "指定图框「{block_name}」的插入点（比例 {scale_text}，缩放 {scale:.2} 倍）…"
        ));
        host.start_interactive(Box::new(FramePlace {
            block_name,
            scale,
            scale_text,
            attdefs,
        }));
    }

    /// `D` / `OCSMPOWERDIM`：智能标注（点/直线/圆/圆弧 → 自动推断标注类型，
    /// 光标跟随预览，frame 比例感知）。
    fn cmd_powerdim(&self, host: &mut dyn HostApi) {
        // 标注固定放 7标注层（模板层），兜底确保存在；同时确保文字/标注样式
        // 就绪——否则样式缺失时标注回退默认参数（文字变图层色、尺寸异常）。
        host.ensure_layers(layer_defs());
        host.ensure_text_styles(text_style_defs());
        host.ensure_dim_styles(dim_style_defs());
        // 本次 dispatch 的 host proxy 是新建的，document() 快照即最新
        // （用户此前画的线/圆/圆弧/图框都在里面），供命令内几何查询。
        let doc = host.document().clone();
        host.push_info(
            "OCSMPOWERDIM: 拾取点模式（选端点/圆心/交点），Enter 切换线段点选；Esc 取消。",
        );
        host.start_interactive(Box::new(PowerDim::new(doc)));
    }
}

// ── OCSMPOWERDIM ────────────────────────────────────────────────────────────

/// 拾取对象的最小几何描述（命令内不持有宿主，只读快照）。
#[derive(Clone, Copy)]
pub(crate) struct LineGeom {
    start: [f64; 3],
    end: [f64; 3],
}

#[derive(Clone, Copy)]
struct CircleGeom {
    center: [f64; 3],
    radius: f64,
}

#[derive(Clone, Copy)]
struct ArcGeom {
    center: [f64; 3],
    radius: f64,
    start_angle: f64,
    end_angle: f64,
}

#[derive(Clone, Copy)]
enum PickKind {
    Line(LineGeom),
    Circle(CircleGeom),
    Arc(ArcGeom),
}

#[derive(Clone, Copy, PartialEq)]
enum PlaceMode {
    Aligned,
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, PartialEq)]
enum CircleMode {
    Diameter,
    Radius,
}

#[derive(Clone, Copy, PartialEq)]
enum ArcMode {
    Radius,
    ArcLength,
}

enum Step {
    /// 等待第一个点击（对象或拾取点）。
    First,
    /// 已有一个拾取点，等待第二个。
    TwoPoints { first: [f64; 3] },
    /// 两个拾取点，等待放置点（A/H/V 关键字可锁定行为）。
    TwoPointPlace {
        first: [f64; 3],
        second: [f64; 3],
        mode: PlaceMode,
    },
    /// 已点一条直线，等待放置点。
    LinePlace {
        line: LineGeom,
        mode: PlaceMode,
    },
    /// 两直线夹角（劣角/优角关键字可切换）。
    AngularPlace {
        line1: LineGeom,
        line2: LineGeom,
        major: bool,
    },
    /// 圆：直径/半径。
    Circle {
        circle: CircleGeom,
        mode: CircleMode,
    },
    /// 圆弧：半径/弧长。
    Arc {
        arc: ArcGeom,
        mode: ArcMode,
    },
}

/// 拾取模式：`Points` = 双拾取点（对象捕捉开）；`Segments` = 线段点选
/// （对象捕捉关，直接点直线/圆/圆弧，光标只留拾取方框）。按 Enter 切换。
#[derive(Clone, Copy, PartialEq)]
enum PickMode {
    Points,
    Segments,
}

impl Clone for Step {
    fn clone(&self) -> Self {
        *self
    }
}
impl Copy for Step {}

/// 智能标注交互命令。全程 entity-pick（`snapped=true` 的点视为拾取点，
/// 未吸附的实体点击视为对象选择；空白点击=自由拾取点）。
struct PowerDim {
    doc: acadrust::CadDocument,
    step: Step,
    /// 拾取模式：Points（默认，对象捕捉开）↔ Segments（Enter 切换，点选线段）。
    pick_mode: PickMode,
    /// 第一个拾取点所在 frame 的缩放比例（决定标注样式）。
    frame_scale: f64,
    style_name: String,
    /// 样式缺失时的提示（附加在 prompt 尾部）。
    style_hint: Option<String>,
    /// 刚点了平行线时的提示。
    parallel_notice: bool,
}

impl PowerDim {
    fn new(doc: acadrust::CadDocument) -> Self {
        Self {
            doc,
            step: Step::First,
            pick_mode: PickMode::Points,
            frame_scale: 1.0,
            style_name: "OCSM_GB".to_string(),
            style_hint: None,
            parallel_notice: false,
        }
    }

    /// 按句柄从快照读取几何。
    fn classify(&self, handle: acadrust::Handle) -> Option<PickKind> {
        use acadrust::entities::EntityType as E;
        let e = self.doc.get_entity(handle)?;
        match e {
            E::Line(l) => Some(PickKind::Line(LineGeom {
                start: to_arr(&l.start),
                end: to_arr(&l.end),
            })),
            E::Circle(c) => Some(PickKind::Circle(CircleGeom {
                center: to_arr(&c.center),
                radius: c.radius,
            })),
            E::Arc(a) => Some(PickKind::Arc(ArcGeom {
                center: to_arr(&a.center),
                radius: a.radius,
                start_angle: a.start_angle,
                end_angle: a.end_angle,
            })),
            _ => None,
        }
    }

    /// 第一个拾取点处计算 frame 缩放比例，选定标注样式。
    fn resolve_style(&mut self, pt: [f64; 3]) {
        let scale = frame_scale_at(&self.doc, pt);
        self.frame_scale = scale;
        if (scale - 1.0).abs() < 1e-9 {
            self.style_name = "OCSM_GB".to_string();
            self.style_hint = None;
            return;
        }
        let name = format!("OCSM_GB_x{}", trim_scale(scale));
        if self
            .doc
            .dim_styles
            .iter()
            .any(|s| s.name.eq_ignore_ascii_case(&name))
        {
            self.style_name = name;
            self.style_hint = None;
        } else {
            // 缩放样式不存在（如旧图）：回退 OCSM_GB 并在提示里说明。
            self.style_name = "OCSM_GB".to_string();
            self.style_hint = Some(format!(
                "（未找到标注样式 {name}，已回退 OCSM_GB——请重新插入图框或运行 OCSM）"
            ));
        }
    }

    fn set_place_mode(&mut self, mode: PlaceMode) {
        match &mut self.step {
            Step::TwoPointPlace { mode: m, .. } | Step::LinePlace { mode: m, .. } => *m = mode,
            _ => {}
        }
    }

    fn set_major(&mut self, major: bool) {
        if let Step::AngularPlace { major: m, .. } = &mut self.step {
            *m = major;
        }
    }

    fn set_circle_mode(&mut self, mode: CircleMode) {
        if let Step::Circle { mode: m, .. } = &mut self.step {
            *m = mode;
        }
    }

    fn set_arc_mode(&mut self, mode: ArcMode) {
        if let Step::Arc { mode: m, .. } = &mut self.step {
            *m = mode;
        }
    }

    /// 构建线性/对齐标注（放置点=pt），并盖上图层/样式。
    fn build_linear(
        &self,
        first: [f64; 3],
        second: [f64; 3],
        pt: [f64; 3],
        mode: PlaceMode,
    ) -> Dimension {
        use acadrust::entities::{DimensionAligned, DimensionLinear};
        let (first, second, pt) = (v3(first), v3(second), v3(pt));
        let dim = match mode {
            PlaceMode::Aligned => {
                let mut d = DimensionAligned::new(first, second);
                let axis = (second - first).normalize();
                d.definition_point = pt;
                d.base.definition_point = pt;
                let tp = linear_text_pos(first, second, pt, axis);
                d.base.text_middle_point = tp;
                d.base.insertion_point = tp;
                d.base.actual_measurement = d.measurement();
                Dimension::Aligned(d)
            }
            PlaceMode::Horizontal => {
                let mut d = DimensionLinear::horizontal(first, second);
                let tp = linear_text_pos(first, second, pt, Vector3::new(1.0, 0.0, 0.0));
                d.definition_point = pt;
                d.base.definition_point = pt;
                d.base.text_middle_point = tp;
                d.base.insertion_point = tp;
                d.base.actual_measurement = d.measurement();
                Dimension::Linear(d)
            }
            PlaceMode::Vertical => {
                let mut d = DimensionLinear::vertical(first, second);
                let tp = linear_text_pos(first, second, pt, Vector3::new(0.0, 1.0, 0.0));
                d.definition_point = pt;
                d.base.definition_point = pt;
                d.base.text_middle_point = tp;
                d.base.insertion_point = tp;
                d.base.actual_measurement = d.measurement();
                Dimension::Linear(d)
            }
        };
        stamp(dim, &self.style_name)
    }

    /// 圆/圆弧：直径或半径（光标在内→内标注，在外→外标注+引出线）。
    fn build_radial(
        &self,
        center: [f64; 3],
        radius: f64,
        pt: [f64; 3],
        mode: CircleMode,
    ) -> Dimension {
        use acadrust::entities::{DimensionDiameter, DimensionRadius};
        let (center, pt) = (v3(center), v3(pt));
        let rim = rim_toward(center, pt, radius);
        let dist = pt.distance(&rim);
        let leader = if dist < radius { 0.0 } else { dist };
        let dim = match mode {
            CircleMode::Diameter => {
                let mut d = DimensionDiameter::new(center, rim);
                d.base.definition_point = rim;
                d.base.text_middle_point = pt;
                d.base.insertion_point = pt;
                d.leader_length = leader;
                // 注意：v0.9.8 cadcodec 的 Diameter measurement() = 两点距离
                //（圆心↔圆周点 = 半径），直径需 ×2。
                d.base.actual_measurement = d.measurement() * 2.0;
                Dimension::Diameter(d)
            }
            CircleMode::Radius => {
                let mut d = DimensionRadius::new(center, rim);
                d.base.text_middle_point = pt;
                d.base.insertion_point = pt;
                d.leader_length = leader;
                d.base.actual_measurement = d.measurement();
                Dimension::Radius(d)
            }
        };
        let mut dim = stamp(dim, &self.style_name);
        // 文字跟随点击/光标位置（否则宿主按样式重算，尺寸线总在圆内侧）。
        dim.base_mut().text_user_positioned = true;
        // 宿主自然旋转对半径/直径返回 0（水平）；这里插件直接把字旋转角设为
        // 径向方向（从圆心指向文字点），并 clamp 到 (-90°, 90°] 防倒置。
        let ang = (pt.y - center.y).atan2(pt.x - center.x);
        let pi = std::f64::consts::PI;
        let rot = if ang > pi / 2.0 {
            ang - pi
        } else if ang <= -pi / 2.0 {
            ang + pi
        } else {
            ang
        };
        dim.base_mut().text_rotation = rot;
        dim
    }

    /// 圆弧：半径或弧长。
    fn build_arc(&self, arc: &ArcGeom, pt: [f64; 3], mode: ArcMode) -> Dimension {
        use acadrust::entities::{DimensionArc, DimensionBase, DimensionType};
        match mode {
            ArcMode::Radius => self.build_radial(arc.center, arc.radius, pt, CircleMode::Radius),
            ArcMode::ArcLength => {
                let center = v3(arc.center);
                let start_pt = center
                    + Vector3::new(arc.start_angle.cos(), arc.start_angle.sin(), 0.0) * arc.radius;
                let end_pt = center
                    + Vector3::new(arc.end_angle.cos(), arc.end_angle.sin(), 0.0) * arc.radius;
                let def = v3(pt);
                let outside = def.distance(&center) > arc.radius;
                // 被标注弧的实际扫角（rem_euclid 规范化跨 0° 方向），
                // 保证优弧（>180°）按优弧长度标注并画优弧。
                let sweep = (arc.end_angle - arc.start_angle).rem_euclid(std::f64::consts::TAU);
                let mut d = DimensionArc {
                    base: DimensionBase::new(DimensionType::ArcLength),
                    definition_point: def,
                    first_extension_point: start_pt,
                    second_extension_point: end_pt,
                    center_point: center,
                    is_partial: true,
                    arc_start_parameter: arc.start_angle,
                    // 端点=起点+实际扫角，measurement = r × sweep（含优弧）。
                    arc_end_parameter: arc.start_angle + sweep,
                    has_leader: outside,
                    first_leader_point: def,
                    second_leader_point: rim_toward(center, def, arc.radius),
                };
                d.base.text_middle_point = def;
                d.base.insertion_point = def;
                d.base.actual_measurement = d.measurement();
                let mut dim = stamp(Dimension::Arc(d), &self.style_name);
                dim.base_mut().text_user_positioned = true;
                dim
            }
        }
    }

    /// 两直线夹角标注（劣角默认；优角 = 360°−劣角，弧取优弧）。
    fn build_angular(
        &self,
        line1: LineGeom,
        line2: LineGeom,
        pt: [f64; 3],
        major: bool,
    ) -> Dimension {
        use acadrust::entities::{DimensionAngular2Ln, DimensionBase, DimensionType};
        let minor_deg = angular_minor_sweep_deg(&line1, &line2, pt);
        let measurement = if major {
            360.0 - minor_deg
        } else {
            minor_deg
        };
        let mut d = DimensionAngular2Ln {
            base: DimensionBase::new(DimensionType::Angular),
            dimension_arc: v3(pt),
            first_point: v3(line1.start),
            second_point: v3(line1.end),
            angle_vertex: v3(line2.start),
            definition_point: v3(line2.end),
        };
        d.base.text_middle_point = v3(pt);
        d.base.insertion_point = v3(pt);
        d.base.actual_measurement = measurement;
        // 宿主对 Angular2Ln 的文字用 measurement()（对两线存储语义错误，
        // 可能导致恒 90°）。不改宿主，这里直接用 user_text 提供正确角度值。
        d.base.user_text = Some(format!("{:.0}°", measurement));
        let mut dim = stamp(Dimension::Angular2Ln(d), &self.style_name);
        dim.base_mut().text_user_positioned = true;
        dim
    }
}

impl ocs_plugin_api::host::InteractiveCommand for PowerDim {
    fn prompt(&self) -> String {
        let base = match &self.step {
            Step::First => {
                if matches!(self.pick_mode, PickMode::Points) {
                    "OCSMPOWERDIM 拾取点模式：选择端点/圆心/交点等，或 Enter 切换为线段点选。".to_string()
                } else {
                    "OCSMPOWERDIM 线段点选模式：点击直线/圆/圆弧创建标注，或 Enter 返回拾取点。".to_string()
                }
            }
            Step::TwoPoints { .. } => "选择下一个拾取点。".to_string(),
            Step::TwoPointPlace { .. } | Step::LinePlace { .. } => {
                "选择标注行为：`对齐(A默认)`,`水平(H)`,`竖直(V)`。选择一根与当前直线不平行的直线以创建角度标注。或选择下一个拾取点以放置标注。".to_string()
            }
            Step::AngularPlace { .. } => {
                "选择标注的行为：`劣角(I默认)`,`优角(R)`。或选择下一个拾取点以放置标注。".to_string()
            }
            Step::Circle { .. } => {
                "选择标注行为：`直径(D默认)`，`半径(R)`。或选择下一个拾取点以放置标注。".to_string()
            }
            Step::Arc { .. } => {
                "选择标注行为：`半径(R默认)`，`弧长(A)`。或选择下一个拾取点以放置标注。".to_string()
            }
        };
        if self.parallel_notice {
            format!("{base}（两直线平行，无法创建角度标注）")
        } else if let Some(hint) = &self.style_hint {
            format!("{base}{hint}")
        } else {
            base
        }
    }

    fn on_point(&mut self, _pt: [f64; 3]) -> CommandStep {
        CommandStep::NeedPoint
    }

    fn on_enter(&mut self) -> CommandStep {
        // First 状态：Enter 在 拾取点模式 ↔ 线段点选模式 间切换；
        // 其它状态：Enter 取消命令。
        if matches!(self.step, Step::First) {
            self.pick_mode = match self.pick_mode {
                PickMode::Points => PickMode::Segments,
                PickMode::Segments => PickMode::Points,
            };
            CommandStep::NeedPoint
        } else {
            CommandStep::Cancel
        }
    }

    fn wants_text_input(&self) -> bool {
        true
    }

    fn on_text_input(&mut self, text: &str) -> CommandStep {
        let kw = text.trim().to_ascii_uppercase();
        let mut consumed = false;
        match &self.step {
            Step::TwoPointPlace { .. } | Step::LinePlace { .. } => match kw.as_str() {
                "A" | "ALIGNED" => {
                    self.set_place_mode(PlaceMode::Aligned);
                    consumed = true;
                }
                "H" | "HORIZONTAL" => {
                    self.set_place_mode(PlaceMode::Horizontal);
                    consumed = true;
                }
                "V" | "VERTICAL" => {
                    self.set_place_mode(PlaceMode::Vertical);
                    consumed = true;
                }
                _ => {}
            },
            Step::AngularPlace { .. } => match kw.as_str() {
                "I" | "INTERIOR" | "MINOR" => {
                    self.set_major(false);
                    consumed = true;
                }
                "R" | "REFLEX" | "MAJOR" => {
                    self.set_major(true);
                    consumed = true;
                }
                _ => {}
            },
            Step::Circle { .. } => match kw.as_str() {
                "D" | "DIAMETER" => {
                    self.set_circle_mode(CircleMode::Diameter);
                    consumed = true;
                }
                "R" | "RADIUS" => {
                    self.set_circle_mode(CircleMode::Radius);
                    consumed = true;
                }
                _ => {}
            },
            Step::Arc { .. } => match kw.as_str() {
                "R" | "RADIUS" => {
                    self.set_arc_mode(ArcMode::Radius);
                    consumed = true;
                }
                "A" | "ARCLENGTH" | "LENGTH" => {
                    self.set_arc_mode(ArcMode::ArcLength);
                    consumed = true;
                }
                _ => {}
            },
            _ => {}
        }
        if consumed {
            CommandStep::NeedPoint
        } else {
            // 非关键字：让宿主按常规解释（实体拾取步骤 → 句柄读取）。
            CommandStep::Ignored
        }
    }

    fn needs_object_pick(&self) -> bool {
        true
    }

    /// 拾取点模式（默认）跑对象捕捉；线段点选模式（Enter 切换）关闭，
    /// 直接点选直线/圆/圆弧（光标只留拾取方框）。
    fn entity_pick_applies_osnap(&self) -> bool {
        matches!(self.pick_mode, PickMode::Points)
    }

    fn on_object_pick_snapped(
        &mut self,
        handle: acadrust::Handle,
        pt: [f64; 3],
        snapped: bool,
    ) -> CommandStep {
        use acadrust::entities::EntityType as E;
        let kind = if handle.is_null() {
            None
        } else {
            self.classify(handle)
        };
        let is_line = matches!(kind, Some(PickKind::Line(_)));
        let segment_select = matches!(self.pick_mode, PickMode::Segments);
        // Step 的字段全部 Copy，先取一份再改 self.step。
        let step = self.step;
        match step {
            Step::First => {
                // 线段点选模式：只认实体点击（无 OSNAP，snapped 恒 false）；
                // 空白点击忽略。拾取点模式：吸附点/空白=拾取点。
                if segment_select && handle.is_null() {
                    return CommandStep::NeedPoint;
                }
                match kind {
                    Some(PickKind::Circle(c)) if !snapped => {
                        self.resolve_style(pt);
                        self.step = Step::Circle {
                            circle: c,
                            mode: CircleMode::Diameter,
                        };
                    }
                    Some(PickKind::Arc(a)) if !snapped => {
                        self.resolve_style(pt);
                        self.step = Step::Arc {
                            arc: a,
                            mode: ArcMode::Radius,
                        };
                    }
                    Some(PickKind::Line(l)) if !snapped => {
                        self.resolve_style(pt);
                        self.step = Step::LinePlace {
                            line: l,
                            mode: PlaceMode::Aligned,
                        };
                    }
                    _ => {
                        // 拾取点（吸附点 / 自由点击 / 非几何实体点击）。
                        self.resolve_style(pt);
                        self.step = Step::TwoPoints { first: pt };
                    }
                }
            }
            Step::TwoPoints { first } => {
                self.step = Step::TwoPointPlace {
                    first,
                    second: pt,
                    mode: PlaceMode::Aligned,
                };
            }
            Step::TwoPointPlace { .. } | Step::LinePlace { .. } => {
                // 点击另一条（不平行）直线 → 角度标注；否则放置。
                if is_line && !snapped {
                    let line1 = match step {
                        Step::TwoPointPlace { first, second, .. } => LineGeom {
                            start: first,
                            end: second,
                        },
                        Step::LinePlace { line, .. } => line,
                        _ => unreachable!(),
                    };
                    let Some(PickKind::Line(line2)) = kind else {
                        unreachable!()
                    };
                    if lines_parallel(&line1, &line2) {
                        self.parallel_notice = true;
                        return CommandStep::NeedPoint;
                    }
                    self.parallel_notice = false;
                    self.step = Step::AngularPlace {
                        line1,
                        line2,
                        major: false,
                    };
                    return CommandStep::NeedPoint;
                }
                self.parallel_notice = false;
                let dim = match step {
                    Step::TwoPointPlace { first, second, mode } => {
                        self.build_linear(first, second, pt, mode)
                    }
                    Step::LinePlace { line, mode } => {
                        self.build_linear(line.start, line.end, pt, mode)
                    }
                    _ => unreachable!(),
                };
                return CommandStep::CommitAndEnd(E::Dimension(dim));
            }
            Step::AngularPlace {
                line1,
                line2,
                major,
            } => {
                let dim = self.build_angular(line1, line2, pt, major);
                return CommandStep::CommitAndEnd(E::Dimension(dim));
            }
            Step::Circle { circle, mode } => {
                let dim = self.build_radial(circle.center, circle.radius, pt, mode);
                return CommandStep::CommitAndEnd(E::Dimension(dim));
            }
            Step::Arc { arc, mode } => {
                let dim = self.build_arc(&arc, pt, mode);
                return CommandStep::CommitAndEnd(E::Dimension(dim));
            }
        }
        CommandStep::NeedPoint
    }

    fn wants_mouse_move(&self) -> bool {
        !matches!(self.step, Step::First)
    }

    fn on_mouse_move(&mut self, pt: [f64; 3]) -> Option<acadrust::EntityType> {
        use acadrust::entities::{Line, EntityType as E};
        match &self.step {
            Step::First => None,
            Step::TwoPoints { first } => {
                let mut l = Line::new();
                l.start = v3(*first);
                l.end = v3(pt);
                Some(E::Line(l))
            }
            Step::TwoPointPlace { first, second, mode } => {
                Some(E::Dimension(self.build_linear(*first, *second, pt, *mode)))
            }
            Step::LinePlace { line, mode } => {
                Some(E::Dimension(self.build_linear(line.start, line.end, pt, *mode)))
            }
            Step::AngularPlace { line1, line2, major } => Some(E::Dimension(
                self.build_angular(*line1, *line2, pt, *major),
            )),
            Step::Circle { circle, mode } => Some(E::Dimension(self.build_radial(
                circle.center,
                circle.radius,
                pt,
                *mode,
            ))),
            Step::Arc { arc, mode } => {
                Some(E::Dimension(self.build_arc(arc, pt, *mode)))
            }
        }
    }
}

// ── 几何/样式辅助 ───────────────────────────────────────────────────────────

pub(crate) fn v3(p: [f64; 3]) -> Vector3 {
    Vector3::new(p[0], p[1], p[2])
}

pub(crate) fn to_arr(v: &Vector3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

/// 标注实体盖上图层（7标注层）+ 样式。
pub(crate) fn stamp(mut dim: Dimension, style_name: &str) -> Dimension {
    let base = dim.base_mut();
    base.common.layer = "7标注层".to_string();
    base.style_name = style_name.to_string();
    dim
}

/// 线性标注文字位置（镜像宿主 linear_dim.rs 的 linear_text_pos）。
pub(crate) fn linear_text_pos(first: Vector3, second: Vector3, def: Vector3, axis: Vector3) -> Vector3 {
    let perp = Vector3::new(-axis.y, axis.x, 0.0);
    let dperp = def.x * perp.x + def.y * perp.y;
    let d1 = first + perp * (dperp - (first.x * perp.x + first.y * perp.y));
    let d2 = second + perp * (dperp - (second.x * perp.x + second.y * perp.y));
    (d1 + d2) * 0.5 + perp * 0.15
}

/// 从中心朝 `pt` 方向的圆周点。
pub(crate) fn rim_toward(center: Vector3, pt: Vector3, radius: f64) -> Vector3 {
    let dir = pt - center;
    if dir.length() < 1e-9 {
        return center + Vector3::new(radius, 0.0, 0.0);
    }
    center + dir.normalize() * radius
}

/// 两直线是否平行（叉积 ≈ 0）。
fn lines_parallel(a: &LineGeom, b: &LineGeom) -> bool {
    let u = v3(a.end) - v3(a.start);
    let v = v3(b.end) - v3(b.start);
    let cross = u.x * v.y - u.y * v.x;
    cross.abs() <= 1e-9 * u.length().max(v.length()).max(1.0)
}

/// 两直线交点（平行返回 None）。
pub(crate) fn line_intersection_vertex(a: &LineGeom, b: &LineGeom) -> Option<Vector3> {
    let u = v3(a.end) - v3(a.start);
    let v = v3(b.end) - v3(b.start);
    let denom = u.x * v.y - u.y * v.x;
    if denom.abs() <= 1e-9 * u.length().max(v.length()).max(1.0) {
        return None;
    }
    let w = v3(b.start) - v3(a.start);
    let t = (w.x * v.y - w.y * v.x) / denom;
    Some(v3(a.start) + u * t)
}

/// 镜像宿主 two_line_angle_frame 的选择逻辑：以 `arc_pt` 为弧点，返回含该点
/// 的最小劣弧扫过角（度）。保证标注文字与绘制弧一致。
pub(crate) fn angular_minor_sweep_deg(a: &LineGeom, b: &LineGeom, arc_pt: [f64; 3]) -> f64 {
    const TAU: f64 = std::f64::consts::TAU;
    const PI: f64 = std::f64::consts::PI;
    let Some(vertex) = line_intersection_vertex(a, b) else {
        return 0.0;
    };
    let u = v3(a.end) - v3(a.start);
    let v = v3(b.end) - v3(b.start);
    let target = (v3(arc_pt) - vertex).y.atan2((v3(arc_pt) - vertex).x);
    let angle_of = |d: Vector3| d.y.atan2(d.x);
    let mut best: Option<f64> = None;
    for su in [1.0, -1.0] {
        for sv in [1.0, -1.0] {
            let (from_u, from_v) = (angle_of(u * su), angle_of(v * sv));
            for (start, end) in [(from_u, from_v), (from_v, from_u)] {
                let mut sweep = end - start;
                while sweep < 0.0 {
                    sweep += TAU;
                }
                if sweep <= 1e-9 || sweep > PI {
                    continue;
                }
                let into_target = (target - start).rem_euclid(TAU);
                if into_target > sweep {
                    continue;
                }
                if best.is_none_or(|known| sweep < known) {
                    best = Some(sweep);
                }
            }
        }
    }
    best.unwrap_or(0.0).to_degrees()
}

/// 模型空间里含 `pt` 的（最内层）图框 Insert 的均匀缩放比例；无命中 = 1.0。
/// 候选：块名 ∈ frame 目录 stem，或块内含 tag 比例/SCALE 的 ATTDEF。
pub(crate) fn frame_scale_at(doc: &acadrust::CadDocument, pt: [f64; 3]) -> f64 {
    use acadrust::EntityType as E;
    let stems = frame_dir_stems();
    let mut best: Option<(f64, f64)> = None; // (AABB 面积, scale)
    for e in doc.entities() {
        let E::Insert(ins) = e else { continue };
        let Some(scale) = ins.uniform_scale() else { continue };
        if !is_frame_insert(doc, ins, &stems) {
            continue;
        }
        let Some((min, max)) = insert_world_aabb(doc, ins) else {
            continue;
        };
        if pt[0] < min.x || pt[0] > max.x || pt[1] < min.y || pt[1] > max.y {
            continue;
        }
        let area = (max.x - min.x).max(0.0) * (max.y - min.y).max(0.0);
        if best.is_none_or(|(a, _)| area < a) {
            best = Some((area, scale));
        }
    }
    best.map(|(_, s)| s).unwrap_or(1.0)
}

/// 是否为图框块引用（帧目录 stem 或含 比例/SCALE ATTDEF）。
fn is_frame_insert(
    doc: &acadrust::CadDocument,
    ins: &acadrust::entities::Insert,
    stems: &[String],
) -> bool {
    use acadrust::EntityType as E;
    if stems
        .iter()
        .any(|s| s.eq_ignore_ascii_case(&ins.block_name))
    {
        return true;
    }
    let Some(br) = doc.block_records.get(&ins.block_name) else {
        return false;
    };
    br.entity_handles.iter().any(|h| {
        doc.get_entity(*h).is_some_and(|e| {
            matches!(
                e,
                E::AttributeDefinition(ad)
                    if ad.tag.eq_ignore_ascii_case("比例")
                        || ad.tag.eq_ignore_ascii_case("SCALE")
            )
        })
    })
}

/// Insert 的世界 AABB：块局部 AABB 四角经插入变换。
pub(crate) fn insert_world_aabb(
    doc: &acadrust::CadDocument,
    ins: &acadrust::entities::Insert,
) -> Option<(Vector3, Vector3)> {
    use acadrust::EntityType as E;
    let br = doc.block_records.get(&ins.block_name)?;
    let mut min: Option<Vector3> = None;
    let mut max: Option<Vector3> = None;
    for h in &br.entity_handles {
        let Some(e) = doc.get_entity(*h) else { continue };
        if matches!(e, E::Block(_) | E::BlockEnd(_)) {
            continue;
        }
        let bb = e.as_entity().bounding_box();
        if !bb.min.x.is_finite() || !bb.max.x.is_finite() {
            continue;
        }
        min = Some(match min {
            Some(m) => Vector3::new(m.x.min(bb.min.x), m.y.min(bb.min.y), m.z.min(bb.min.z)),
            None => bb.min,
        });
        max = Some(match max {
            Some(m) => Vector3::new(m.x.max(bb.max.x), m.y.max(bb.max.y), m.z.max(bb.max.z)),
            None => bb.max,
        });
    }
    let (min, max) = (min?, max?);
    let xf = ins.get_transform();
    // 8 角（Z 忽略，但变换保持完整以兼容旋转）。
    let corners = [
        Vector3::new(min.x, min.y, min.z),
        Vector3::new(max.x, min.y, min.z),
        Vector3::new(min.x, max.y, min.z),
        Vector3::new(max.x, max.y, min.z),
    ];
    let mut wmin: Option<Vector3> = None;
    let mut wmax: Option<Vector3> = None;
    for c in corners {
        let w = xf.apply(c);
        wmin = Some(match wmin {
            Some(m) => Vector3::new(m.x.min(w.x), m.y.min(w.y), m.z.min(w.z)),
            None => w,
        });
        wmax = Some(match wmax {
            Some(m) => Vector3::new(m.x.max(w.x), m.y.max(w.y), m.z.max(w.z)),
            None => w,
        });
    }
    Some((wmin?, wmax?))
}

/// frame 目录里所有 DWG 的文件名（不含扩展名），用于识别图框块。
fn frame_dir_stems() -> Vec<String> {
    let mut stems: Vec<String> = std::fs::read_dir(frame_dir())
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    e.path()
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|x| x.eq_ignore_ascii_case("dwg"))
                        .unwrap_or(false)
                })
                .filter_map(|e| {
                    e.path()
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                })
                .collect()
        })
        .unwrap_or_default();
    stems.sort();
    stems.dedup();
    stems
}

// ── 标准件库（parts/）─────────────────────────────────────────────────────
//
// 库契约见 `docs`/`OCSMBOM-plan.md` §2：每个标准件一个 DWG（含 ATTDEF +
// 正确基点），可选一份 `catalog.csv` 索引。此处只负责"列出库里有什么"，
// 真正的块导入/属性写入由命令路径完成（见 §9 实施清单 2~5 步）。

/// 标准件库里的一项。
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // 二期（序号）/三期（明细表）接线后用满
pub(crate) struct PartEntry {
    /// 类别（如 `bolt` / `nut` / `washer`）。
    pub category: String,
    /// 标准号（如 `GB/T 5782`）。
    pub std: String,
    /// 名称（如 `六角头螺栓`）；无索引时回退为文件名主干。
    pub name: String,
    /// 规格（如 `M10x40`）。
    pub spec: String,
    /// 材料（索引可选）。
    pub material: String,
    /// 单件重量（索引可选，文本，保留库里的写法）。
    pub weight: String,
    /// DWG 绝对路径。
    pub path: String,
}

/// 标准件文件夹：优先 `OCSM_PARTS_DIR` 环境变量（测试/排障），否则为插件
/// 安装目录下的 `parts/`。与 `frame_dir()` 同一套路。
#[allow(dead_code)]
fn parts_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("OCSM_PARTS_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            return std::path::PathBuf::from(dir);
        }
    }
    let mut args = std::env::args();
    while let Some(arg) = args.next() {
        if arg == "--ocs-plugin-runner" {
            if let (Some(_socket), Some(lib)) = (args.next(), args.next()) {
                let p = std::path::PathBuf::from(lib);
                if let Some(dir) = p.parent() {
                    return dir.join("parts");
                }
            }
        }
    }
    std::path::PathBuf::from("parts")
}

/// 文件名兜底解析：`<类别>_<标准号>_<规格>.dwg` → `(类别, 标准号, 规格)`。
/// 段数不足或为空时该位留空（不报错：库文件不规范也要能列出来）。
#[allow(dead_code)]
fn parse_part_stem(stem: &str) -> (String, String, String) {
    let mut it = stem.split('_').map(|x| x.trim());
    let category = it.next().unwrap_or_default().to_string();
    let std = it.next().unwrap_or_default().to_string();
    // 规格里允许出现 `_`（如 `M10_40`）→ 剩余部分全部接回，保真。
    let spec = it.collect::<Vec<_>>().join("_");
    (category, std, spec)
}

/// 解析 `catalog.csv` 文本。列序：`类别,标准号,名称,规格,材料,单重,文件名`。
/// - 首行若以「类别」开头视为表头跳过；`#` 开头为注释；空行忽略。
/// - 列数不足的行走"能填多少填多少"（不丢弃，避免库文件一点点不规范就消失）。
#[allow(dead_code)]
fn parse_catalog_csv(text: &str) -> Vec<(String, String, String, String, String, String, String)> {
    let mut rows = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<String> = line
            .split(',')
            .map(|c| c.trim().trim_matches('"').to_string())
            .collect();
        if idx == 0 && cols.first().map(|c| c.as_str()) == Some("类别") {
            continue; // 表头
        }
        let get = |i: usize| cols.get(i).cloned().unwrap_or_default();
        let file = get(6);
        if file.is_empty() {
            continue; // 没有文件名 → 无法插入，跳过
        }
        rows.push((
            get(0),
            get(1),
            get(2),
            get(3),
            get(4),
            get(5),
            file,
        ));
    }
    rows
}

/// 扫描指定目录（纯函数，便于测试；`parts_scan()` 传 `parts_dir()`）。
/// 优先用 `catalog.csv`（只保留实际存在的 DWG）；没有索引则扫 `*.dwg`
/// 并用文件名兜底解析。
#[allow(dead_code)]
fn parts_scan_in(dir: &std::path::Path) -> Vec<PartEntry> {
    let dwg_names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    e.path()
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|x| x.eq_ignore_ascii_case("dwg"))
                        .unwrap_or(false)
                })
                .filter_map(|e| e.path().file_name().map(|s| s.to_string_lossy().into_owned()))
                .collect()
        })
        .unwrap_or_default();

    let mut out: Vec<PartEntry> = Vec::new();

    if let Ok(text) = std::fs::read_to_string(dir.join("catalog.csv")) {
        for (category, std, name, spec, material, weight, file) in parse_catalog_csv(&text) {
            // 文件名对不上（大小写不敏感）时按原样试拼，仍不存在则跳过。
            let matched = dwg_names
                .iter()
                .find(|n| n.eq_ignore_ascii_case(&file))
                .cloned()
                .unwrap_or_else(|| file.clone());
            let path = dir.join(&matched);
            if !path.exists() {
                continue;
            }
            out.push(PartEntry {
                category,
                std,
                name,
                spec,
                material,
                weight,
                path: path.to_string_lossy().into_owned(),
            });
        }
    } else {
        for name in &dwg_names {
            let stem = std::path::Path::new(name)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let (category, std, spec) = parse_part_stem(&stem);
            out.push(PartEntry {
                category,
                std,
                name: stem.clone(),
                spec,
                material: String::new(),
                weight: String::new(),
                path: dir.join(name).to_string_lossy().into_owned(),
            });
        }
    }

    out.sort_by(|a, b| {
        (&a.category, &a.std, &a.spec, &a.path).cmp(&(&b.category, &b.std, &b.spec, &b.path))
    });
    out
}

/// 扫描标准件库（`parts_dir()`）。
#[allow(dead_code)]
pub(crate) fn parts_scan() -> Vec<PartEntry> {
    parts_scan_in(&parts_dir())
}

/// 表面粗糙度交互：点选插入点 → 打开粗糙度 GUI（rough.html?x=&y=）。
struct RoughnessPlace {
    sender: std::sync::Arc<dyn PluginRequestSender>,
}

impl InteractiveCommand for RoughnessPlace {
    fn prompt(&self) -> String {
        "OCSM 表面粗糙度：指定符号插入点。".to_string()
    }

    fn on_point(&mut self, pt: [f64; 3]) -> CommandStep {
        // 确保标注更新服务器在跑（GUIDE_PORT 进程级防重），打开 GUI。
        let sender = self.sender.clone();
        let mut slot = GUIDE_PORT
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .unwrap();
        let port = if let Some(p) = *slot {
            p
        } else {
            let Some(server) = crate::guide_server::spawn(sender) else {
                return CommandStep::Cancel;
            };
            *slot = Some(server.port);
            server.port
        };
        drop(slot);
        open_plugin_page(
            port,
            &format!("/rough.html?x={}&y={}", pt[0], pt[1]),
            "rough",
            900,
            880,
        );
        CommandStep::Cancel
    }
}

/// 标准件插入：点选插入点 → 记下点 + 打开选择器页（族/规格/视图由页面挑）。
/// 与粗糙度同构：点选回调里**不能**发宿主请求，写图统一由页面 `/api/part_pick` 触发。
struct PartPlace {
    sender: std::sync::Arc<dyn PluginRequestSender>,
    /// 放置阶段（`Cell`：prompt 只用 &self）
    phase: std::cell::Cell<PlacePhase>,
    /// 已定位的基点
    base: std::cell::Cell<[f64; 3]>,
}

/// 放置阶段：`Follow` = 零件跟光标（未定位基点）；`Rotate` = 已定位基点、跟随光标绕基点旋转。
#[derive(Clone, Copy, PartialEq, Eq)]
enum PlacePhase {
    Follow,
    Rotate,
}

impl InteractiveCommand for PartPlace {
    fn prompt(&self) -> String {
        match (pending_part_label(), self.phase.get()) {
            (None, _) => "OCSM 标准件：请在零件库窗口里点「零件出库」，然后在此点击放置。".to_string(),
            (Some(l), PlacePhase::Follow) => {
                format!("OCSM 标准件：{l} —— 点击定位基点（可连续，Esc 结束）")
            }
            (Some(l), PlacePhase::Rotate) => format!(
                "OCSM 标准件：{l} —— 移动光标绕基点旋转，再点击落定（Esc 取消）"
            ),
        }
    }

    /// 光标跟随预览（与 POWERDIM 同机制）：定位前跟光标走，定位后绕基点旋转。
    fn wants_mouse_move(&self) -> bool {
        true
    }

    fn on_mouse_move(&mut self, pt: [f64; 3]) -> Option<acadrust::EntityType> {
        let block = pending_block()?;
        let (at, rot) = match self.phase.get() {
            PlacePhase::Follow => (pt, 0.0),
            PlacePhase::Rotate => (self.base.get(), rotate_angle(self.base.get(), pt)),
        };
        let mut ins = acadrust::entities::Insert::new(&block, Vector3::new(at[0], at[1], at[2]));
        ins.rotation = rot;
        {
            let c = &mut ins.common;
            c.layer = partgen::LAYER_MAIN.to_string();
            c.color = Color::ByLayer;
            c.linetype = "ByLayer".to_string();
            c.line_weight = LineWeight::ByLayer;
        }
        Some(acadrust::EntityType::Insert(ins))
    }

    fn on_point(&mut self, pt: [f64; 3]) -> CommandStep {
        if pending_block().is_none() {
            // 还没出库：只进入放置态等待（兼容旧的"先点后选"路径）
            *parts_point_slot().lock().unwrap() = Some(pt);
            return CommandStep::NeedPoint;
        }
        match self.phase.get() {
            // 第一下：定位基点，转入旋转阶段
            PlacePhase::Follow => {
                self.base.set(pt);
                self.phase.set(PlacePhase::Rotate);
                CommandStep::NeedPoint
            }
            // 第二下：落定（交给放置 worker 发宿主请求：INSERT + OCSM_PART + 标脏）
            PlacePhase::Rotate => {
                let task = PlaceTask {
                    pt: self.base.get(),
                    rotation: rotate_angle(self.base.get(), pt),
                };
                let _ = place_sender(self.sender.clone()).send(task);
                self.phase.set(PlacePhase::Follow);
                CommandStep::NeedPoint
            }
        }
    }
}

/// 绕基点旋转角（弧度）：基点 → 光标方向；光标离基点太近（<1 mm）视为 0，避免抖动。
fn rotate_angle(base: [f64; 3], pt: [f64; 3]) -> f64 {
    let (dx, dy) = (pt[0] - base[0], pt[1] - base[1]);
    if (dx * dx + dy * dy).sqrt() < 1.0 {
        0.0
    } else {
        dy.atan2(dx)
    }
}

/// 交互插入：点击给出插入点 → 构造带属性的 Insert 交给宿主提交。
struct FramePlace {
    block_name: String,
    scale: f64,
    scale_text: String,
    attdefs: Vec<acadrust::entities::AttributeDefinition>,
}

impl InteractiveCommand for FramePlace {
    fn prompt(&self) -> String {
        format!(
            "指定图框「{}」的插入点（比例 {}）：",
            self.block_name, self.scale_text
        )
    }

    fn on_point(&mut self, pt: [f64; 3]) -> CommandStep {
        use acadrust::entities::{AttributeEntity, Entity, Insert};
        use acadrust::types::Vector3;

        let mut ins = Insert::new(&self.block_name, Vector3::new(pt[0], pt[1], pt[2]));
        ins.set_x_scale(self.scale);
        ins.set_y_scale(self.scale);
        ins.set_z_scale(self.scale);
        let xform = ins.get_transform();
        for ad in &self.attdefs {
            let is_scale = ad.tag.eq_ignore_ascii_case("比例")
                || ad.tag.eq_ignore_ascii_case("SCALE");
            let value = if is_scale {
                self.scale_text.clone()
            } else {
                ad.default_value.clone()
            };
            let mut attr = AttributeEntity::from_definition(ad, Some(value));
            attr.apply_transform(&xform);
            ins.attributes.push(attr);
        }
        CommandStep::CommitAndEnd(acadrust::EntityType::Insert(ins))
    }
}

export_plugin!(OcsmPlugin);

#[cfg(test)]
mod tests {
    // ── 标准件库扫描 ──────────────────────────────────────────────
    #[test]
    fn parse_part_stem_splits_three_parts() {
        assert_eq!(
            parse_part_stem("bolt_GBT5782_M10x40"),
            ("bolt".to_string(), "GBT5782".to_string(), "M10x40".to_string())
        );
        // 规格里含 `_` → 剩余段全部接回（保真）。
        assert_eq!(
            parse_part_stem("bearing_GBT276_M10_40"),
            ("bearing".to_string(), "GBT276".to_string(), "M10_40".to_string())
        );
        // 段数不足：缺的空着，不报错。
        assert_eq!(
            parse_part_stem("washer"),
            ("washer".to_string(), String::new(), String::new())
        );
    }

    #[test]
    fn parse_catalog_csv_skips_header_comments_and_blanks() {
        let text = "类别,标准号,名称,规格,材料,单重,文件名\n\
                    # 这是注释\n\
                    \n\
                    bolt,GB/T 5782,六角头螺栓,M10x40,8.8,31.5,bolt_GBT5782_M10x40.dwg\n\
                    nut,GB/T 6170,1型六角螺母,M10,8,11,\n";
        let rows = parse_catalog_csv(text);
        assert_eq!(rows.len(), 1, "表头/注释/空行/无文件名行都应被跳过");
        assert_eq!(rows[0].0, "bolt");
        assert_eq!(rows[0].2, "六角头螺栓");
        assert_eq!(rows[0].6, "bolt_GBT5782_M10x40.dwg");
    }

    #[test]
    fn parts_scan_without_index_falls_back_to_filename() {
        let dir = std::env::temp_dir().join(format!("ocsm_parts_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp parts dir");
        std::fs::write(dir.join("bolt_GBT5782_M10x40.dwg"), b"not a real dwg").unwrap();
        std::fs::write(dir.join("readme.txt"), b"ignore me").unwrap();

        let entries = parts_scan_in(&dir);
        assert_eq!(entries.len(), 1, "非 dwg 文件必须被忽略");
        assert_eq!(entries[0].category, "bolt");
        assert_eq!(entries[0].std, "GBT5782");
        assert_eq!(entries[0].spec, "M10x40");
        assert_eq!(entries[0].name, "bolt_GBT5782_M10x40", "无索引时名称回退为文件名主干");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parts_scan_with_index_uses_catalog_and_drops_missing_files() {
        let dir = std::env::temp_dir().join(format!("ocsm_parts_idx_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp parts dir");
        std::fs::write(dir.join("bolt_GBT5782_M10x40.dwg"), b"x").unwrap();
        std::fs::write(
            dir.join("catalog.csv"),
            "类别,标准号,名称,规格,材料,单重,文件名\n\
             bolt,GB/T 5782,六角头螺栓,M10x40,8.8级,31.5,bolt_GBT5782_M10x40.dwg\n\
             nut,GB/T 6170,1型六角螺母,M10,8级,11,nut_GBT6170_M10.dwg\n",
        )
        .unwrap();

        let entries = parts_scan_in(&dir);
        assert_eq!(entries.len(), 1, "索引里缺失的 DWG 必须被跳过");
        assert_eq!(entries[0].name, "六角头螺栓", "有索引时用中文名称而非文件名");
        assert_eq!(entries[0].material, "8.8级");
        assert_eq!(entries[0].weight, "31.5");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 端口/句柄不一致时才重写链接（避免每次开图都改图纸）。
    #[test]
    fn needs_link_rewrite_only_when_stale() {
        let h = acadrust::Handle::new(0x4A);
        let good = "http://127.0.0.1:23751/guide.html?handle=0x4A";
        assert!(!needs_link_rewrite(Some(good), 23751, h), "端口与句柄都对 → 不写");
        assert!(needs_link_rewrite(Some(good), 23752, h), "端口变了 → 要写");
        assert!(
            needs_link_rewrite(Some("http://127.0.0.1:23751/guide.html?handle=0x99"), 23751, h),
            "句柄对不上（被复制过）→ 要写"
        );
        assert!(needs_link_rewrite(None, 23751, h), "没链接 → 要写");
        assert!(
            needs_link_rewrite(Some("https://example.com/x"), 23751, h),
            "无关链接 → 要写"
        );
    }

    use super::*;
    use acadrust::entities::{Arc, Circle, EntityType, Insert, Line};

    fn lw(v: i16) -> LineWeight {
        LineWeight::from_value(v)
    }

    #[test]
    fn layer_defs_match_layers_quick_template() {
        let defs = layer_defs();
        assert_eq!(defs.len(), 10);
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "1轮廓实线层",
                "2细线层",
                "3中心线层",
                "4虚线层",
                "5剖面线层",
                "6文字层",
                "7标注层",
                "8符号标注层",
                "9双点划线层",
                "10引导线层",
            ]
        );
        // 轮廓实线层 0.35mm；2~9 层 0.18mm；引导线层 0mm（不打印）。
        assert_eq!(defs[0].lineweight, lw(35));
        for d in &defs[1..9] {
            assert_eq!(d.lineweight, lw(18));
        }
        assert_eq!(defs[9].lineweight, lw(0));
        // 线型：中心线 CENTER2、虚线 DASHED2、双点划线 DIVIDE2。
        assert_eq!(defs[2].linetype, "CENTER2");
        assert_eq!(defs[3].linetype, "DASHED2");
        assert_eq!(defs[8].linetype, "DIVIDE2");
        // 符号标注层 RGB(255,191,127)。
        assert_eq!(defs[7].color, Color::from_rgb(255, 191, 127));
        // 引导线层：蓝色(5)、Continuous、默认关闭（off），其余层默认开启。
        assert_eq!(defs[9].color, Color::from_index(5));
        assert_eq!(defs[9].linetype, "Continuous");
        assert!(defs[9].plottable);
        assert!(defs[9].off);
        for d in &defs[..9] {
            assert!(!d.off, "{} 不应默认关闭", d.name);
        }
    }

    #[test]
    fn linetype_defs_cover_template_linetypes() {
        let defs = linetype_defs();
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"CENTER2"));
        assert!(names.contains(&"DASHED2"));
        assert!(names.contains(&"DIVIDE2"));
        // DIVIDE2 模式与内置 OpenCADStudio.lin 一致。
        let divide = defs.iter().find(|d| d.name == "DIVIDE2").unwrap();
        assert_eq!(divide.elements, vec![6.35, -3.175, 0.0, -3.175, 0.0, -3.175]);
    }

    #[test]
    fn text_style_def_is_ocsm_gb() {
        let defs = text_style_defs();
        assert_eq!(defs.len(), 1);
        let s = &defs[0];
        assert_eq!(s.name, "OCSM_GB");
        assert_eq!(s.height, 3.5);
        assert_eq!(s.width_factor, 0.7);
        assert!(s.annotative);
        assert_eq!(s.font_file, "Unicode");
        assert_eq!(s.true_type_font, "Zhuque Fangsong");
    }

    #[test]
    fn scale_factor_semantics() {
        // 1:2 → 2x；2:1 → 0.5x。
        let factor = |v1: i64, v2: i64| v2 as f64 / v1 as f64;
        assert!((factor(1, 2) - 2.0).abs() < 1e-9);
        assert!((factor(2, 1) - 0.5).abs() < 1e-9);
        assert!((factor(1, 1) - 1.0).abs() < 1e-9);
        assert!((factor(1, 100) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn scale_validation_rule_one_must_be_one() {
        let valid = |v1: i64, v2: i64| v1 > 0 && v2 > 0 && (v1 == 1 || v2 == 1);
        assert!(valid(1, 2));
        assert!(valid(2, 1));
        assert!(valid(1, 1));
        assert!(!valid(2, 2));
        assert!(!valid(3, 2));
        assert!(!valid(0, 1));
        assert!(!valid(1, 0));
        assert!(!valid(-1, 1));
    }

    #[test]
    fn frame_dir_prefers_env_override() {
        // OCSM_FRAME_DIR 优先于安装目录解析。
        unsafe { std::env::set_var("OCSM_FRAME_DIR", "/tmp/ocsm-frames") };
        assert_eq!(frame_dir(), std::path::PathBuf::from("/tmp/ocsm-frames"));
        unsafe { std::env::remove_var("OCSM_FRAME_DIR") };
    }

    #[test]
    fn frame_place_on_point_builds_insert_with_attributes() {
        use acadrust::entities::AttributeDefinition;
        let attdefs = vec![
            AttributeDefinition::new("图号".into(), "Drawing No.".into(), " ".into()),
            AttributeDefinition::new("比例".into(), "Scale".into(), " ".into()),
        ];
        let mut cmd = FramePlace {
            block_name: "a3_landscape_att".into(),
            scale: 2.0,
            scale_text: "1:2".into(),
            attdefs,
        };
        match cmd.on_point([100.0, 200.0, 0.0]) {
            CommandStep::CommitAndEnd(acadrust::EntityType::Insert(ins)) => {
                assert_eq!(ins.block_name, "a3_landscape_att");
                assert_eq!(ins.uniform_scale(), Some(2.0));
                assert_eq!(ins.insert_point.x, 100.0);
                assert_eq!(ins.insert_point.y, 200.0);
                assert_eq!(ins.attributes.len(), 2);
                // 属性顺序与 ATTDEF 一致；比例属性填入比例文本。
                assert_eq!(ins.attributes[0].tag, "图号");
                assert_eq!(ins.attributes[0].value, " ");
                assert_eq!(ins.attributes[1].tag, "比例");
                assert_eq!(ins.attributes[1].value, "1:2");
            }
            other => panic!("expected CommitAndEnd Insert, got {other:?}"),
        }
    }

    // ── 标注样式 ────────────────────────────────────────────────────────────

    #[test]
    fn dim_style_def_is_ocsm_gb_mechanical() {
        let defs = dim_style_defs();
        assert_eq!(defs.len(), 1);
        let d = &defs[0];
        // 参数来自用户示例（标注对比.dxf 的 Mechanical 风格，Phase 0 分析）。
        assert_eq!(d.name, "OCSM_GB");
        assert!(d.make_current);
        assert_eq!(d.dimtxt, 2.5);
        assert_eq!(d.dimasz, 2.5);
        assert_eq!(d.dimcen, 2.5);
        assert_eq!(d.dimexe, 2.0);
        assert!((d.dimexo - 0.625).abs() < 1e-9);
        assert_eq!(d.dimgap, 1.0);
        // 与 general.dxf DSTYLE XDATA 一致（渲染实际值）。
        assert!((d.dimdli - 0.38).abs() < 1e-9);
        assert_eq!(d.dimscale, 1.0);
        assert_eq!(d.dimtad, 1);
        assert_eq!(d.dimtoh, true); // 文字强制水平（general.dxf dimtoh=1）
        assert_eq!(d.dimtofl, true);
        assert_eq!(d.dimdec, 2); // 2 位小数（用户指定：原为 4）
        assert_eq!(d.dimlunit, 2);
        assert_eq!(d.dimzin, 8);
        assert_eq!(d.dimclrd, 130);
        assert_eq!(d.dimclre, 130);
        assert_eq!(d.dimclrt, 3);
        assert_eq!(d.dimlwd, -1);
        assert_eq!(d.dimlwe, -1);
        // 字体必须是 OCSM_GB（project.md：标注字体换成 OCSM_GB）。
        assert_eq!(d.dimtxsty, "OCSM_GB");
        assert!(!d.annotative);
    }

    #[test]
    fn scaled_dim_style_keeps_base_params_with_dimscale() {
        let s = scaled_dim_style_def(2.0);
        assert_eq!(s.name, "OCSM_GB_x2");
        assert_eq!(s.dimscale, 2.0);
        assert_eq!(s.dimtxt, 2.5); // 尺寸不变，由 dimscale 放大
        assert!(!s.make_current);
        let s = scaled_dim_style_def(0.5);
        assert_eq!(s.name, "OCSM_GB_x0.5");
        assert_eq!(s.dimscale, 0.5);
        assert_eq!(trim_scale(1.0), "1");
        assert_eq!(trim_scale(2.5), "2.5");
        assert_eq!(trim_scale(0.125), "0.125");
    }

    // ── frame 比例感知 ───────────────────────────────────────────────────────

    /// 合成文档：一个 100×50 的图框块（含 比例 ATTDEF），模型空间插入一次。
    fn synthetic_frame_doc(scale: f64, insert_at: [f64; 2]) -> acadrust::CadDocument {
        use acadrust::entities::{
            AttributeDefinition, Block, BlockEnd, EntityType as E, Insert, Line,
        };
        use acadrust::tables::BlockRecord;
        let mut doc = acadrust::CadDocument::default();
        let mut att = AttributeDefinition::new("比例".into(), "Scale".into(), " ".into());
        att.tag = "比例".into();
        let lines: Vec<E> = vec![
            E::Line(Line {
                common: Default::default(),
                start: Vector3::new(0.0, 0.0, 0.0),
                end: Vector3::new(100.0, 0.0, 0.0),
                thickness: 0.0,
                normal: Vector3::new(0.0, 0.0, 1.0),
            }),
            E::Line(Line {
                common: Default::default(),
                start: Vector3::new(100.0, 0.0, 0.0),
                end: Vector3::new(100.0, 50.0, 0.0),
                thickness: 0.0,
                normal: Vector3::new(0.0, 0.0, 1.0),
            }),
            E::Line(Line {
                common: Default::default(),
                start: Vector3::new(100.0, 50.0, 0.0),
                end: Vector3::new(0.0, 50.0, 0.0),
                thickness: 0.0,
                normal: Vector3::new(0.0, 0.0, 1.0),
            }),
            E::Line(Line {
                common: Default::default(),
                start: Vector3::new(0.0, 50.0, 0.0),
                end: Vector3::new(0.0, 0.0, 0.0),
                thickness: 0.0,
                normal: Vector3::new(0.0, 0.0, 1.0),
            }),
            E::AttributeDefinition(att),
        ];
        let mut br = BlockRecord::new("a3_test");
        br.handle = doc.allocate_handle();
        br.entity_handles
            .push(doc.add_entity(E::Block(Block::new("a3_test", Vector3::ZERO))).unwrap());
        for e in lines {
            br.entity_handles.push(doc.add_entity(e).unwrap());
        }
        br.entity_handles
            .push(doc.add_entity(E::BlockEnd(BlockEnd::new())).unwrap());
        doc.block_records.add(br).unwrap();
        let mut ins = Insert::new("a3_test", Vector3::new(insert_at[0], insert_at[1], 0.0));
        ins.set_x_scale(scale);
        ins.set_y_scale(scale);
        ins.set_z_scale(scale);
        doc.add_entity(E::Insert(ins)).unwrap();
        doc
    }

    #[test]
    fn frame_scale_at_finds_innermost_containing_insert() {
        // 2 倍图框插在 (0,0)，块局部 100×50 → 世界 200×100。
        let doc = synthetic_frame_doc(2.0, [0.0, 0.0]);
        assert_eq!(frame_scale_at(&doc, [50.0, 25.0, 0.0]), 2.0);
        assert_eq!(frame_scale_at(&doc, [199.0, 99.0, 0.0]), 2.0);
        // 框外 → 1.0。
        assert_eq!(frame_scale_at(&doc, [201.0, 25.0, 0.0]), 1.0);
        assert_eq!(frame_scale_at(&doc, [50.0, 101.0, 0.0]), 1.0);
    }

    #[test]
    fn frame_scale_at_prefers_innermost_smaller_frame() {
        // 外层 4x（0,0 起 400×200）内含 2x（100,100 起 200×100）：取内层 2。
        let mut doc = synthetic_frame_doc(4.0, [0.0, 0.0]);
        let inner = synthetic_frame_doc(2.0, [100.0, 100.0]);
        // 合并两个文档的实体/块（简单做法：把内层块的实体也加进 doc）。
        let inner_entities: Vec<EntityType> = {
            let br = inner.block_records.get("a3_test").unwrap();
            br.entity_handles
                .iter()
                .filter_map(|h| inner.get_entity(*h).cloned())
                .collect()
        };
        let mut ins = Insert::new("a3_test", Vector3::new(100.0, 100.0, 0.0));
        ins.set_x_scale(2.0);
        ins.set_y_scale(2.0);
        ins.set_z_scale(2.0);
        doc.add_entity(EntityType::Insert(ins)).unwrap();
        // 块已存在（同名），只补实体到现有块定义里。
        let mut new_handles = Vec::new();
        for e in inner_entities {
            new_handles.push(doc.add_entity(e).unwrap());
        }
        doc.block_records.get_mut("a3_test").unwrap().entity_handles.extend(new_handles);
        // 点在内层框内 → 2；在外层框内但在内层外 → 4。
        assert_eq!(frame_scale_at(&doc, [150.0, 150.0, 0.0]), 2.0);
        assert_eq!(frame_scale_at(&doc, [350.0, 150.0, 0.0]), 4.0);
    }

    // ── POWERDIM 状态机 ───────────────────────────────────────────────────────

    #[test]
    fn powerdim_two_points_places_aligned_dimension() {
        use ocs_plugin_api::host::InteractiveCommand as Ic;
        let mut cmd = PowerDim::new(acadrust::CadDocument::default());
        // 第一次点击：自由拾取点（handle NULL）。
        assert!(matches!(
            cmd.on_object_pick_snapped(acadrust::Handle::NULL, [0.0, 0.0, 0.0], false),
            CommandStep::NeedPoint
        ));
        assert!(cmd.prompt().contains("选择下一个拾取点"));
        // 第二次点击：另一个拾取点。
        assert!(matches!(
            cmd.on_object_pick_snapped(acadrust::Handle::NULL, [100.0, 0.0, 0.0], false),
            CommandStep::NeedPoint
        ));
        assert!(cmd.prompt().contains("对齐(A默认)"));
        // 第三次点击：放置 → 对齐标注。
        match cmd.on_object_pick_snapped(acadrust::Handle::NULL, [50.0, -20.0, 0.0], false) {
            CommandStep::CommitAndEnd(acadrust::EntityType::Dimension(dim)) => {
                assert!(matches!(dim, Dimension::Aligned(_)));
                assert_eq!(dim.base().common.layer, "7标注层");
                assert_eq!(dim.base().style_name, "OCSM_GB");
                assert!((dim.base().actual_measurement - 100.0).abs() < 1e-6);
            }
            other => panic!("expected CommitAndEnd Aligned, got {other:?}"),
        }
    }

    #[test]
    fn powerdim_keywords_lock_horizontal_vertical() {
        use ocs_plugin_api::host::InteractiveCommand as Ic;
        let mut cmd = PowerDim::new(acadrust::CadDocument::default());
        let _ = cmd.on_object_pick_snapped(acadrust::Handle::NULL, [0.0, 0.0, 0.0], false);
        let _ = cmd.on_object_pick_snapped(acadrust::Handle::NULL, [100.0, 0.0, 0.0], false);
        // 关键字 H → 水平线性标注。
        assert!(matches!(cmd.on_text_input("H"), CommandStep::NeedPoint));
        match cmd.on_object_pick_snapped(acadrust::Handle::NULL, [50.0, -20.0, 0.0], false) {
            CommandStep::CommitAndEnd(acadrust::EntityType::Dimension(Dimension::Linear(d))) => {
                assert!(d.rotation.abs() < 1e-9);
                assert!((d.base.actual_measurement - 100.0).abs() < 1e-6);
            }
            other => panic!("expected horizontal Linear, got {other:?}"),
        }
        // V → 竖直。
        let mut cmd = PowerDim::new(acadrust::CadDocument::default());
        let _ = cmd.on_object_pick_snapped(acadrust::Handle::NULL, [0.0, 0.0, 0.0], false);
        let _ = cmd.on_object_pick_snapped(acadrust::Handle::NULL, [0.0, 80.0, 0.0], false);
        assert!(matches!(cmd.on_text_input("V"), CommandStep::NeedPoint));
        match cmd.on_object_pick_snapped(acadrust::Handle::NULL, [30.0, 40.0, 0.0], false) {
            CommandStep::CommitAndEnd(acadrust::EntityType::Dimension(Dimension::Linear(d))) => {
                assert!((d.rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
                assert!((d.base.actual_measurement - 80.0).abs() < 1e-6);
            }
            other => panic!("expected vertical Linear, got {other:?}"),
        }
    }

    #[test]
    fn powerdim_line_line_creates_angular_with_major() {
        use ocs_plugin_api::host::InteractiveCommand as Ic;
        let mut doc = acadrust::CadDocument::default();
        // 两条不平行线段：L1 (0,0)-(100,0)，L2 (0,0)-(0,100)。
        let h1 = doc
            .add_entity(EntityType::Line(Line {
                common: Default::default(),
                start: Vector3::new(0.0, 0.0, 0.0),
                end: Vector3::new(100.0, 0.0, 0.0),
                thickness: 0.0,
                normal: Vector3::new(0.0, 0.0, 1.0),
            }))
            .unwrap();
        let h2 = doc
            .add_entity(EntityType::Line(Line {
                common: Default::default(),
                start: Vector3::new(0.0, 0.0, 0.0),
                end: Vector3::new(0.0, 100.0, 0.0),
                thickness: 0.0,
                normal: Vector3::new(0.0, 0.0, 1.0),
            }))
            .unwrap();
        let mut cmd = PowerDim::new(doc.clone());
        // 先点 L1（无吸附）→ LinePlace。
        assert!(matches!(
            cmd.on_object_pick_snapped(h1, [50.0, 0.0, 0.0], false),
            CommandStep::NeedPoint
        ));
        // 再点 L2 → AngularPlace。
        assert!(matches!(
            cmd.on_object_pick_snapped(h2, [0.0, 50.0, 0.0], false),
            CommandStep::NeedPoint
        ));
        assert!(cmd.prompt().contains("劣角(I默认)"));
        // 劣角：点击在夹角内侧 → 90°（actual_measurement + user_text）。
        match cmd.on_object_pick_snapped(acadrust::Handle::NULL, [25.0, 25.0, 0.0], false) {
            CommandStep::CommitAndEnd(acadrust::EntityType::Dimension(Dimension::Angular2Ln(
                d,
            ))) => {
                assert!((d.base.actual_measurement - 90.0).abs() < 1e-6);
                assert_eq!(d.base.user_text.as_deref(), Some("90°"));
            }
            other => panic!("expected Angular2Ln, got {other:?}"),
        }
        // 优角：R 关键字 → 270°。
        let mut cmd = PowerDim::new(doc.clone());
        let _ = cmd.on_object_pick_snapped(h1, [50.0, 0.0, 0.0], false);
        let _ = cmd.on_object_pick_snapped(h2, [0.0, 50.0, 0.0], false);
        assert!(matches!(cmd.on_text_input("R"), CommandStep::NeedPoint));
        match cmd.on_object_pick_snapped(acadrust::Handle::NULL, [25.0, 25.0, 0.0], false) {
            CommandStep::CommitAndEnd(acadrust::EntityType::Dimension(Dimension::Angular2Ln(
                d,
            ))) => {
                assert!((d.base.actual_measurement - 270.0).abs() < 1e-6);
                assert_eq!(d.base.user_text.as_deref(), Some("270°"));
            }
            other => panic!("expected Angular2Ln major, got {other:?}"),
        }
    }

    #[test]
    fn powerdim_parallel_lines_reject_angle() {
        use ocs_plugin_api::host::InteractiveCommand as Ic;
        let mut doc = acadrust::CadDocument::default();
        let h1 = doc
            .add_entity(EntityType::Line(Line {
                common: Default::default(),
                start: Vector3::new(0.0, 0.0, 0.0),
                end: Vector3::new(100.0, 0.0, 0.0),
                thickness: 0.0,
                normal: Vector3::new(0.0, 0.0, 1.0),
            }))
            .unwrap();
        let h2 = doc
            .add_entity(EntityType::Line(Line {
                common: Default::default(),
                start: Vector3::new(0.0, 10.0, 0.0),
                end: Vector3::new(100.0, 10.0, 0.0),
                thickness: 0.0,
                normal: Vector3::new(0.0, 0.0, 1.0),
            }))
            .unwrap();
        let mut cmd = PowerDim::new(doc);
        let _ = cmd.on_object_pick_snapped(h1, [50.0, 0.0, 0.0], false);
        assert!(matches!(
            cmd.on_object_pick_snapped(h2, [50.0, 10.0, 0.0], false),
            CommandStep::NeedPoint
        ));
        assert!(cmd.parallel_notice);
        assert!(cmd.prompt().contains("两直线平行"));
    }

    #[test]
    fn powerdim_circle_inside_outside_leader() {
        use ocs_plugin_api::host::InteractiveCommand as Ic;
        let mut doc = acadrust::CadDocument::default();
        let h = doc
            .add_entity(EntityType::Circle(Circle {
                common: Default::default(),
                center: Vector3::new(50.0, 50.0, 0.0),
                radius: 20.0,
                thickness: 0.0,
                normal: Vector3::new(0.0, 0.0, 1.0),
            }))
            .unwrap();
        let mut cmd = PowerDim::new(doc.clone());
        // 点击圆（无吸附）→ 直径模式提示。
        assert!(matches!(
            cmd.on_object_pick_snapped(h, [50.0, 30.0, 0.0], false),
            CommandStep::NeedPoint
        ));
        assert!(cmd.prompt().contains("直径(D默认)"));
        // 光标在圆外 → 外标注（leader>0）。
        match cmd.on_object_pick_snapped(acadrust::Handle::NULL, [100.0, 50.0, 0.0], false) {
            CommandStep::CommitAndEnd(acadrust::EntityType::Dimension(Dimension::Diameter(d))) => {
                assert!(d.leader_length > 0.0);
                assert!((d.base.actual_measurement - 40.0).abs() < 1e-6);
            }
            other => panic!("expected Diameter, got {other:?}"),
        }
        // R 关键字 → 半径；光标在圆内 → 内标注（leader=0）。
        let mut cmd = PowerDim::new(doc.clone());
        let _ = cmd.on_object_pick_snapped(h, [50.0, 30.0, 0.0], false);
        assert!(matches!(cmd.on_text_input("R"), CommandStep::NeedPoint));
        match cmd.on_object_pick_snapped(acadrust::Handle::NULL, [55.0, 55.0, 0.0], false) {
            CommandStep::CommitAndEnd(acadrust::EntityType::Dimension(Dimension::Radius(d))) => {
                assert_eq!(d.leader_length, 0.0);
                assert!((d.base.actual_measurement - 20.0).abs() < 1e-6);
            }
            other => panic!("expected Radius, got {other:?}"),
        }
    }

    #[test]
    fn powerdim_arc_radius_and_arclength() {
        use ocs_plugin_api::host::InteractiveCommand as Ic;
        let mut doc = acadrust::CadDocument::default();
        let h = doc
            .add_entity(EntityType::Arc(Arc {
                common: Default::default(),
                center: Vector3::new(0.0, 0.0, 0.0),
                radius: 50.0,
                start_angle: 0.0,
                end_angle: std::f64::consts::FRAC_PI_2,
                thickness: 0.0,
                normal: Vector3::new(0.0, 0.0, 1.0),
            }))
            .unwrap();
        // 半径模式（默认）。
        let mut cmd = PowerDim::new(doc.clone());
        let _ = cmd.on_object_pick_snapped(h, [50.0, 0.0, 0.0], false);
        assert!(cmd.prompt().contains("半径(R默认)"));
        match cmd.on_object_pick_snapped(acadrust::Handle::NULL, [100.0, 50.0, 0.0], false) {
            CommandStep::CommitAndEnd(acadrust::EntityType::Dimension(Dimension::Radius(d))) => {
                assert!((d.base.actual_measurement - 50.0).abs() < 1e-6);
            }
            other => panic!("expected Radius, got {other:?}"),
        }
        // 弧长模式：A 关键字 → DimensionArc，弧长 = r·(π/2)。
        let mut cmd = PowerDim::new(doc.clone());
        let _ = cmd.on_object_pick_snapped(h, [50.0, 0.0, 0.0], false);
        assert!(matches!(cmd.on_text_input("A"), CommandStep::NeedPoint));
        match cmd.on_object_pick_snapped(acadrust::Handle::NULL, [80.0, 80.0, 0.0], false) {
            CommandStep::CommitAndEnd(acadrust::EntityType::Dimension(Dimension::Arc(d))) => {
                let expected = 50.0 * std::f64::consts::FRAC_PI_2;
                assert!((d.base.actual_measurement - expected).abs() < 1e-6);
                assert!(d.has_leader); // 光标在外侧
            }
            other => panic!("expected Arc, got {other:?}"),
        }
    }

    #[test]
    fn powerdim_resolve_style_uses_scaled_style_when_present() {
        use ocs_plugin_api::host::InteractiveCommand as Ic;
        use acadrust::tables::DimStyle;
        let mut doc = synthetic_frame_doc(2.0, [0.0, 0.0]);
        // 模拟 TF 已创建缩放样式：OCSM_GB_x2 存在。
        let mut style = DimStyle::new("OCSM_GB_x2");
        style.handle = doc.allocate_handle();
        style.dimscale = 2.0;
        doc.dim_styles.add_or_replace(style);
        let mut cmd = PowerDim::new(doc);
        // 第一次点击在 2x 框内 → 样式 OCSM_GB_x2。
        let _ = cmd.on_object_pick_snapped(acadrust::Handle::NULL, [50.0, 25.0, 0.0], true);
        assert_eq!(cmd.style_name, "OCSM_GB_x2");
        assert!(cmd.style_hint.is_none());
        // 缩放样式缺失时回退 OCSM_GB 并提示。
        let mut cmd = PowerDim::new(synthetic_frame_doc(4.0, [0.0, 0.0]));
        let _ = cmd.on_object_pick_snapped(acadrust::Handle::NULL, [50.0, 25.0, 0.0], true);
        assert_eq!(cmd.style_name, "OCSM_GB");
        assert!(cmd.style_hint.is_some());
    }

    #[test]
    fn powerdim_enter_toggles_segment_select_mode() {
        use ocs_plugin_api::host::InteractiveCommand as Ic;
        let mut doc = acadrust::CadDocument::default();
        let h = doc
            .add_entity(EntityType::Line(Line {
                common: Default::default(),
                start: Vector3::new(0.0, 0.0, 0.0),
                end: Vector3::new(100.0, 0.0, 0.0),
                thickness: 0.0,
                normal: Vector3::new(0.0, 0.0, 1.0),
            }))
            .unwrap();
        let mut cmd = PowerDim::new(doc);
        // 默认拾取点模式：OSNAP 开。
        assert!(cmd.entity_pick_applies_osnap());
        assert!(cmd.prompt().contains("拾取点模式"));
        // Enter → 线段点选模式：OSNAP 关。
        assert!(matches!(cmd.on_enter(), CommandStep::NeedPoint));
        assert!(!cmd.entity_pick_applies_osnap());
        assert!(cmd.prompt().contains("线段点选"));
        // 点选模式下点击直线（snapped=false）→ 立即进入线段标注。
        assert!(matches!(
            cmd.on_object_pick_snapped(h, [50.0, 0.0, 0.0], false),
            CommandStep::NeedPoint
        ));
        assert!(cmd.prompt().contains("对齐(A默认)"));
        // 放置（任意点）→ 对齐标注。
        match cmd.on_object_pick_snapped(acadrust::Handle::NULL, [50.0, -20.0, 0.0], false) {
            CommandStep::CommitAndEnd(acadrust::EntityType::Dimension(dim)) => {
                assert!(matches!(dim, Dimension::Aligned(_)));
            }
            other => panic!("expected Aligned, got {other:?}"),
        }
        // 点选模式下 First 状态：空白点击忽略。
        let mut cmd = PowerDim::new(acadrust::CadDocument::default());
        assert!(matches!(cmd.on_enter(), CommandStep::NeedPoint));
        assert!(matches!(
            cmd.on_object_pick_snapped(acadrust::Handle::NULL, [10.0, 10.0, 0.0], false),
            CommandStep::NeedPoint
        ));
        assert!(matches!(cmd.step, Step::First));
        // 再 Enter 回到拾取点模式。
        assert!(matches!(cmd.on_enter(), CommandStep::NeedPoint));
        assert!(cmd.entity_pick_applies_osnap());
    }

    #[test]
    fn powerdim_radial_text_is_user_positioned_for_outside() {
        use ocs_plugin_api::host::InteractiveCommand as Ic;
        let mut doc = acadrust::CadDocument::default();
        let h = doc
            .add_entity(EntityType::Circle(Circle {
                common: Default::default(),
                center: Vector3::new(50.0, 50.0, 0.0),
                radius: 20.0,
                thickness: 0.0,
                normal: Vector3::new(0.0, 0.0, 1.0),
            }))
            .unwrap();
        let mut cmd = PowerDim::new(doc);
        let _ = cmd.on_object_pick_snapped(h, [50.0, 30.0, 0.0], false);
        // 光标在圆外 → 外标注：leader>0 且文字跟随点击点（user_positioned）。
        match cmd.on_object_pick_snapped(acadrust::Handle::NULL, [100.0, 50.0, 0.0], false) {
            CommandStep::CommitAndEnd(acadrust::EntityType::Dimension(Dimension::Diameter(d))) => {
                assert!(d.leader_length > 0.0);
                assert!(d.base.text_user_positioned);
                assert!((d.base.text_middle_point.x - 100.0).abs() < 1e-9);
            }
            other => panic!("expected Diameter, got {other:?}"),
        }
    }

    #[test]
    fn powerdim_arc_length_uses_actual_sweep_including_major() {
        use ocs_plugin_api::host::InteractiveCommand as Ic;
        let mut doc = acadrust::CadDocument::default();
        // 270° 优弧：start=0°, end=270°。
        let h = doc
            .add_entity(EntityType::Arc(Arc {
                common: Default::default(),
                center: Vector3::new(0.0, 0.0, 0.0),
                radius: 50.0,
                start_angle: 0.0,
                end_angle: std::f64::consts::TAU * 0.75,
                thickness: 0.0,
                normal: Vector3::new(0.0, 0.0, 1.0),
            }))
            .unwrap();
        let mut cmd = PowerDim::new(doc);
        let _ = cmd.on_object_pick_snapped(h, [50.0, 0.0, 0.0], false);
        assert!(matches!(cmd.on_text_input("A"), CommandStep::NeedPoint));
        match cmd.on_object_pick_snapped(acadrust::Handle::NULL, [80.0, 80.0, 0.0], false) {
            CommandStep::CommitAndEnd(acadrust::EntityType::Dimension(Dimension::Arc(d))) => {
                let expected = 50.0 * std::f64::consts::TAU * 0.75;
                assert!(
                    (d.base.actual_measurement - expected).abs() < 1e-6,
                    "arc length should be the major sweep, got {}",
                    d.base.actual_measurement
                );
                assert!(d.is_partial, "partial sweep keeps the major arc");
                assert!(d.base.text_user_positioned);
            }
            other => panic!("expected Arc, got {other:?}"),
        }
    }

    #[test]
    fn powerdim_angular_measures_45_degrees_not_90() {
        use ocs_plugin_api::host::InteractiveCommand as Ic;
        // 45° 相交的两条线（非正交），验证角度值正确（F5 回归）。
        let minor = angular_minor_sweep_deg(
            &LineGeom {
                start: [0.0, 0.0, 0.0],
                end: [100.0, 0.0, 0.0],
            },
            &LineGeom {
                start: [0.0, 0.0, 0.0],
                end: [100.0, 100.0, 0.0],
            },
            [50.0, 20.0, 0.0],
        );
        assert!((minor - 45.0).abs() < 1e-6, "got {minor}");
    }

    #[test]
    fn powerdim_preview_follows_cursor() {
        use ocs_plugin_api::host::InteractiveCommand as Ic;
        let mut cmd = PowerDim::new(acadrust::CadDocument::default());
        // 第一步无预览。
        assert!(cmd.on_mouse_move([10.0, 10.0, 0.0]).is_none());
        let _ = cmd.on_object_pick_snapped(acadrust::Handle::NULL, [0.0, 0.0, 0.0], false);
        // 两点步骤：橡皮筋线。
        assert!(matches!(
            cmd.on_mouse_move([50.0, 30.0, 0.0]),
            Some(EntityType::Line(_))
        ));
        let _ = cmd.on_object_pick_snapped(acadrust::Handle::NULL, [100.0, 0.0, 0.0], false);
        // 放置步骤：标注实体预览（跟随光标）。
        match cmd.on_mouse_move([50.0, -20.0, 0.0]) {
            Some(EntityType::Dimension(dim)) => {
                assert!(matches!(dim, Dimension::Aligned(_)));
                assert_eq!(dim.base().common.layer, "7标注层");
                assert_eq!(dim.base().style_name, "OCSM_GB");
            }
            other => panic!("expected preview Dimension, got {other:?}"),
        }
    }
    // ── 标准件放置：两段式（定位基点 → 绕基点旋转 → 落定）──────────────────

    /// 记录型 sender（只记请求，不做真实图纸操作）。
    struct RecordingSender {
        reqs: std::sync::Mutex<Vec<String>>,
    }
    impl RecordingSender {
        fn new() -> Self {
            RecordingSender { reqs: std::sync::Mutex::new(Vec::new()) }
        }
        fn names(&self) -> Vec<String> {
            self.reqs.lock().unwrap().clone()
        }
    }
    impl PluginRequestSender for RecordingSender {
        fn request(
            &self,
            req: ocs_plugin_api::ipc::protocol::PluginRequest,
        ) -> Result<ocs_plugin_api::ipc::protocol::PluginResponse, ocs_plugin_api::host::PluginRequestError>
        {
            use ocs_plugin_api::ipc::protocol::{PluginRequest as R, PluginResponse as P};
            let name = match &req {
                R::AddEntities(v) => {
                    // 记下 INSERT 的旋转角（弧度）便于断言
                    let rot = v.iter().find_map(|e| match e {
                        acadrust::EntityType::Insert(i) => Some(i.rotation),
                        _ => None,
                    });
                    self.reqs
                        .lock()
                        .unwrap()
                        .push(format!("AddEntities rot={:.6}", rot.unwrap_or(f64::NAN)));
                    return Ok(P::Handles(vec![acadrust::Handle::NULL]));
                }
                R::AddBlockRecord { name, .. } => format!("AddBlockRecord {name}"),
                R::WriteRecord { .. } => "WriteRecord".to_string(),
                R::BumpGeometry => "BumpGeometry".to_string(),
                R::SetDirty => "SetDirty".to_string(),
                other => format!("{other:?}"),
            };
            self.reqs.lock().unwrap().push(name);
            Ok(P::Ok)
        }
    }

    #[test]
    fn rotate_angle_follows_cursor_direction() {
        let b = [0.0, 0.0, 0.0];
        assert!(rotate_angle(b, [10.0, 0.0, 0.0]).abs() < 1e-12, "向右 = 0°");
        assert!((rotate_angle(b, [0.0, 10.0, 0.0]) - std::f64::consts::FRAC_PI_2).abs() < 1e-12, "向上 = +90°");
        assert!((rotate_angle(b, [-10.0, 0.0, 0.0]).abs() - std::f64::consts::PI).abs() < 1e-12, "向左 = 180°");
        assert!((rotate_angle(b, [0.0, -10.0, 0.0]) + std::f64::consts::FRAC_PI_2).abs() < 1e-12, "向下 = −90°");
        assert!((rotate_angle(b, [5.0, 5.0, 0.0]) - std::f64::consts::FRAC_PI_4).abs() < 1e-12, "45°");
        assert_eq!(rotate_angle(b, [0.2, 0.2, 0.0]), 0.0, "贴近基点不抖动");
        // 基点不在原点也按"基点→光标"方向算
        assert!((rotate_angle([3.0, 4.0, 0.0], [8.0, 9.0, 0.0]) - std::f64::consts::FRAC_PI_4).abs() < 1e-12, "平移基点");
    }

    #[test]
    fn part_place_is_two_stage_with_rotation_preview() {
        let rec = std::sync::Arc::new(RecordingSender::new());
        let sender: std::sync::Arc<dyn PluginRequestSender> = rec.clone();
        set_pending_part(PendingPart {
            block: "OCSM_TEST".into(),
            meta_json: "{}".into(),
            label: "测试件 M10x40".into(),
        });
        let mut cmd = PartPlace {
            sender: sender.clone(),
            phase: std::cell::Cell::new(PlacePhase::Follow),
            base: std::cell::Cell::new([0.0, 0.0, 0.0]),
        };
        // 未定位：预览跟光标（旋转 0）
        let prev = cmd.on_mouse_move([7.0, 8.0, 0.0]).expect("预览");
        match prev {
            acadrust::EntityType::Insert(i) => {
                assert_eq!((i.insert_point.x, i.insert_point.y), (7.0, 8.0));
                assert_eq!(i.rotation, 0.0);
                assert_eq!(i.block_name, "OCSM_TEST");
            }
            other => panic!("预览应为 INSERT：{other:?}"),
        }
        assert!(cmd.prompt().contains("点击定位基点"), "提示：{}", cmd.prompt());
        // 第一下：定位基点 → 转入旋转阶段
        assert_eq!(std::mem::discriminant(&cmd.on_point([2.0, 3.0, 0.0])),
                   std::mem::discriminant(&CommandStep::NeedPoint));
        assert!(cmd.prompt().contains("绕基点旋转"), "提示：{}", cmd.prompt());
        // 移动光标：预览停在基点、绕基点旋转（光标在 +y → 90°）
        let prev = cmd.on_mouse_move([2.0, 13.0, 0.0]).expect("旋转预览");
        match prev {
            acadrust::EntityType::Insert(i) => {
                assert_eq!((i.insert_point.x, i.insert_point.y), (2.0, 3.0), "预览吸附到基点");
                assert!((i.rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-12, "旋转 90°，实际 {}", i.rotation);
            }
            other => panic!("预览应为 INSERT：{other:?}"),
        }
        // 第二下：落定（插入交给 worker；这里直接同步调用 place_one 验证请求内容）
        let pending = PendingPart {
            block: "OCSM_TEST".into(),
            meta_json: "{}".into(),
            label: "测试件".into(),
        };
        place_one(&sender, &pending, PlaceTask { pt: [2.0, 3.0, 0.0], rotation: std::f64::consts::FRAC_PI_2 })
            .expect("落件");
        let names = rec.names();
        assert!(names.iter().any(|n| n.contains("AddEntities") && n.contains("rot=1.570796")), "{names:?}");
        assert!(names.contains(&"WriteRecord".to_string()), "{names:?}");
        assert!(names.contains(&"SetDirty".to_string()), "{names:?}");
        // 落定后回到跟随阶段，可连续放置
        set_pending_part(PendingPart {
            block: "OCSM_TEST".into(),
            meta_json: "{}".into(),
            label: "测试件".into(),
        });
        let mut cmd2 = PartPlace {
            sender,
            phase: std::cell::Cell::new(PlacePhase::Follow),
            base: std::cell::Cell::new([0.0, 0.0, 0.0]),
        };
        let _ = cmd2.on_point([1.0, 1.0, 0.0]);
        assert!(cmd2.prompt().contains("绕基点旋转"));
        let _ = cmd2.on_point([1.0, 2.0, 0.0]);
        assert!(cmd2.prompt().contains("点击定位基点"), "落定后回到跟随：{}", cmd2.prompt());
    }

    #[test]
    fn parse_session_env_picks_display_keys() {
        let sample = "LANG=zh_CN.UTF-8\nDISPLAY=:0\nWAYLAND_DISPLAY=wayland-0\nXAUTHORITY=/run/user/1000/xauth_uAXGEA\nXDG_SESSION_TYPE=wayland\nPATH=/usr/bin\nEMPTY=\n";
        let got = parse_session_env(sample);
        let get = |k: &str| got.iter().find(|(kk, _)| kk == k).map(|(_, v)| v.as_str());
        assert_eq!(get("DISPLAY"), Some(":0"));
        assert_eq!(get("WAYLAND_DISPLAY"), Some("wayland-0"));
        assert_eq!(get("XAUTHORITY"), Some("/run/user/1000/xauth_uAXGEA"));
        assert_eq!(get("PATH"), None, "无关变量不入表");
        assert_eq!(get("EMPTY"), None, "空值不入表");
        // 去重：同名只保留第一个
        assert_eq!(parse_session_env("DISPLAY=:1\nDISPLAY=:2\n").len(), 1);
    }

}
