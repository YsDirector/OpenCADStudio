# 本地（fork）补丁台账 · Local patch inventory

> 本仓库是 OpenCADStudio 的**二开 fork**：主线功能 + `crates/ocs_ocsm`（OCSMechanical
> 机械工具包插件）+ 为插件打通宿主所需的**本地补丁**。本文件登记所有与上游不同的
> 宿主侧改动，供**每次同步上游后照单核对**，避免补丁被合掉却无人发现。
>
> 上游：`origin` = HakanSeven12/OpenCADStudio（经 ghfast 镜像）
> 最近同步点：`80733ecb4`（Merge upstream **v2026.36**，上游父 = `ddbbdd27b`）
> 台账基准：`git diff 80733ecb4^2..HEAD`（上游 v2026.36 → 当前本地 HEAD）

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
cargo test -p ocs_ocsm --lib                           # 插件 155
cargo test -p OpenCADStudio --lib dimtmove              # 宿主 DIMTMOVE 引线 4
OCS_SMOKE_PLUGIN=$PWD/target/release/libocs_ocsm.so \
  OCS_PLUGIN_RUNNER_EXE=$PWD/target/release/OpenCADStudio \
  cargo test -p OpenCADStudio --lib installed_plugin_registers_its_ribbon
# ④ 重启 OCS 复测
```

⚠️ **用户启动入口**：`~/桌面/OpenCADStudio.desktop` → `Exec=<repo>/target/release/OpenCADStudio`。
所以**改到宿主 `src/` 的渲染/几何逻辑必须重编宿主**（只换 `.so` 会看到旧渲染，
2026-09-13 曾因此误判"改了没效果"）。

## 1. 补丁总表（基准：上游 v2026.36 → HEAD，24 文件 / +2972 −119）

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
| **D. 测试/冒烟** | `src/plugin/external.rs` | +67 | `OCS_SMOKE_PLUGIN` 外部插件冒烟（纯新增） |
| | `tests/dim_leader_render_check.rs` | +107 | 引线渲染级冒烟（新文件） |
| **E. 插件本体（无需台账）** | `crates/ocs_ocsm/**`、`crates/ocs_ocsm_mcp/**` | 4400+ | 与上游天然解耦；仅 `Cargo.toml` 成员需保留 |

## 2. 关键补丁详情

### C-2 `src/entities/dimension.rs` — 标注文字宽度自适应（fork 本地）

- **位置**：`fn dimension_text_entity()`（`DA::` → `MA::` 映射之后）。
- **为什么**：cadcodec 的 `MText` 默认 `rectangle_width = 10`，长 dimtext（测量值 + 公差堆叠 +
  后缀，如 `Ø100+H7/g6+`）会被按 10 单位宽度折行。
- **改法**：按可见字符数估算宽度 `(visible_len * text_height * 0.75).max(10.0)`。
- **验证**：给标注加长文字/公差，观察不折行。
- **合并注意**：与 C-1 同文件不同区域；**上游 PR 分支不含此项**（见下）。

### C-1 `src/entities/dimension.rs` — DIMTMOVE=1 引线（2026-09-13，提交 `cbaf6d61f`）

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
  `acadrust` 固定到 fork 镜像 rev（`ghfast.top/.../cadcodec` rev `5b56571a`）。
- **合并注意**：上游更新 acadrust rev 时，需确认插件依赖的实体 API（ATTDEF/块/标注字段）
  没变；必要时同步升 rev 并重跑插件测试。**不要**让上游覆盖成本地的镜像 URL（网络环境原因）。

## 3. 已知取舍

- 宿主补丁刻意保持**最小、可局部合并**：C 组只有 1 个文件、1 个私有函数；
  A/B 组是"给插件开的口子"，追加式修改，不动既有语义。
- 若上游接受，C-1 可作为上游 PR（旧行为从尺寸线中点拉引线本身可疑）；
  D 组冒烟测试也是不错的 upstream 贡献候选。
