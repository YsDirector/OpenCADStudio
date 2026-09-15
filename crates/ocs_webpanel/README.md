# OpenCADStudio Web Panel（通用网页面板插件）

把**任意网页**停靠进 OpenCADStudio 的侧边栏 —— pi-web、opencode 网页端、本地开发服务器、
在线文档、仪表盘都行。面板占据的是宿主**预留的停靠列**，行为与内置的「特性」面板一致：

- 可拖宽（`DOCK_MIN_W`…每面板上限）、可换边（左/右）、可自动收起（pin）、可关闭（标题栏 ×）
- 面板宽度持久化；关闭后命令再开，回到原位置
- **不遮挡**任何界面（宿主在布局里为它让出空间，画布自动变窄）

本插件与 OCSM 无关，命令与 OCSM 的命令空间不重叠。

## 为什么需要"宿主预留 + 子进程 webview"

宿主（iced）**不能渲染 HTML**，WebKitGTK 也没有离屏 API，所以网页必须是**真实原生窗口**。
唯一能做到"真嵌进窗口且不遮挡"的形态是：

```
宿主窗口（X11）
 └─ 停靠列（宿主在 iced 布局里留出的空间，只画一个占位）
     └─ 网页面板子窗口（XReparentWindow 进来的 GTK3 + WebKitGTK 进程）
```

宿主负责：布局留位、把该位置的矩形报到 `HostApi::web_panel_rect()`；
插件负责：拉起面板进程、把矩形喂给它；
面板进程负责：创建 webview、reparent 进宿主窗口、按矩形摆位、抢焦点。

## 安装

```bash
bash crates/ocs_webpanel/install.sh
# 会把 libocs_webpanel.so + ocs-webpanel-host + plugin.toml 装到
# ~/.config/OpenCADStudio/plugins/opencad.webpanel/
```

重启 OCS 生效。

## 用法（OCS 命令行里敲）

| 命令 | 说明 |
|---|---|
| `WEBPANEL <url>` | 打开网页面板（默认右侧），例如 `WEBPANEL http://127.0.0.1:30141` |
| `WEBPANEL <url> left` / `right` | 打开并指定停靠边 |
| `WEBPANEL` | 用上次的 URL 再打开一次（URL 记在 `~/.config/OpenCADStudio/webpanel.url`） |
| `WEBPANEL OFF` | 关闭（面板标题栏的 × 同样有效） |
| `WP` / `WEBPANELOFF` | 简写 |

功能区还有一个「Web Panel 网页面板」选项卡（打开 / 关闭两个按钮）。

## ⚠️ 平台前提：面板要"嵌进去"，宿主必须跑在 X11/XWayland

Wayland 下**任何**跨进程窗口嵌入都不可能（没有 XEmbed，xdg-foreign 也做不出 subsurface）。
所以以 X11 方式启动 OCS：

```bash
env -u WAYLAND_DISPLAY ./target/release/OpenCADStudio
```

（winit 0.30 按 `WAYLAND_DISPLAY` → `DISPLAY` 选择后端，去掉前者即走 X11。）
宿主本身**不需要任何补丁** —— 停靠列、矩形上报都是宿主的公开插件 API（v7）。

## 两个必须知道的坐标陷阱

1. **逻辑像素 ≠ 设备像素**：宿主在 iced 的逻辑坐标系里布局（本机 1024×768），
   而 X11 子窗口用物理像素（1280×960），缩放比 1.25。
   `DockRect` 因此同时给出矩形和**逻辑窗口尺寸**，面板进程用
   `真实父窗口尺寸 ÷ 逻辑窗口尺寸` 反推缩放比后再摆位。
2. **GDK 会抢几何**：外来 reparent 后 GTK 仍以为自己是顶层窗口，会重新应用它的尺寸/位置。
   面板因此把所有摆位都走 `gdk_window_move_resize`，并每 500ms 重申一次。
3. **焦点**：X11 子窗口拿不到 WM 给的键盘焦点 —— 点击网页时面板自己 `XSetInputFocus`；
   点回 CAD 窗口即交还（与任何 X11 子窗口一致）。

## 诊断

- 面板进程 stderr → `/tmp/ocs-webpanel-host.log`（含找到的窗口、收到的 dock 指令、缩放比）
- 每次槽位变化会往 OCS 命令行打一行：`网页面板槽位：385×554 @ (640,184)（逻辑窗口 1024×768）→ 已写入：dock …`

## 演化方向

- 面板内工具条（刷新/后退/devtools）、多标签（多个 URL）
- Windows/macOS：把 `src/bin/host.rs` 的 webview 换成 WebView2 / WKWebView
  （嵌入方式同构：找宿主窗口 → 子窗口 → 跟随矩形）
- 上游化：`PanelId::Web` + `web_panel_rect()` 这套宿主 API 是通用的，
  可以作为一个独立 PR 提交（OCSM 之外的能力）
