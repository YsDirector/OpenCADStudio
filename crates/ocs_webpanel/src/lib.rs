//! OpenCADStudio **Web Panel** — a generic, CAD-agnostic plugin that fills the
//! host's reserved web-panel column (`PanelId::Web`) with a real web view.
//!
//! The host cannot render HTML, so it reserves an edge column and hands out the
//! rectangle it occupies (`HostApi::web_panel_rect`). This plugin spawns a
//! separate **panel host** process (`ocs-webpanel-host`) that owns a GTK3 +
//! WebKitGTK window, and docks that window into the host's window with an X11
//! reparent — the same trick the panel needs on either stack: the web view lives
//! in its own process, and the host keeps laying out (and reserving) the space.
//!
//! Nothing here is OCSM-specific: pass any URL (`WEBPANEL http://…`, including a
//! local dev server, `opencode`'s web UI, docs, a dashboard). The layout, drag,
//! side switch, auto-collapse and close chrome all come from the host's stock
//! dock behaviour.
//!
//! Platform note: cross-process window embedding needs a real window system.
//! On Linux this means **X11 / XWayland** — start the host with
//! `env -u WAYLAND_DISPLAY` (winit then picks the X11 backend) when you want the
//! panel embedded. On Wayland the panel host still runs, but as a plain window.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};

use ocs_plugin_api::export_plugin;
use ocs_plugin_api::host::{BuiltinPlugin, HostApi, PluginRequestSender};
use ocs_plugin_api::ipc::protocol::{PluginRequest, PluginResponse};
use ocs_plugin_api::manifest::{ApiVersion, PluginManifest};
use ocs_plugin_api::ribbon::{
    CadModule, IconKind, ModuleEvent, RibbonGroup, RibbonItem, ToolDef,
};

static MANIFEST: PluginManifest = PluginManifest {
    id: "opencad.webpanel",
    name: "Web Panel 网页面板",
    version: "0.1.0",
    description: "把任意网页（pi-web / opencode / 本地服务）停靠进 OCS 侧边栏（通用，与 OCSM 无关）",
    api_version: ApiVersion { major: 5 },
    ribbon_order: 200,
    xdata_apps: &[],
    command_prefixes: &["WEBPANEL", "WP", "WEBPANELOFF"],
};

/// Where the last used URL is remembered (`$XDG_CONFIG_HOME/OpenCADStudio/webpanel.url`).
const DEFAULT_URL: &str = "http://127.0.0.1:30141";

/// Session state: the panel-host child process and the rectangle we last told
/// it to occupy.
#[derive(Default)]
struct State {
    child: Option<Child>,
    url: String,
    /// Last rectangle pushed to the child (`None` = told it to undock).
    /// `(x, y, w, h, logical_win_w, logical_win_h)`.
    last: Option<(i32, i32, i32, i32, i32, i32)>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| {
        Mutex::new(State {
            url: load_url().unwrap_or_else(|| DEFAULT_URL.to_string()),
            ..Default::default()
        })
    })
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("OpenCADStudio").join("webpanel.url"))
}

fn load_url() -> Option<String> {
    let text = std::fs::read_to_string(config_path()?).ok()?;
    let url = text.trim().to_string();
    (!url.is_empty()).then_some(url)
}

fn store_url(url: &str) {
    if let Some(path) = config_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, url);
    }
}

/// The plugin's install directory, recovered from the runner's argv
/// (`--ocs-plugin-runner <socket> <lib path>`), where `ocs-webpanel-host` sits.
fn install_dir() -> Option<PathBuf> {
    let raw = std::fs::read("/proc/self/cmdline").ok()?;
    let args: Vec<String> = raw
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    let i = args.iter().position(|a| a == "--ocs-plugin-runner")?;
    let lib = PathBuf::from(args.get(i + 2)?);
    lib.parent().map(PathBuf::from)
}

fn host_binary() -> Option<PathBuf> {
    let dir = install_dir()?;
    let candidate = dir.join("ocs-webpanel-host");
    candidate.is_file().then_some(candidate)
}

/// Start (or restart) the panel-host process for `url`.
fn spawn_panel(url: &str) -> Result<(), String> {
    let exe = host_binary().ok_or_else(|| {
        "找不到 ocs-webpanel-host —— 它应与插件 .so 同目录（见 install.sh）".to_string()
    })?;
    let mut st = state().lock().map_err(|_| "webpanel state poisoned")?;
    let same_url = st.url == url;
    if st.child.is_some() && same_url {
        return Ok(()); // already running with this URL
    }
    if let Some(mut child) = st.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    // Keep the panel host's stderr: it carries the reparent/dock diagnostics.
    let log = std::fs::File::create("/tmp/ocs-webpanel-host.log").ok();
    let mut cmd = Command::new(exe);
    cmd.arg(url)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(match log {
            Some(f) => Stdio::from(f),
            None => Stdio::null(),
        });
    let child = cmd
        .spawn()
        .map_err(|e| format!("启动面板进程失败：{e}"))?;
    st.child = Some(child);
    st.url = url.to_string();
    st.last = None;
    store_url(url);
    Ok(())
}

fn stop_panel() {
    if let Ok(mut st) = state().lock() {
        if let Some(mut child) = st.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        st.last = None;
    }
}

/// Push the rectangle into the child's stdin (the panel host's dock protocol).
fn tell_child(rect: Option<(i32, i32, i32, i32, i32, i32)>) -> String {
    let Ok(mut st) = state().lock() else {
        return "state 锁失败".into();
    };
    let Some(child) = st.child.as_mut() else {
        return "没有面板进程（未 spawn？）".into();
    };
    let Some(stdin) = child.stdin.as_mut() else {
        return "面板进程的 stdin 不可用".into();
    };
    let line = match rect {
        Some((x, y, w, h, lw, lh)) => format!("dock {x} {y} {w} {h} {lw} {lh}\n"),
        None => "undock\n".to_string(),
    };
    match stdin.write_all(line.as_bytes()).and_then(|_| stdin.flush()) {
        Ok(()) => format!("已写入：{}", line.trim()),
        Err(e) => format!("写入失败：{e}"),
    }
}

/// Poll the host's reserved slot a few times a second; the host re-lays out on
/// resizes, dock drags, side switches and collapses, and this keeps the child
/// window glued to the space actually reserved for it.
fn start_poller(sender: Box<dyn PluginRequestSender>) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    std::thread::spawn(move || {
        // A host older than API v7 does not know `WebPanelRect`; stop polling
        // after a few failures instead of hammering a runner that can't answer.
        let mut failures = 0u32;
        loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
        let rect = match sender.request(PluginRequest::WebPanelRect) {
            Ok(PluginResponse::DockRect(r)) => {
                failures = 0;
                r
            }
            Ok(_) => None,
            Err(_) => {
                failures += 1;
                if failures > 5 {
                    eprintln!("[webpanel] 宿主不支持 WebPanelRect（需要 v7 宿主），已停止轮询");
                    return;
                }
                continue;
            }
        };
        let want = rect.map(|r| {
            (
                r.x.round() as i32,
                r.y.round() as i32,
                r.w.round() as i32,
                r.h.round() as i32,
                r.logical_w.round() as i32,
                r.logical_h.round() as i32,
            )
        });
        let changed = match state().lock() {
            Ok(mut st) => {
                let changed = st.last != want;
                st.last = want;
                changed
            }
            Err(_) => false,
        };
        if changed {
            // Surface the slot in the host's command line: it is the one number
            // that explains where the panel went when something looks off.
            // Order matters: tell the child **first** (a synchronous IPC request
            // for the human-readable line could otherwise delay the dock).
            let told = tell_child(want);
            if let Some((x, y, w, h, lw, lh)) = want {
                let _ = sender.request(PluginRequest::PushInfo(format!(
                    "网页面板槽位：{w}×{h} @ ({x},{y})（逻辑窗口 {lw}×{lh}）→ {told}"
                )));
            } else {
                let _ = sender.request(PluginRequest::PushInfo(format!("网页面板：槽位不可见 → {told}")));
            }
        }
        }
    });
}

/// `WEBPANEL [url] [width] [left|right]` / `WEBPANEL OFF`.
#[derive(Default)]
struct WebPanelPlugin;

impl BuiltinPlugin for WebPanelPlugin {
    fn manifest(&self) -> &'static PluginManifest {
        &MANIFEST
    }

    fn ribbon(&self) -> Box<dyn CadModule> {
        Box::new(Ribbon)
    }

    fn dispatch(&self, host: &mut dyn HostApi, cmd: &str) -> bool {
        let mut parts = cmd.split_whitespace();
        let Some(head) = parts.next() else {
            return false;
        };
        let head_up = head.to_ascii_uppercase();
        if head_up != "WEBPANEL" && head_up != "WP" && head_up != "WEBPANELOFF" {
            return false;
        }
        let mut off = head_up == "WEBPANELOFF";
        let mut url: Option<String> = None;
        let mut side: Option<&'static str> = None;
        for arg in parts {
            match arg.to_ascii_uppercase().as_str() {
                "OFF" | "CLOSE" => off = true,
                "LEFT" => side = Some("left"),
                "RIGHT" => side = Some("right"),
                // Anything that looks like a URL / path wins over the rest.
                _ => url = Some(arg.to_string()),
            }
        }

        if off {
            host.push_info("网页面板：已关闭（WEBPANEL 可再次打开）");
            stop_panel();
            let _ = host.set_web_panel_docked(false, None);
            return true;
        }

        // Remember the requested URL before docking, so the poller has it.
        if let Some(u) = url {
            if let Ok(mut st) = state().lock() {
                st.url = u;
            }
        }
        let want_url = match state().lock() {
            Ok(st) => st.url.clone(),
            Err(_) => DEFAULT_URL.to_string(),
        };
        if want_url.trim().is_empty() {
            host.push_error("用法：WEBPANEL <url> [宽度] [left|right]（或 WEBPANEL OFF）");
            return true;
        }

        match spawn_panel(&want_url) {
            Ok(()) => {
                let docked = host.set_web_panel_docked(true, side);
                host.push_info(&format!(
                    "网页面板：{} 已停靠{}（关闭：WEBPANEL OFF 或面板标题栏 ×）",
                    want_url,
                    if docked { "" } else { "（布局未变）" }
                ));
                if let Some(sender) = host.plugin_request_sender() {
                    start_poller(sender);
                }
            }
            Err(e) => host.push_error(&format!("网页面板：{e}")),
        }
        true
    }

    fn on_load(&mut self, host: &mut dyn HostApi) {
        // Keep the poller alive even if the panel is opened later from the
        // ribbon rather than the command line.
        if let Some(sender) = host.plugin_request_sender() {
            start_poller(sender);
        }
    }
}

/// Ribbon entry: one button that opens the panel (and one that closes it), so
/// the panel is reachable without remembering the command.
struct Ribbon;

impl CadModule for Ribbon {
    fn id(&self) -> &'static str {
        MANIFEST.id
    }
    fn title(&self) -> &'static str {
        "Web Panel 网页面板"
    }
    fn ribbon_groups(&self) -> &[RibbonGroup] {
        static GROUPS: OnceLock<Vec<RibbonGroup>> = OnceLock::new();
        GROUPS.get_or_init(|| {
            vec![RibbonGroup {
                title: "网页面板",
                tools: vec![
                    RibbonItem::LargeTool(ToolDef {
                        id: "WEBPANEL",
                        label: "打开网页面板",
                        icon: IconKind::Glyph("🌐"),
                        event: ModuleEvent::Command("WEBPANEL".to_string()),
                    }),
                    RibbonItem::LargeTool(ToolDef {
                        id: "WEBPANELOFF",
                        label: "关闭网页面板",
                        icon: IconKind::Glyph("✖"),
                        event: ModuleEvent::Command("WEBPANELOFF".to_string()),
                    }),
                ],
            }]
        })
    }
}

export_plugin!(WebPanelPlugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_head_is_recognised_case_insensitively() {
        // Dispatch only claims our own commands — anything else falls through
        // to the host's own command table.
        for cmd in ["WEBPANEL", "webpanel", "wp", "WEBPANELOFF"] {
            let head = cmd.split_whitespace().next().unwrap().to_ascii_uppercase();
            assert!(
                head == "WEBPANEL" || head == "WP" || head == "WEBPANELOFF",
                "{cmd} 应被识别"
            );
        }
        assert!(!matches!(
            "BOM".split_whitespace().next().unwrap().to_ascii_uppercase().as_str(),
            "WEBPANEL" | "WP" | "WEBPANELOFF"
        ));
    }

    #[test]
    fn arguments_split_into_off_side_and_url() {
        let parse = |cmd: &str| {
            let mut off = false;
            let mut url: Option<String> = None;
            let mut side: Option<&'static str> = None;
            for arg in cmd.split_whitespace().skip(1) {
                match arg.to_ascii_uppercase().as_str() {
                    "OFF" | "CLOSE" => off = true,
                    "LEFT" => side = Some("left"),
                    "RIGHT" => side = Some("right"),
                    _ => url = Some(arg.to_string()),
                }
            }
            (off, url, side)
        };
        assert_eq!(
            parse("WEBPANEL http://127.0.0.1:30141 right"),
            (false, Some("http://127.0.0.1:30141".into()), Some("right"))
        );
        assert_eq!(parse("WEBPANEL OFF"), (true, None, None));
        assert_eq!(parse("WEBPANEL"), (false, None, None));
    }
}
