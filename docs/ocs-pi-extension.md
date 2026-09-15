# OCS Pi Extension（原「网页面板 / AI 助手」线）— 交接与待办

> 2026-09-16 交接。上一会话上下文已满，本文件是唯一入口：新会话从「现状 → 待办」读起即可。

## 1. 定位与命名

- 正式名：**OCS Pi Extension**（宿主内建面板，标题「Pi 助手」，命令 `PI`，别名 `OCSPI` / `AI` / `AICHAT`）。
- **它是宿主内建能力，不是插件**：宿主插件 API 没有「注册 UI 面板」的能力，而 iced 控件无法跨进程传递，
  所以面板内容必须由宿主自己用 iced 画（`src/ui/pi_panel.rs`）。
- 曾经的方案（**已废弃并删除**）：通用「网页面板」插件 `crates/ocs_webpanel` + 子进程 GTK/WebKitGTK +
  `XReparentWindow` 嵌进宿主窗口。放弃原因：宿主绘制的面板标题栏（× / 图钉）与 5px 拖动分割线被盖住、
  对齐脆弱、双进程争焦点、依赖 XWayland、缩放/DPI 要自己换算、视觉不像原生。
  提交 `a4f122ec` 已把这条线（插件 crate、API v7 的 `DockRect`/`SetWebPanelDocked`/`WebPanelRect`、
  宿主 `web_panel_rect`、两列 bounds 上报、已安装插件目录）全部移除。
- 因此**通用性不再成立**：本面板专用于 pi（`pi-web` 本地 HTTP API），故按用户要求改名 Pi Extension。

## 2. 现状（已完成，可运行）

| 提交 | 内容 |
|---|---|
| `a4f122ec` | 停靠位改原生面板；删除 webview 方案；`PanelId::Pi`（当时叫 `Ai`）；面板骨架；数据层 `src/pi.rs`；命令 `PI` |
| `7ee77938` | 宽度策略（下限 150 / 上限占窗 40% / 默认 340 / 默认钉住）+ 拖动幽灵失焦清除 |
| `675e552b` | **真因修复**：面板 `view(width, …)` 必须 `.width(Length::Fixed(width))` |
| 本提交 | 改名 Pi（`PanelId::Pi` / `PiPanelState` / `src/pi.rs` / `src/ui/pi_panel.rs`）+ 本文件 |

现在能做的：命令 `PI` 在右侧栏停靠出原生面板，可拖宽（150 起）、换边、钉住收起、关闭；
面板显示连接状态占位（"未连接 / 正在连接 / 已就绪"），**还没有消息列表与输入框**。

## 3. 待办：Phase 2（把内容做出来）

数据层已就绪（`src/pi.rs`，2 个单测），缺的是接到 UI 上：

1. **轮询客户端事件**：宿主每帧有 `Message::Tick(Instant)`（`window::frames()` 订阅，见
   `src/app/view/mod.rs`）。在 Tick 里 `while let Ok(ev) = state.worker.rx.try_recv()` 抽干事件，
   映射成 `PiPanelState` 的状态/条目；只在必要时 `Task::none()`（不要每帧重绘整列）。
   建议新增 `Message::PiPoll`（订阅里节流到 ~10Hz）而不是直接用 Tick，避免 60fps 空转。
2. **消息列表**（iced 原生）：
   - 用户消息 / 助手文本（流式中**原地追加**到同一条 `PiEntry::Assistant`，靠 `message_start|end` 配对）
   - 思考块：默认折叠（一行摘要 + 点击展开）
   - 工具调用：`PiEntry::Tool { name, output, done }`，标题行「工具名 + 运行中/完成」，
     输出默认可折叠（长输出截断显示，展开看全量）
   - 用 `scrollable`，新事件到达时滚到底（`scrollable::scroll_to` 需要 `scrollable::Id`）
3. **底部输入框**：多行（`text_editor`），Enter 发送 / Shift+Enter 换行；
   发送 → `Command::Send { session, prompt }`，本地先插入 `PiEntry::User` 并清空输入；
   流式中禁用发送或改为排队（二选一并注释理由）。
4. **异常态**：pi-web 未运行 → 显示「未连接 + 启动方式（`pi-web` 或 `node ~/.local/bin/pi-web`）」，
   重连按钮（`Command::Reconnect`）；不阻塞宿主、不刷屏报错。
5. **会话**：`/api/sessions` 已有列表与首条消息；Phase 2 先"跟随最近活跃会话"，
   顶部可选加一个会话切换下拉（`/api/agent/<id>/events` 换 id + `Event::Replace` 清空列表）。
   回填历史：SSE 只推新事件，历史可读 `/api/sessions` 里的 `path`（session `.jsonl`）尾部。
6. 可选：`extension_ui_request`（`setWidget`，如 `bash-bg` 后台任务小部件）→ 面板底部显示一行状态。

## 4. 关键契约与坑（血泪版）

- **面板宽度**：`build_edge_stack` 把列宽传进 `view(width, …)`；实现**必须** `.width(Length::Fixed(width))`，
  写 `Fill` 会与画布平分同一行空间（症状：宽度与 `dock.set_width` 存的值无关、关掉另一侧面板后恰好占 50%）。
  内置面板 `src/ui/properties.rs`、`src/ui/window/block_palette.rs` 是正确范例。
- **dock 配置**：`~/.config/OpenCADStudio/settings.json` 的**顶层 `dock` 键**：
  `{"left":[…],"right":[…],"panels":{"<id>":{"width":f32,"auto_collapse":bool}}}`。
  **改配置必须先关 OCS**：运行中的实例退出时会把内存值写回，覆盖你的修改。
  本次改名把 serde 键从 `ai` 改为 `pi` → 旧 `ai` 条目被忽略，将采用新默认（340 + 钉住）；
  想手工设定就写 `"pi": {"width":300.0,"auto_collapse":false}`。
- **钉住语义**：`auto_collapse: true` = 平时收成窄轨（`DOCK_RAIL_W`）、鼠标悬停才展开；图钉按钮切换。
- **拖动幽灵**：蓝色 2px 边框 + 蓝底标题 = `dock_dragging` 投放预览；窗口内点一下或
  `window::Event::Unfocused` 会结束手势（后者是本次补的修复）。
- **验收方式（用户指定）**：改完用**视觉子代理**读截图量像素（它会给出每块面板左右边界与占比），
  再用配置里的宽度/比例复核，不要凭肉眼看截图下结论（我在这上面错过两轮）。
- **构建/重启**：`cargo build --release`（全量约 1.5 min，机器负载高时更久）→ `pkill -x OpenCADStudio` →
  `bash /tmp/launch-ocs.sh 1231`（原生 Wayland）或 `bash /tmp/launch-ocs-x11.sh`（XWayland，现已不需要）。
  自动化接口：`mcporter call ocs.ocs_sessions` → `ocs.ocs_read op:"state"` → `ocs.ocs_execute op:"run" cmd:"PI"`。
  注意：欢迎页不渲染停靠栏，必须 `op:"new"` 新建图纸后再开面板。

## 5. pi-web 本地 API 契约（详见 wiki: `pi-web 本地 API 契约`）

| 用途 | 请求 |
|---|---|
| 会话列表 | `GET /api/sessions` → `{"sessions":[{id,path,cwd,modified,messageCount,firstMessage}]}` |
| 事件流 | `GET /api/agent/<id>/events` → SSE `data: {json}` |
| 发消息 | `POST /api/agent/<id>`，body `{"prompt":"…"}` |

SSE 事件：`connected`（`isStreaming`）、`message_start`/`message_end`（`message.role` =
`user`/`assistant`/`toolResult`，`content[].text`）、`tool_execution_update`（`partialResult`）、
`tool_execution_end`（`result`）、`extension_ui_request`（`setWidget`）。
默认端点 `http://127.0.0.1:30141`（可用环境变量 `OCS_PI_ENDPOINT` 覆盖）。

## 6. 文件地图

```
src/pi.rs                        pi-web 客户端（工作线程 + SSE 解析 + 单测）
src/ui/pi_panel.rs               Pi 面板：状态 + header（pin/close/DockGrab）+ 占位内容
src/ui/dock.rs                   PanelId::Pi（宽度策略：150 / 40% / 340 / 默认钉住）
src/app/document.rs              tab 字段 pi_panel
src/app/update/mod.rs            Message::TogglePiPanel（停靠 + 启动客户端线程）；Tick 轮询要加在这
src/app/update/dialog.rs         可见性（dock_panel_visible）与关闭语义
src/app/view/mod.rs              expanded_panel 的 Pi 分支；订阅（含 Unfocused → DragRelease）
src/app/commands/display.rs      PI / OCSPI / AI / AICHAT 命令
```

## 7. 开放问题（下次先问用户）

1. 工具输出默认展开还是折叠？思考块要不要显示？
2. 发送后是"禁用输入"还是"排队"？
3. 会话跟随最近活跃，还是顶部加会话切换器？
4. 是否需要把 pi 的审批弹窗（`permission` 类交互）也接进面板？（可能超出面板范围，先问）
