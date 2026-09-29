# ocs_session 守卫回归测试（`tools/tests/ocs_session_guard/`）

★ 这是「`tools/ocs_session.py` 绝不误连别人（尤其是用户正在画图的那一个）实例」这条**安全属性**
的唯一回归测试。原来它只活在 `/tmp/ocsfix/`（重启即失），现在冻结进仓库，可独立复跑。

## 一键跑

```sh
tools/tests/ocs_session_guard/run.sh
# 等价：python3 tools/tests/ocs_session_guard/run_tests.py
```

零 GUI、零 cargo，全程几秒到几十秒（含真宿主探测）。退出码 `0` = 没有失败断言（SKIP 不计入通过），
`1` = 有断言失败。`OCS_GUARD_KEEP=1` 保留临时工作目录（默认跑完删干净）。

## 它验什么

| case | 断言要点 |
| --- | --- |
| 准备 | 夹具 sha256 与记录一致（钉死）；夹具不含 `OCS_ALLOW_EXISTING`；真二进制存在；`pgrep -x OpenCADStudio` 为空 |
| 正控制 | 无外部实例 ⇒ 脚本从零起自己的宿主（wrap 收到 `--new-instance`）、隔离透传到宿主 `XDG_CONFIG_HOME`、`OCS_PLUGINS_DIR` 回填用户真实插件目录、最小 RPC（`query`）跑通、退出后临时目录与宿主都没残留 |
| 反证 A（隔离） | 共享位置放一个「别人的」活实例 ⇒ 选中的 session 不是它，★ 它收到的 TCP `connect` 数 **== 0**，脚本自己起宿主把活干完 |
| 红证（★ 关键） | **同一布局**跑「隔离版之前」的老脚本夹具 ⇒ 它 `session=FAKE-SHARED` 且该实例 `connects >= 1`，且没有自己起宿主。`connects==0`（新）对 `connects>=1`（旧）就是整套测试的有效性来源；这条掉了测试即失去意义 |
| 反证 B（逃生口） | `OCS_ALLOW_EXISTING=1` ⇒ 才连上共享位置那个实例（`connects >= 1`），stderr 有风险提示，且不再自己起宿主 |
| 反证 C1 | 宿主写出的描述符 pid 对不上 ⇒ 非 0 退出 + 「实例归属校验失败」+ 点名 pid，**不输出** `# session=`，且是归属不符（< 20s）而不是 45s 超时 |
| 反证 C2 | 私有目录里混进别人的活会话 ⇒ 发现阶段成功过（宿主收到过 hello）但校验失败，仍不输出 `# session=` |
| 反证 C3（对照） | 同一枚「pid 对不上」的描述符放共享位置 + 逃生口 ⇒ 才被允许连，说明默认模式的拒绝确实来自归属校验 |
| 正控制（真宿主探测） | `OCS_BIN` 直接指向真二进制：真跑通 = PASS；真宿主起不来（GUI 需要显示）⇒ ★ **明确打印「跳过（原因）」**，跳过绝不计入通过，也绝不当失败吞掉 |
| 收尾自证 | `pgrep -x OpenCADStudio` 仍为空；拉起过的假进程全部退出（僵尸不算）；真 `~/.config/OpenCADStudio/automation` 的文件数与最新 mtime 快照未被触碰 |

## 为什么这样造

* **全程隔离**：自带 `mktemp -d`，`HOME` / `XDG_CONFIG_HOME` / `XDG_DATA_HOME` / `XDG_CACHE_HOME` /
  `XDG_STATE_HOME` 全部指到那个临时目录，伪造实例的描述符也写在里面，跑完删除。不依赖 `/tmp/ocsfix`
  之类外部残留，也绝不碰用户真实配置目录（种子只读）。
* **夹具钉死**（★ 上一批的血泪）：`fixtures/ocs_session_pre_guard.py` 是 commit `6da03e1d` 的
  `tools/ocs_session.py` 的**逐字快照** —— 「隔离版之前」那版。上一批红证用
  `git show HEAD:tools/ocs_session.py` 取旧版本，隔离版一提交进 HEAD，同一条命令就取到了新版本，
  红证静默失效，**41/41 掉到 38/41**。所以断言要用的旧版本必须存在仓库里，测试**不读 git 历史**
  （`FIXTURE_SHA256` 就是那把钉子；改夹具必须同步改它）。
* **`OCS_BIN` 用替身脚本**（`fixtures/wrap_ocs_bin.sh`）：`--mcp` 走**真二进制**（真 MCP 服务器 +
  真描述符发现 / hello 握手链路），只有 `--new-instance` 走 `fixtures/fake_ocs_host.py`（描述符字段
  照 `src/app/control/transport.rs:44`，协议照 `transport.rs`）—— 因为真宿主是 GUI
  （`src/mcp.rs:315` 的 `start_gui` 就是 `--new-instance`），跑起来会开窗上屏，本测试禁止上屏。
* **正控制尽量用真宿主**：最后那条 case 把 `OCS_BIN` 直接指向 `target/release/OpenCADStudio`，并把
  `WAYLAND_DISPLAY` 指到一个**不存在**的通道、`XDG_RUNTIME_DIR` 指到临时目录、移掉 `DISPLAY`，
  于是「即使真宿主意外起来也绝不可能把窗口开到用户桌面上」。真宿主起不来时按「跳过（原因）」处理，
  ★ 跳过既不算通过也不算失败，只在汇总里单列。

## 文件

| 路径 | 作用 |
| --- | --- |
| `run.sh` | 一键入口（设好 `PATH`/`LC_ALL`，转发给 `run_tests.py`） |
| `run_tests.py` | 测试本体（断言 + 隔离 + 汇总；`OCS_GUARD_KEEP/WORK` 可调） |
| `fixtures/ocs_session_pre_guard.py` | ★ 冻结夹具：`6da03e1d` 版 `tools/ocs_session.py`，只用于证明断言能红，**不是可运行的现行版本** |
| `fixtures/fake_ocs_host.py` | 替身宿主（三种角色：自己起的宿主 / 共享位置里「别人的实例」 / 装 pid 对不上） |
| `fixtures/wrap_ocs_bin.sh` | `OCS_BIN` 替身：`--mcp` → 真二进制，`--new-instance` → 替身宿主 |

## 夹具重新生成（只有刻意换夹具时才做）

`fixtures/ocs_session_pre_guard.py` = `git show 6da03e1d:tools/ocs_session.py` 的逐字内容，
前面加一段说明用的注释头（注释头插在 shebang 之后、模块 docstring 之前，不影响 docstring 语义）。
重新生成后必须同步更新 `run_tests.py` 里的 `FIXTURE_SHA256`，并确认 `OCS_ALLOW_EXISTING` 仍然
**不出现**在夹具里（出现了就说明取到的是隔离版之后的版本，红证会失效）。
