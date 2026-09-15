//! `ocs-webpanel-host` — the web-view half of the Web Panel plugin.
//!
//! A GTK3 + WebKitGTK window that loads a URL and **docks itself into the host
//! window**: it finds the OpenCADStudio X11 window (the process that started the
//! plugin runner), reparents itself into it, and follows the rectangle the
//! plugin forwards on stdin:
//!
//! ```text
//! dock <x> <y> <w> <h>     # window-relative rectangle to occupy
//! undock                   # hide (panel closed in the host)
//! ```
//!
//! The host reserves that rectangle in its own layout, so the web view never
//! covers the ribbon, toolbars or other panels — it fills exactly the space the
//! dock column gave up (the same space the built-in Properties panel occupies).
//!
//! X11 specifics: the window is withdrawn, flagged `override-redirect` (so the
//! window manager never manages it), then reparented. On Wayland there is no
//! cross-process embedding — the window simply shows as a normal window.

#![cfg_attr(not(target_os = "linux"), allow(dead_code, unused_imports))]

#[cfg(target_os = "linux")]
fn main() {
    linux::run();
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("ocs-webpanel-host: 目前只有 Linux(X11) 版本；其它平台把这里换成 WebView2 / WKWebView。");
    std::process::exit(2);
}

#[cfg(target_os = "linux")]
mod linux {
    use gtk::prelude::*;
    use webkit2gtk::WebViewExt as _;
    use std::io::{BufRead, BufReader};
    use std::sync::mpsc::{channel, Receiver};
    use std::sync::Arc;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConnectionExt as _, *};
    use x11rb::rust_connection::RustConnection;

    /// A command from the plugin (stdin).
    #[derive(Debug, Clone, Copy)]
    enum Cmd {
        Dock {
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            /// Host window size in the host's logical space, used to recover
            /// the logical → device factor (see `scale_from_parent`).
            logical_w: i32,
            logical_h: i32,
        },
        Undock,
        Quit,
    }

    fn parse_cmd(line: &str) -> Option<Cmd> {
        let mut it = line.split_whitespace();
        match it.next()?.to_ascii_lowercase().as_str() {
            "dock" => {
                let v: Vec<i32> = it.filter_map(|t| t.parse().ok()).collect();
                (v.len() >= 4).then(|| Cmd::Dock {
                    x: v[0],
                    y: v[1],
                    w: v[2].max(1),
                    h: v[3].max(1),
                    logical_w: v.get(4).copied().unwrap_or(0),
                    logical_h: v.get(5).copied().unwrap_or(0),
                })
            }
            "undock" => Some(Cmd::Undock),
            "quit" => Some(Cmd::Quit),
            _ => None,
        }
    }

    /// Read our own command channel (`dock x y w h` / `undock`) on a thread.
    fn stdin_reader() -> Receiver<Cmd> {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            eprintln!("stdin 读取线程启动");
            for line in BufReader::new(stdin).lines().map_while(Result::ok) {
                let parsed = parse_cmd(&line);
                eprintln!("stdin 读到 {line:?} → {parsed:?}");
                if let Some(cmd) = parsed {
                    let quit = matches!(cmd, Cmd::Quit);
                    if tx.send(cmd).is_err() || quit {
                        break;
                    }
                }
            }
            eprintln!("stdin 读取线程结束（EOF）");
            let _ = tx.send(Cmd::Quit);
        });
        rx
    }

    // ── X11 helpers ──────────────────────────────────────────────────────────

    fn net_wm_pid(conn: &RustConnection, win: Window) -> Option<u32> {
        let atom = conn.intern_atom(false, b"_NET_WM_PID").ok()?.reply().ok()?.atom;
        let reply = conn.get_property(false, win, atom, AtomEnum::CARDINAL, 0, 1).ok()?.reply().ok()?;
        reply.value32().and_then(|mut v| v.next())
    }

    fn wm_name(conn: &RustConnection, win: Window) -> Option<String> {
        let reply = conn.get_property(false, win, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 256).ok()?.reply().ok()?;
        let s = String::from_utf8_lossy(&reply.value).trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    /// Walk the window tree collecting every window (depth-first).
    fn all_windows(conn: &RustConnection, root: Window, out: &mut Vec<Window>) {
        out.push(root);
        let Ok(cookie) = conn.query_tree(root) else { return };
        let Ok(reply) = cookie.reply() else { return };
        for child in reply.children {
            all_windows(conn, child, out);
        }
    }

    /// The OpenCADStudio main window: the top-level window owned by the process
    /// that owns *us* (host → runner → panel host), falling back to a title
    /// match so a detached/odd launch still works.
    fn find_host_window(conn: &RustConnection, root: Window) -> Option<Window> {
        let mut wins = Vec::new();
        all_windows(conn, root, &mut wins);

        // Preferred: the window whose `_NET_WM_PID` is our ancestor host.
        let mut wanted = Vec::new();
        let mut pid = std::process::id();
        for _ in 0..4 {
            let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else { break };
            let ppid = status
                .lines()
                .find_map(|l| l.strip_prefix("PPid:"))
                .and_then(|v| v.trim().parse::<u32>().ok());
            match ppid {
                Some(p) if p > 1 => {
                    wanted.push(p);
                    pid = p;
                }
                _ => break,
            }
        }
        for w in wins.iter().copied() {
            if let Some(p) = net_wm_pid(conn, w) {
                if wanted.contains(&p) && wm_name(conn, w).is_some() {
                    return Some(w);
                }
            }
        }
        // Fallback: by window title (the host always starts with this prefix).
        for w in wins.iter().copied() {
            if let Some(name) = wm_name(conn, w) {
                if name.starts_with("Open CAD Studio") {
                    return Some(w);
                }
            }
        }
        None
    }

    /// Our own window (by title, restricted to our PID so two OCS instances in
    /// one X server can't cross wires).
    fn find_own_window(conn: &RustConnection, root: Window) -> Option<Window> {
        let mut wins = Vec::new();
        all_windows(conn, root, &mut wins);
        let me = std::process::id();
        wins.iter()
            .copied()
            .find(|w| {
                wm_name(conn, *w).is_some_and(|n| n == "OCS Web Panel")
                    && net_wm_pid(conn, *w).is_none_or(|p| p == me)
            })
    }

    /// Reparent `child` into `parent` at the given window-relative rectangle.
    fn dock(conn: &RustConnection, child: Window, parent: Window, x: i32, y: i32, w: u32, h: u32) {
        // ① withdraw so any window manager forgets about us…
        let _ = conn.unmap_window(child);
        // …② and flag override-redirect so it never manages us again.
        let attr = ChangeWindowAttributesAux::new().override_redirect(1);
        let _ = conn.change_window_attributes(child, &attr);
        // ③ reparent + place + show + raise.
        let _ = conn.reparent_window(child, parent, x as i16, y as i16);
        let geom = ConfigureWindowAux::new()
            .x(x)
            .y(y)
            .width(w)
            .height(h)
            .stack_mode(StackMode::ABOVE);
        let _ = conn.configure_window(child, &geom);
        let _ = conn.map_window(child);
        let _ = conn.flush();
    }

    /// Recover the host's logical → device pixel factor: the host lays out in
    /// its own logical space (which winit/iced scale however they like), while
    /// our child window lives in device pixels. Comparing the host window's
    /// real X11 size against the logical size it reported gives the factor
    /// without either side having to know iced's internals.
    fn scale_from_parent(
        conn: &RustConnection,
        parent: Window,
        logical_w: i32,
        logical_h: i32,
    ) -> f64 {
        if logical_w <= 0 || logical_h <= 0 {
            return 1.0;
        }
        let Some(geo) = conn.get_geometry(parent).ok().and_then(|c| c.reply().ok()) else {
            return 1.0;
        };
        let fx = geo.width as f64 / logical_w as f64;
        let fy = geo.height as f64 / logical_h as f64;
        // A sane factor only; anything else means we misread the window.
        let f = (fx + fy) / 2.0;
        if (0.4..=3.0).contains(&f) {
            f
        } else {
            1.0
        }
    }

    /// Place the window through **GDK** (not raw X11): GDK keeps its own idea of
    /// a window's geometry and will fight an externally set position/size after
    /// a foreign reparent, so every placement must go through it.
    fn gdk_place(win: &gtk::Window, x: i32, y: i32, w: u32, h: u32) {
        if let Some(gdk_win) = win.window() {
            gdk_win.move_resize(x, y, w.max(1) as i32, h.max(1) as i32);
        }
    }

    /// Move/resize an already-docked window.
    fn place(conn: &RustConnection, child: Window, x: i32, y: i32, w: u32, h: u32) {
        let geom = ConfigureWindowAux::new().x(x).y(y).width(w).height(h);
        let _ = conn.configure_window(child, &geom);
        let _ = conn.map_window(child);
        let _ = conn.flush();
    }

    /// Give the panel keyboard focus (X11 children of another client never get
    /// it from the window manager). Called when the user clicks the web view —
    /// clicking back into the CAD window hands focus back, exactly as with any
    /// other X11 child window.
    fn claim_focus(conn: &RustConnection, child: Window) {
        let _ = conn.set_input_focus(InputFocus::POINTER_ROOT, child, x11rb::CURRENT_TIME);
        let _ = conn.flush();
    }

    pub fn run() {
        let url = std::env::args()
            .nth(1)
            .unwrap_or_else(|| "about:blank".to_string());

        if gtk::init().is_err() {
            eprintln!("ocs-webpanel-host: GTK 初始化失败");
            std::process::exit(1);
        }

        let win = gtk::Window::new(gtk::WindowType::Toplevel);
        win.set_title("OCS Web Panel");
        win.set_decorated(false);
        win.set_resizable(true);
        win.set_skip_taskbar_hint(true);
        win.set_skip_pager_hint(true);
        win.set_default_size(620, 900);
        // The host owns our geometry once we are docked: GTK must not keep
        // re-applying its own preferred size to a reparented window.
        win.set_resizable(false);

        let view = webkit2gtk::WebView::new();
        if let Some(settings) = webkit2gtk::WebViewExt::settings(&view) {
            settings.set_property("enable-developer-extras", true);
            settings.set_property("enable-page-cache", true);
            // Embedded panels are narrow: let pages shrink their own layout.
            settings.set_property("enable-html5-local-storage", true);
        }
        win.add(&view);

        // Realize first: we need the X11 window id before anything is mapped,
        // so the window never appears as a floating frame on screen.
        win.realize();
        if let Some(gdk_win) = win.window() {
            gdk_win.set_override_redirect(true);
        }
        win.show_all();
        view.load_uri(&url);

        // X11 is optional: without it the panel still runs, just not embedded.
        // `Arc` so both the focus hook and the dock loop can share one
        // connection (x11rb connections are thread-safe and clone-free).
        let x11 = RustConnection::connect(None)
            .ok()
            .map(|(conn, num)| (Arc::new(conn), num));
        let (own, host) = match x11.as_ref() {
            Some((conn, screen_num)) => {
                let root = conn.setup().roots[*screen_num].root;
                (find_own_window(conn, root), find_host_window(conn, root))
            }
            None => (None, None),
        };
        match (x11.as_ref(), own, host) {
            (Some((conn, _)), Some(child), Some(parent)) => {
                // Clicks on the web view take keyboard focus for the panel: an
                // X11 child of another client never gets it from the WM, and
                // clicking back into the CAD window hands it back.
                let conn_for_focus = Arc::clone(conn);
                view.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
                view.connect_button_press_event(move |_, _| {
                    claim_focus(&conn_for_focus, child);
                    gtk::glib::Propagation::Proceed
                });
                eprintln!("ocs-webpanel-host: 面板窗口 {child} → 宿主窗口 {parent}（等待停靠指令）");
            }
            _ => eprintln!(
                "ocs-webpanel-host: 未找到可嵌入的宿主窗口（X11 不可用或宿主不在 X11 上）—— 面板作为普通窗口显示"
            ),
        }

        // Drain the plugin's dock commands on the GTK main loop.
        let rx = stdin_reader();
        let x11_main = x11.clone();
        let win_for_dock = win.clone();
        let mut docked_once = false;
        let mut last_rect: Option<(i32, i32, u32, u32)> = None;
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            while let Ok(cmd) = rx.try_recv() {
                match cmd {
                    Cmd::Dock {
                        x,
                        y,
                        w,
                        h,
                        logical_w,
                        logical_h,
                    } => {
                        let f = match (x11_main.as_ref(), host) {
                            (Some((conn, _)), Some(parent)) => {
                                scale_from_parent(conn, parent, logical_w, logical_h)
                            }
                            _ => 1.0,
                        };
                        eprintln!(
                            "dock 收到：{w}x{h} @ ({x},{y}) 逻辑 {logical_w}x{logical_h} → 缩放 {f:.3} → {:.0}x{:.0} @ ({:.0},{:.0}) | x11={} 自身={:?} 宿主={:?}",
                            w as f64 * f,
                            h as f64 * f,
                            x as f64 * f,
                            y as f64 * f,
                            x11_main.is_some(),
                            own,
                            host
                        );
                        let (x, y, w, h) = (
                            (x as f64 * f).round() as i32,
                            (y as f64 * f).round() as i32,
                            (w as f64 * f).round().max(1.0) as i32,
                            (h as f64 * f).round().max(1.0) as i32,
                        );
                        last_rect = Some((x, y, w.max(1) as u32, h.max(1) as u32));
                        if let Some((conn, _)) = x11_main.as_ref() {
                            match (own, host) {
                                (Some(child), Some(parent)) => {
                                    if !docked_once {
                                        dock(conn, child, parent, x, y, w as u32, h as u32);
                                        docked_once = true;
                                    } else {
                                        place(conn, child, x, y, w as u32, h as u32);
                                    }
                                }
                                _ => {
                                    if let Some(child) = own {
                                        place(conn, child, x, y, w as u32, h as u32);
                                    }
                                }
                            }
                        }
                        // Then let GDK record the geometry as its own, so it
                        // stops re-applying the pre-dock position/size.
                        gdk_place(&win_for_dock, x, y, w as u32, h as u32);
                    }
                    Cmd::Undock => {
                        last_rect = None;
                        if let (Some((conn, _)), Some(child)) = (x11_main.as_ref(), own) {
                            let _ = conn.unmap_window(child);
                            let _ = conn.flush();
                        }
                    }
                    Cmd::Quit => {
                        gtk::main_quit();
                        return gtk::glib::ControlFlow::Break;
                    }
                }
            }
            gtk::glib::ControlFlow::Continue
        });

        // GTK/GDK keeps re-asserting its own idea of a reparented window's
        // geometry; re-applying ours on a timer wins the race and keeps the
        // panel exactly in the slot the host reserved.
        let win_keep = win.clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            if let Some((x, y, w, h)) = last_rect {
                gdk_place(&win_keep, x, y, w, h);
            }
            gtk::glib::ControlFlow::Continue
        });

        gtk::main();
    }
}

#[cfg(all(test, target_os = "linux"))]
mod parse_tests {
    // `parse_cmd` lives inside the private `linux` module; re-expose a tiny
    // copy of the grammar check so a regression in the stdin protocol is
    // caught without a display.
    fn parse(line: &str) -> Option<(Vec<i32>, bool)> {
        let mut it = line.split_whitespace();
        let head = it.next()?.to_ascii_lowercase();
        match head.as_str() {
            "dock" => {
                let v: Vec<i32> = it.filter_map(|t| t.parse().ok()).collect();
                (v.len() >= 4).then_some((v, false))
            }
            "undock" => Some((vec![], false)),
            "quit" => Some((vec![], true)),
            _ => None,
        }
    }

    #[test]
    fn dock_lines_carry_the_slot_and_shutdown_works() {
        assert_eq!(
            parse("dock 640 184 385 554 1024 768"),
            Some((vec![640, 184, 385, 554, 1024, 768], false))
        );
        assert_eq!(parse("undock"), Some((vec![], false)));
        assert_eq!(parse("quit"), Some((vec![], true)));
        assert_eq!(parse("dock 1 2 3"), None, "缺参数应忽略");
    }
}
