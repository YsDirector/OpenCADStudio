# 本地（fork）补丁台账 · Local patch inventory

> 本仓库是 OpenCADStudio 的**二开 fork**：主线功能 + `crates/ocs_ocsm`（OCSMechanical
> 机械工具包插件）+ 为插件打通宿主所需的**本地补丁**。本文件登记所有与上游不同的
> 宿主侧改动，供**每次同步上游后照单核对**，避免补丁被合掉却无人发现。
>
> 上游：`origin` = HakanSeven12/OpenCADStudio（经 ghfast 镜像）
> 最近同步点：`d4007055`（Merge upstream **v2026.37** = `fc1788df`，执行记录见 §0.5）
> 台账基准：`git diff d4007055^2..HEAD`（上游 v2026.37 → 当前本地 HEAD，**47 文件 / +10991 −96**）
> ⚠️ 上游此后仍有更新（`origin/main` = `1450fdee`，**尚未同步**）
>
> **2026-09-17 状态刷新（本次调查顺便核实）**：
> * §0.6 / §0.7 / §0.9 的分支**都已推到 `prf` 远端，但上游 PR 未开** ——
>   `gh pr list --author YsDirector -R HakanSeven12/OpenCADStudio --state all` 只有已合并的
>   **#1234 / #1235**（2026-09-13T18:14:50Z）。要发 PR 直接用各节的 compare 链接
>   （`桌面/OCSM/PR-*-链接.txt` 里已经是拼好的 create-PR URL）。
> * 上游 `origin/main`（`1450fdee`）现领先本地 **127 个提交**；台账基准仍是 v2026.37（`d4007055`）。
> * 别人的上游 PR **#1306**（*docs(plugin): fix inaccuracies and flag real gaps found building a plugin*）
>   还开着，值得看一眼它点名的插件坑（可能是我们也会踩的）。

## 0. 怎么用（每次同步上游后）

```bash
git fetch origin && git merge <upstream-tag>          # 解决冲突
# ① 核对补丁是否都在（与本文件 §2 的表一致，缺文件=补丁丢了）
git diff --stat $(git rev-parse <merge>^2)..HEAD -- src/ crates/ocs_plugin_api Cargo.toml tests/
# ② 两套产物都要重编！（插件 .so ≠ 宿主二进制）
cargo build --release                                  # 宿主（也是插件 runner）
cargo build --release -p ocs_ocsm && cp target/release/libocs_ocsm.so \
    ~/.config/OpenCADStudio/plugins/opencad.ocsm/       # 插件（含 plugin.toml 的 rustc 门禁）
# ③ 回归
cargo test -p ocs_ocsm                                 # 插件 178（2026-09-14 起）
cargo test -p OpenCADStudio --lib dimtmove              # 宿主 DIMTMOVE 引线 4
OCS_SMOKE_PLUGIN=$PWD/target/release/libocs_ocsm.so \
  OCS_PLUGIN_RUNNER_EXE=$PWD/target/release/OpenCADStudio \
  cargo test -p OpenCADStudio --lib installed_plugin_registers_its_ribbon
# ④ 重启 OCS 复测
```

⚠️ **用户启动入口**：`~/桌面/OpenCADStudio.desktop` → `Exec=<repo>/target/release/OpenCADStudio`。
所以**改到宿主 `src/` 的渲染/几何逻辑必须重编宿主**（只换 `.so` 会看到旧渲染，
2026-09-13 曾因此误判"改了没效果"）。

## 0.5 已上游化（2026-09-14 记 — **已在同日的上游同步中执行完毕**）

> ✅ **执行结果**：`git merge origin/main`（上游 `fc1788df`，即 **v2026.37.0**）完成，
> 合并提交 `d4007055`（父：fork `ba6fab35` + 上游 `fc1788df`）。9 个冲突按本表处置：
> `selection.rs` / `dim_leader_render_check.rs` 取上游；`viewport.rs` / `app/mod.rs` /
> `manifest.rs` 融合（fork 钩子与上游新结构两边都留）；`dimension.rs` 取上游 DIMTMOVE +
> **手工补回 fork 的 C-2 文字宽度自适应**（上游没有）；`Cargo.toml`/`Cargo.lock` 保住镜像 URL 与
> workspace 成员。另修一处自动合并产生的重复再导出（`src/scene/mod.rs` 同时出现上游
> `pub(crate) use selection::pe_url_of;` 与 fork 旧 `pub use`，后者会编译报错，已删）。
> 验证：`cargo check --lib` ✅ · `--lib dimtmove` **5 passed** · `--test dim_leader_render_check` **1 passed** ·
> `-p ocs_ocsm` **178 passed**（插件 API 未被上游改动破坏）。安全网分支/tag 仍保留指向 `ba6fab35`。

上游 owner HakanSeven12 于 **2026-09-13T18:14:50Z** 一次性合并了本 fork 的两个 PR：

| PR | 标题 | 合并提交 | 上游实现说明 |
|---|---|---|---|
| **#1234** | `feat(viewport): Ctrl+click opens an entity PE_URL hyperlink` | `84836b0f` | 直接收录本 fork 的提交 `171fc489`（哈希一致） |
| **#1235** | `fix(dimension): extend the DIMTMOVE=1 leader under the text for linear dims` | `64e64c80` | **改造后收录**：合并信息写 *"with kernel-based dimension leaders"*，上游把该修复接进了他们新的 kernel 渲染器 |

**同步动作（重要）**
1. 冲突时**一律取上游版**（`--theirs` 语义），不要保留本地副本——否则同一功能会有两份实现。
2. 删除本地对应补丁条目：本文档 **C-1**（`cbaf6d61f` → `src/entities/dimension.rs`、
   `tests/dim_leader_render_check.rs`）与 **E-1**（`381c328e`/`6ba3da8e` → `src/scene/selection.rs`、
   `src/scene/mod.rs`、`src/app/update/viewport.rs`）。本地这两处改动**尚未进过台账表格**，
   同步后无需再登记。
3. 注意 **DIMTMOVE 的本地版可能与上游 kernel 版语义不同**（上游重构了引线绘制）→ 同步后必须
   重跑 `cargo test -p OpenCADStudio --lib dimtmove` 与 `tests/dim_leader_render_check.rs`，
   并实测 `DIMTMOVE=1` 的显示（fork 的 OCSM 标注也依赖这条路径）。
4. `crates/ocs_ocsm/src/guide_server.rs` 里对 DIMTMOVE 的处理（`cbaf6d61f` 顺带改动）**属于插件侧**，
   不受上游合并影响，保留。
5. 收尾：合并后可删除分支 `feat/ctrl-click-hyperlink`、`fix/dimtmove-leader-under-text`
   （本地 `/tmp/ocs-pr` 工作区 + fork 远端）。

> 结论：fork 的**上游接受路线**已成功（对照 `obs-2026-08-22-pr-fork` 的双保险策略，
> 无需启用"fork 单向吸收"备选方案）。

## 0.6 待上游合并（2026-09-15 开 PR）

> **修复**：MCP `ocs_read op:"commands"` 的清单响应把 `detail_parameters.name` /
> `search_parameter.search` 硬编码成示例值 `"LINE"`（任何 search 都回显 LINE，客户端无法判断
> 实际生效的过滤条件）。现改为回显真正应用的 `search`（未过滤时为 `null`），
> `detail_parameters.name` 固定 `null`（带 `name` 的请求在上方提前返回 `command` 清单）。
>
> | 项 | 值 |
> |---|---|
> | 文件 | `src/app/control/mod.rs`（+27 −2，含 1 个回归测试） |
> | fork 提交 | `c0f42f4e`（与 PR 提交 **逐字节相同的 diff/消息/作者**） |
> | PR 分支 | `fix/mcp-command-listing-filters`（`34175d9a`，基点 = 上游 `d738ebfc`） |
> | 验证 | 上游基点 `cargo test -p OpenCADStudio --lib control::` → **9 passed**；fork 同套 **9 passed** |
> | 报告 | `桌面/OCSM/PR-MCP命令清单回显-{正文.md,链接.txt}` |
>
> **同步动作**：上游合并后**直接取上游版**（两边提交内容一致，git 会自动归并，无需手工补回）；
> 然后删掉本节、删除分支 `fix/mcp-command-listing-filters`（fork 远端 + 本地 worktree `/tmp/ocs-pr-upstream`）。
> 该改动**不属于 fork 独有补丁**，故不进 §2 总表。

## 0.7 待上游合并 · 插件撤销事务 + 高亮事件（2026-09-15 开 PR）

> 两条宿主侧改动（都源于 OCSM 的 MCP/自动化实测缺口），各自一个上游 PR：
>
> | 项 | 内容 | 文件 | PR 分支 | fork 提交 | 报告 |
> |---|---|---|---|---|---|
> | **撤销事务** | `PluginRequest::BeginUndo/CommitUndo` + `HostApi::begin_undo/commit_undo`：宿主每条 message 末尾提交（空快照即丢）pending 快照，插件一次用户动作（HTTP 流程）必须显式开/关事务才能成一个撤销条目 | `crates/ocs_plugin_api/src/host.rs`、`.../ipc/{protocol.rs,server.rs,client.rs,v4/client.rs}`、`src/app/{document.rs,history.rs,plugin_host.rs}`、`docs/plugin-architecture.md` | `feat/plugin-undo-transaction` | `30425cbc` | `桌面/OCSM/PR-插件撤销事务-正文.md` |
> | **高亮事件** | `state.selection_revision` + `op:"events"` 的 `kind:"selection"` 事件（宿主本来只广播给 V4 插件，自动化客户端只能轮询） | `src/app/{control/mod.rs,update/mod.rs}`、`src/mcp.rs`、`docs/automation/README.md` | `feat/automation-selection-events` | `8845d5f8`+`7bb6d1e2` | `桌面/OCSM/PR-高亮选择事件-正文.md` |
>
> ⚠️ **同步注意（重要）**：`crates/ocs_plugin_api/src/host.rs` 与 `ipc/protocol.rs` 在本 fork 里还带着**尚未上游的 v5 插件面**（`ensure_layers/ensure_linetypes/ensure_text_styles/ensure_dim_styles`、
> `add_block_record`、`SelectedHandles`、`SetCurrentLayer`、frame picker 等；实测上游 `d738ebfc` 里 `ensure_layers`/`add_block_record`/`SelectedHandles` 均 **0 命中**）。
> 上游 PR **只含 v6 追加部分**（追加在 trait/enum 末尾，保 vtable 槽位），合并时两边都要留：上游 v6 段 ≠ fork v5 段，别把 fork 的 v5 段当成已上游。
>
> **同步动作**：上游合并后取上游版（内容一致，git 自动归并）；删本节 + 删分支 `feat/plugin-undo-transaction`、`feat/automation-selection-events`。
> 插件侧（`crates/ocs_ocsm`，非上游）已在 `20ea5ef6` 改用 Begin/Commit；命令驱动流程继续用 `push_undo`（同一条 message 内本来就成立）。

## 0.8 已开上游 issue · 面板扩展点 RFC（2026-09-16）

> **#1303**「RFC: plugin-supplied dock panels — declarative widget surface instead of embedding
> external windows」· https://github.com/HakanSeven12/OpenCADStudio/issues/1303
>
> **背景**：宿主 dock 的 `PanelId` 是**编译期枚举**（`dock.rs` 原注释：*"New palettes add a
> variant"*），第三方 `.so` 插件拿不到面板。本 fork 先做过通用方案 —— 宿主预留停靠列 + 上报矩形，
> 插件把自己的 GTK3+WebKitGTK 进程用 `XReparentWindow` 嵌进去（`cc5ec098`）：**实测像素级成功**
> （槽位 385×554 逻辑 @(640,184) → 481×692 设备像素 @(800,230)），但代价是
> ①X11/XWayland 专属（宿主须 `env -u WAYLAND_DISPLAY` 启动，Wayland/Windows/macOS/wasm 全废）
> ②多一个 GTK 进程 ③200ms 轮询矩形 + GDK 反复重申几何 ④焦点交接脆弱 ⑤为矩形握手扩 ABI（v7）
> —— 已在 `a4f122ec` 整体删除（−1811 行，含 `crates/ocs_webpanel` 5 个文件）。
>
> **issue 里给出两件事**：
> 1. **建议形态**：插件在 `plugin.toml` 声明面板 + 一棵声明式控件树（`row`/`column`/`scrollable`/
>    `label`/`button`/`text_input`/`checkbox`/`select`/`list`/`progress`），宿主用同一套 iced 控件渲染，
>    事件带 `widget_id` 回插件 —— 跨平台、wasm 无害、无第二进程；明确反对做立即模式绘图面。
> 2. **可独立成小 PR 的抓手**：每面板宽度策略（`min_width`/`max_width`/`max_fraction`）
>    + `Length::Fixed(width)` 宽度契约（`Fill` 面板会与画布抢同一行 —— fork「占半屏」的根因）。
>
> **同步动作**：无需同步代码。等维护者回复：若只要「小 PR」，按 §2 F-1 的宽度策略段落剥独立分支
> （英文注释、去掉 Pi 依赖、以现有面板为消费者）；若接受声明式控件面，另开设计分支 ——
> **不要**复活 `cc5ec098` 的 reparent 方案。
> 本地材料：`~/桌面/OCSM/issue-插件面板UI-正文.md` + `issue-插件面板UI-链接.txt`。

## 0.9 分支已推、**PR 未开** · 插件请求跟「活跃图纸」（2026-09-16）

> **修复**：插件自建 HTTP 服务（OCSMechanical 的 `GET /api/guide`、`POST /api/apply_refresh`…）
> 发的**无 tab 请求**原先被 `Message::DrainPluginRequests` 从 **tab 0** 开始 drain，
> 于是“总是答在启动那张图上”：读错（`DocumentSnapshot`/`SelectedHandles`/图层扫描）、写也落错图。
> 一张图时看不出来，一旦有第二个标签（New/Open/自动化建图）就暴露（实测：活动图里的引导线
> 在插件 HTTP 侧报 `no such guide entity`，而插件命令 `GDIM/OCSM` 看得到）。
> 修法：drain 顺序改为**先活跃标签、再其它**（带 tab id 的请求仍回自己的 session）。
>
> | 项 | 值 |
> |---|---|
> | 文件 | `src/app/update/mod.rs`（drain 顺序） |
> | fork 提交 | `3b35b258` |
> | PR 分支 | `fix/plugin-requests-active-tab`（已推 `prf` 远端） |
> | 验证 | OCSMechanical + 自动化客户端建第二图：修前 `no such guide entity`/`400`，修后 `200` + 几何，
> 生成的标注落进**活跃**文档（`op:"query"` 在 8符号标注层查到）；宿主 `cargo test --lib` 973 passed |
> | 报告 | `桌面/OCSM/PR-插件请求活动标签页-{正文.md,链接.txt}` |
>
> **同步动作**：上游合并后取上游版（内容一致时 git 自动归并）；删本节 + 删分支
> `fix/plugin-requests-active-tab`。**本条 2026-09-17 才登记进台账**（先前只在 fork 提交里）。

## 0.10 分支未开 PR · 插件一步提交多实体（2026-09-17，`CommandStep::CommitEntities*`）

> **缺口**：插件命令一步只能落**一个**实体（`CommandStep::Commit` / `CommitAndEnd`），
> 而宿主 `CmdResult` 早就有 `CommitEntities` / `CommitEntitiesAndExit`（镜像/复制等内置命令在用）。
> 插件要画“复合几何”（例：`OCSMCENTERLINE` 的十字中心线 = **两条 `LINE`**）就只能
> ① 塞进匿名块（用户没法直接修剪/夹点），或 ② 走插件 HTTP 通道异步补实体（撤销分组/原子性变差）。
>
> **改法**（两处，纯追加）：
> * `crates/ocs_plugin_api/src/host.rs`：`CommandStep` **末尾**追加
>   `CommitEntities(Vec<EntityType>)` / `CommitEntitiesAndExit(Vec<EntityType>)`
>   —— 必须追加在末尾：该类型走 **bincode**（`ipc::transport`），动已有变体的判别值就断旧插件；
> * `src/app/plugin_host.rs`：`plugin_step_to_result()` 加两条映射到宿主同名 `CmdResult`。
>
> | 项 | 值 |
> |---|---|
> | 文件 | `crates/ocs_plugin_api/src/host.rs`（+~22）、`src/app/plugin_host.rs`（+2 + 1 测试） |
> | 验证 | 宿主 `plugin_commit_entities_and_exit_lands_both_in_one_undo_entry`：两条线一次落图 + 命令结束 + **一个**撤销条目 + `undo` 一次两条一起回去；插件侧 `centerline::` 15 个单测 |
> | 报告 | （待写）`桌面/OCSM/PR-插件多实体提交-正文.md` |
>
> **同步动作**：上游若接受，取上游版；若上游改成别的形态（如 `CommitMany`），
> 插件侧只需改 `centerline.rs` 的返回值（其余逻辑不依赖）。

## 1. 补丁总表（基准：上游 v2026.37 `fc1788df` → HEAD，47 文件 / +10991 −96）

> A–E 组的行数是 v2026.36 基准时的记录（功能性描述仍适用）；F 组为 2026-09-16 新增。

| 组 | 文件 | 规模 | 作用 |
|---|---|---|---|
| **A. 插件 API v5（基础设施）** | `crates/ocs_plugin_api/src/host.rs` | +349 | HostApi 扩展方法 + 数据类型 |
| | `crates/ocs_plugin_api/src/ipc/protocol.rs` | +238 | `PluginRequest/Response` 追加变体 |
| | `crates/ocs_plugin_api/src/ipc/client.rs` | +155 | 代理转发（V2/V3） |
| | `crates/ocs_plugin_api/src/ipc/v4/client.rs` | +177 | V4 代理转发 |
| | `crates/ocs_plugin_api/src/ipc/server.rs` | +19 | 服务端分发 |
| | `crates/ocs_plugin_api/src/process.rs` / `process/v4.rs` / `runner.rs` | +123 / +6 / +208 | 交互命令、预览实体、通知 |
| | `crates/ocs_plugin_api/src/manifest.rs` | +7 | `API_VERSION` 4 → **5** |
| | `Cargo.toml` | +5 | workspace 成员（ocs_ocsm / ocs_ocsm_mcp）+ **acadrust rev pin** |
| **B. 宿主落地与 UI** | `src/app/plugin_host.rs` | +1095 | v5 请求在宿主的实现 |
| | `src/app/update/mod.rs` | +96 | drain 插件请求 + 图框选择回调 |
| | `src/app/view/modal.rs` | +75 | `OcsmFramePicker` 模态框 |
| | `src/app/mod.rs` | +38 | 图框选择/待取状态 |
| | `src/app/update/viewport.rs` | +88 | 插件命令的对象捕捉/点选语义 |
| | `src/app/view/mod.rs` | +5 | 插件点选模式隐藏十字 |
| | `src/ui/overlay.rs` | +61 | 拾取方框光标（纯方框，无十字） |
| | `src/command.rs` | +36 | 插件交互命令：预览实体、`hides_crosshair` 等钩子 |
| | `src/app/commands/mod.rs` | +17 | **插件命令优先于别名表**（`plugin_wins`） |
| | `src/scene/mod.rs` | +3 | `tessellate_one()`（预览用单实体剖分） |
| **C. 渲染器** | `src/entities/dimension.rs` | +189 | **C-1** DIMTMOVE=1 线性引线（本次）+ 4 测试；**C-2** 标注文字宽度自适应 |
| | `src/entities/text_support.rs` | +30 | MTEXT 花括号作用域字体恢复（`{\fGDT;x}` 供符号字形） |
| **D. 测试** | `src/plugin/external.rs` | +67 | `OCS_SMOKE_PLUGIN` 外部插件测试（纯新增） |
| | `tests/dim_leader_render_check.rs` | +107 | 引线渲染级测试（新文件） |
| **E. 插件本体（无需台账）** | `crates/ocs_ocsm/**`、`crates/ocs_ocsm_mcp/**` | 4400+ | 与上游天然解耦；仅 `Cargo.toml` 成员需保留 |
| **F. 内建面板：Pi 助手（fork 本地，2026-09-16）** | `src/ui/pi_panel.rs`（新） | +2239 | 面板 UI（聊天流 / 审批 / 用量统计 / 表格横向滚动） |
| | `src/pi.rs`、`src/pi_rpc.rs`、`src/app/update/pi.rs`（新） | +1957 / +961 / +328 | pi-web HTTP 后端 / pi RPC 子进程后端 / 消息处理 |
| | `src/ui/dock.rs` | ~+30 | `PanelId::Pi` 变体 + `ALL` + `title()` + 每面板宽度策略（340 / 150 / 1200 / 0.4）+ 默认钉住 |
| | `src/app/view/mod.rs` | ~+20 | 面板渲染分支（展开态 L1863 / 收边态 L2945）+ 10Hz `pi_poll`（L2589） |
| | `src/app/{mod.rs,document.rs,update/mod.rs,update/dialog.rs}` | ~+40 | `Message::Pi` / `PiImagePasted` 路由、`show_pi_panel` 开关、**每标签页** `PiPanelState`、× 关闭语义 |
| | `src/lib.rs`、`src/ui/mod.rs` | +3 | 模块注册（`pi`、`pi_rpc`、`pi_panel`） |
| | 依赖 | 0 | **无新增 crate**（`ureq`/`base64`/`image` 上游本有；仅 `Cargo.lock` +3 行） |

## 2. 关键补丁详情

### C-2 `src/entities/dimension.rs` — 标注文字宽度自适应（fork 本地）

- **位置**：`fn dimension_text_entity()`（`DA::` → `MA::` 映射之后）。
- **为什么**：cadcodec 的 `MText` 默认 `rectangle_width = 10`，长 dimtext（测量值 + 公差堆叠 +
  后缀，如 `Ø100+H7/g6+`）会被按 10 单位宽度折行。
- **改法**：按可见字符数估算宽度 `(visible_len * text_height * 0.75).max(10.0)`。
- **验证**：给标注加长文字/公差，观察不折行。
- **合并注意**：与 C-1 同文件不同区域；**上游 PR 分支不含此项**（见下）。

### C-1 `src/entities/dimension.rs` — DIMTMOVE=1 引线（2026-09-13，提交 `cbaf6d61f`）
> ⚠️ **已上游化**：PR **#1235** 已合并（`64e64c80`，上游改用 kernel 渲染器实现）
> → 下轮同步**按上游版、删本条目**，见 §0.5。

- **位置**：私有 `dimtmove_leader_endpoints()`（重写）+ 新增私有 `linear_leader()` +
  2 处调用点（`tessellate_dimension_inner` ≈L3768、baked 渲染 ≈L7183）+ 测试模块
  `dimtmove_leader_tests`（4 例）。
- **为什么**：旧实现把引线画成「尺寸线**中点** → 文字」的斜线；机械制图（GB/T 4458.4，
  同 AutoCAD DIMTMOVE=1）要求文字移出尺寸线范围时，**尺寸线沿轴延伸到文字底下**。
- **接口影响**：无（全部私有）；**行为被 `DIMTMOVE=1` 门控**（宿主默认 `dimtmove=0`，
  故上游/其它图档行为不变；目前只有 OCSM 生成/转换的标注在实体 XDATA 里置 1）。
- **验证**：`cargo test -p OpenCADStudio --lib dimtmove`；
  `OCSM_DIM_LEADER_PDF=/tmp/x.pdf cargo test --test dim_leader_render_check` → PDF 转图目视。
- **合并注意**：若上游重写该引线，保留"文字在外→水平延伸"这一形状语义（可对照本行说明）。
- **已提交上游 PR**（2026-09-13）：分支 `fix/dimtmove-leader-under-text`（fork
  `YsDirector/OpenCADStudio`），基点 = 上游 `main` `052b6b23`，提交 `e7675160`。
  PR 分支上该文件的注释为**英文**（上游语境），且**不含 C-2**（文字宽度补丁是 fork 本地项）。
  即：PR 分支与本仓库该文件会有注释语言差异——上游若合并，记得同步回来（或保留 fork 版本）。

### B-1 `src/app/commands/mod.rs` — 插件命令优先于别名表（`plugin_wins`）

- **为什么**：OCSM 的 `D`（OCSMPOWERDIM）、`CC`（粗糙度）必须压过随包别名 `D`→`*DIMSTYLE`、
  `CC`→`COPYCLIP`，否则用户敲不出来。
- **合并注意**：上游若重构别名解析（`src/app/alias.rs` 与 dispatch 顺序），要保证
  **先查插件命令、后查别名**。

### A-1 插件 API v5（`crates/ocs_plugin_api`）

- **新增 HostApi**（全部带默认实现，旧插件仍可加载）：`selected_handles`、`set_current_layer`、
  `ensure_layers`、`ensure_linetypes`、`ensure_text_styles`、`ensure_dim_styles`、
  `show_frame_picker`、`take_pending_frame_selection`、`import_frame_block`、
  `add_block_record`，以及交互命令钩子：`wants_text_input`、`on_text_input`、
  `wants_mouse_move`、`on_mouse_move`、`on_object_pick_snapped`、
  `entity_pick_applies_osnap`。
- **IPC**：`PluginRequest/PluginResponse` **末尾追加**变体（bincode discriminant 稳定）；
  类型：`LayerDef`/`LinetypeDef`/`TextStyleDef`/`DimStyleDef`/`FrameItem`/`FrameSelection`/
  `ImportFrameBlockRequest`。
- **合并注意**：若上游也改 `crates/ocs_plugin_api`，**优先取上游**，再把本组的追加项补回
  （追加在枚举/方法末尾，保持既有次序）。

### A-2 `Cargo.toml` — workspace 成员与 acadrust pin

- 成员新增 `crates/ocs_ocsm`、`crates/ocs_ocsm_mcp`；
  `acadrust` 固定到 fork 镜像 rev（`ghfast.top/.../cadcodec`）。
  **2026-09-14 上游同步时 rev 由 `5b56571a` 升到 `5eea24cf`（acadrust 0.5.5）**：上游 542 提交依赖
  更新的 cadcodec/cadkernel（DIMTMOVE kernel 渲染器等），沿用旧 rev 无法编译；**镜像 URL 保持 ghfast.top，
  未被覆盖成官方 URL**。宿主与插件经 `cargo tree` 确认共用同一份 acadrust（无重复版本），插件 178 测试全过
  → 实体 API 未破坏。这符合本节原定规则「上游更新 rev 时同步升 rev 并重跑插件测试」。
- **合并注意**：上游更新 acadrust rev 时，需确认插件依赖的实体 API（ATTDEF/块/标注字段）
  没变；必要时同步升 rev 并重跑插件测试。**不要**让上游覆盖成本地的镜像 URL（网络环境原因）。

### F-1 `src/ui/` 等 — 内建 Pi 助手面板（fork 本地，2026-09-16）

- **为什么**：宿主 dock 的 `PanelId` 是编译期枚举，第三方插件拿不到面板（→ §0.8 的 RFC）。
  本面板是**内建**面板，用的就是上游 dock 自己的扩展方式（"New palettes add a variant"），
  **未改任何插件 ABI**（`ocs_plugin_api` 仍是 v5；`cc5ec098` 的 v7 追加已随 `a4f122ec` 删除）。
- **接点清单**（同步上游后逐项核对，缺一即面板报错或丢失；行号为 2026-09-16 的值，
  实际以符号名搜索 `PanelId::Pi` / `pi_poll` / `PiPanelState` / `on_pi_msg` 为准）：
  1. `src/ui/dock.rs`：`PanelId::Pi` 变体、`ALL: [PanelId; 3]`、`title()`、`default_width()` 340、
     `min_width()` 150、`max_width()` 1200、`max_fraction()` 0.4、`auto_collapse` 默认 `true`（钉住）。
  2. `src/app/view/mod.rs`：`expanded_panel` 的 `PanelId::Pi` 分支（L1863）、收边/占用分支（L2945）、
     `pi_poll` 订阅（L2589：`show_pi_panel && worker.is_some()` 时 100ms）。
  3. `src/app/mod.rs`：`Message::Pi`（L1900）、`show_pi_panel` 开关（L688 / 初始化 L3659）。
  4. `src/app/document.rs`：**每标签页** 的 `pi_panel: PiPanelState`（L133 / 初始化 L585）——
     上游若重构 Tab 结构，这处要跟着搬。
  5. `src/app/update/mod.rs`：`Message::Pi` → `on_pi_msg`、`Message::PiImagePasted` →
     `on_pi_image_pasted`（L499/502）。
  6. `src/app/update/dialog.rs`：dock × 关闭语义（× 只是取消停靠，面板仍在栈里，`PI` 命令可再开）。
  7. 模块注册：`src/lib.rs`（`pub mod pi; pub mod pi_rpc;`）、`src/ui/mod.rs`（`pub mod pi_panel;`）、
     `src/app/update/`（`mod pi;`）。
- **接口影响**：插件 ABI 无改动。但面板 view **必须** `.width(Length::Fixed(width))` —— `Fill`
  会与画布抢同一行空间（「占半个屏」的根因，提交 `675e552b`）；宽度策略在 `7ee77938`。
- **验证**：`cargo test --lib pi` → **145 passed**；实测：`PI` 命令开面板、后端标签
  （`pi rpc · <cwd>` 或 `pi-web · <url>`）、用量统计行、表格横向滚动、审批 UI
  （细节见 `docs/ocs-pi-extension.md`）。
- **合并注意**：
  - 按 §0 规则「冲突一律取上游版」会**丢掉全部 7 处接点**，必须手工补回（清单见上）；
    `PanelId::ALL` 的数组长度、`auto_collapse` 默认值、`Fixed(width)` 契约是三个易丢点。
  - 若上游同步后要跑 **wasm 构建**（上游有 pages 工作流），`src/pi.rs` / `pi_rpc.rs` /
    `ui/pi_panel.rs` 依赖 `ureq`、文件系统与子进程 → 需 `#[cfg(not(target_arch = "wasm32"))]`
    门控，否则 wasm 目标编译失败。
  - 面板需要**运行时环境**（本机 `pi` CLI 或 pi-web HTTP）：不影响构建，没有它时面板显示错误态。

## 3. 已知取舍

- 宿主补丁刻意保持**最小、可局部合并**：C 组只有 1 个文件、1 个私有函数；
  A/B 组是"给插件开的口子"，追加式修改，不动既有语义。
- 若上游接受，C-1 可作为上游 PR（旧行为从尺寸线中点拉引线本身可疑）。
  D 组渲染测试与 C-2 曾考虑作上游贡献候选，**用户判断采纳概率不大（2026-09-14）→ 不再推进**。
- ✅ **2026-09-14 状态更新**：C-1（→ PR #1235）与 Ctrl+点击超链接（→ PR #1234）**均已被上游合并**。
  `tests/leader_smoke_render.rs`（引线渲染测试）与 C-2（标注文字宽度自适应）**仍为 fork 本地**；
  用户判断这两项**上游采纳概率不大** → **不作为上游贡献推进**，仅本地保留。
- ⚠️ **2026-09-16 结论：「让第三方插件加窗口」的通用机制不作为上游候选。** 该机制
  （`cc5ec098`：`PanelId::Web` + `HostApi::web_panel_rect` + 插件 API v7 + `crates/ocs_webpanel`）
  因 X11/XWayland 专属等五点代价（见 §0.8）已在 `a4f122ec` 整体删除；**同步/合并时不要**把它
  当作「已上游化的能力」复活，也不要把它算作本 fork 的待推补丁。替代路线（插件声明式控件面）
  已开 issue #1303 征询意向。
- 供参考：内建 Pi 面板的宿主侧改动属**本地独有**（F 组），**不由**上游候选清单管理；
  它依赖外部 `pi` 运行时，是产品选择而非通用补丁，故不推上游。
