# ocs_drive 守卫回归测试（`tools/tests/ocs_drive_guard/`）

★ 这是「`tools/ocs_drive.py` 默认不猜目标实例」这条**安全属性**的唯一回归测试：默认拒绝执行
（rc=2，且拒绝先于任何 TCP 连接），只有 `--session <id>` / `--allow-existing`（或
`OCS_ALLOW_EXISTING=1`）三种显式许可才驱动现有实例。原来这套验证只活在 `/tmp/b_drive_check.sh`
（重启即失），而且只有「拒绝/允许」的正向输出，没有「旧版会连上去」的能红证据；现在两者都冻结进仓库。

## 一键跑

```sh
tools/tests/ocs_drive_guard/run.sh
# 等价：python3 tools/tests/ocs_drive_guard/run_tests.py
```

零 GUI、零 cargo，全程临时目录（含真二进制 `--mcp` 客户端握手，几秒到几十秒）。退出码 `0` = 没有
失败断言（SKIP 不计入通过），`1` = 有断言失败。`OCS_GUARD_KEEP=1` 保留临时工作目录（默认跑完删干净）。

## 它验什么

| case | 断言要点 |
| --- | --- |
| 准备 | 夹具 sha256 与记录一致（钉死）；夹具不含显式许可；真二进制存在；`pgrep -x OpenCADStudio` 为空 |
| 起靶子 | `tools/ocs_session.py`（`OCS_BIN` 替身 + `OCS_KEEP_HOST=1`）自己拉起自家无头实例 A（`# session=DRIVE-A`）；直接起第二个实例 B，两者描述符都在**同一共享临时 XDG** |
| list | 唯一不需要许可的 op：rc=0、列出两个实例、一次 TCP 连接都不发 |
| 红证（★ 关键） | **同一布局**跑「加固前」的冻结夹具 ⇒ 旧脚本 rc=0、打印出 `pid=… mode=gui … doc=[…]` 实例信息，且 A/B 的 `connects >= 1`。新版默认这条 `connects == 0` —— `connects>=1`（旧）对 `connects==0`（新）就是整套测试的有效性来源；这条掉了测试即失去意义 |
| 反证（默认） | 新版不带许可 ⇒ rc=2 + stderr「拒绝执行」+ 给出显式许可办法，★ 且 **A/B connects == 0**（拒绝先于任何连接） |
| 反证（flag 许可） | `--allow-existing` ⇒ 才连上（connects >= 1），stderr 有逃生态风险提示 |
| 反证（env 许可） | `OCS_ALLOW_EXISTING=1` ⇒ 才连上（connects >= 1），stderr 有逃生态风险提示 |
| 反证（精确目标） | `--session DRIVE-B` ⇒ rc=0、只打印 B 的 pid 行，B `connects >= 1` 且 A `connects == 0` |
| 反证（无实例） | 空布局 ⇒ rc=1 + stderr 点名「没有匹配」（绝不静默成功） |
| 收尾自证 | `pgrep -x OpenCADStudio` 仍为空；拉起的靶子全部退出；真 `~/.config/OpenCADStudio/automation` 的文件数与最新 mtime 快照未被触碰 |

`connects` 来自每个实例夹具写的 JSON 事件日志（`connect` / `request` 带时间戳），只统计**本次运行期间**
的连接，所以「到底连没连」是可判定的硬指标，不靠输出文本猜。

## 为什么这样造

* **全程隔离**：自带 `mktemp -d`，`HOME` / `XDG_CONFIG_HOME` / `XDG_DATA_HOME` / `XDG_CACHE_HOME` /
  `XDG_STATE_HOME` 全部指到那个临时目录（靶子描述符也写在里面），跑完删除。不依赖 `/tmp/b_drive_check.sh`
  之类外部残留，搬进仓库后就能独立跑；也绝不碰用户真实配置目录（只读快照）。
* **夹具钉死**：`fixtures/ocs_drive_pre_guard.py` 是 commit `98b472f9` 的 `tools/ocs_drive.py` 的
  **逐字节快照**（sha256 记在 `run_tests.py` 的 `FIXTURE_SHA256`），红证只许用它。测试**不读 git 历史**；
  夹具里**不加任何注释头**（加了 sha256 就对不上，说明文字一律放本 README 和 `fixtures/README.md`）。
* **无头实例**：真宿主是 GUI（`--new-instance`，`src/mcp.rs:315`），本机无显示且测试禁止上屏，所以
  `OCS_BIN` 走 `fixtures/wrap_ocs_bin.sh`：`--new-instance` 用自家无头实例夹具
  （`fixtures/fake_ocs_instance.py`，描述符字段照 `src/app/control/transport.rs:44`、协议照
  `transport.rs` 的 token+session_id 校验、答完即关写端），`--mcp` 用真二进制（真 MCP 客户端 / 真描述符发现）。
  第二个靶子由 `run_tests.py` 直接拉起同一夹具——这样 `--session` 的「只驱动那一个」才有可判定差异。

## 夹具（钉死 + 可核对）

| 项 | 值 |
| --- | --- |
| 文件 | `fixtures/ocs_drive_pre_guard.py` = `98b472f9:tools/ocs_drive.py` 的逐字节快照 |
| sha256 | `4f095f8c64969d86cce40fc423e0110e0aa3020f090b7bd8f77f40218cce93cb` |
| 行数 / 体积 | 152 行 / 5736 B |
| 性质 | ★ 只用于证明断言**能红**，**不是可运行的现行版本**：它没有默认守卫，直接跑会连上发现到的任意实例 |

可核对：

```sh
git show 98b472f9:tools/ocs_drive.py | sha256sum     # 应与上表 sha256 完全一致
git show 98b472f9:tools/ocs_drive.py | wc -lc        # 152 5736
```

★ 若哪天故意换夹具，必须同步改 `run_tests.py` 的 `FIXTURE_SHA256` + 本表的 sha256/行数/体积，
并重跑红证（旧夹具那条必须仍然 `connects >= 1`）；否则红证会静默失效。

## 权限约定

`run.sh` 带执行位（`chmod +x`，可直接 `./run.sh`）；`fixtures/*` 保持 `644` —— 需要执行的夹具
（`wrap_ocs_bin.sh`）由 `run_tests.py` 先拷进临时目录再 `chmod 755`，仓库里那份的模式位不动。

## 文件

| 路径 | 作用 |
| --- | --- |
| `run.sh` | 一键入口（设好 `PATH`/`LC_ALL`，转发给 `run_tests.py`） |
| `run_tests.py` | 测试本体（断言 + 隔离 + 汇总；`OCS_GUARD_KEEP/WORK` 可调） |
| `fixtures/ocs_drive_pre_guard.py` | ★ 冻结夹具：`98b472f9` 版 `tools/ocs_drive.py`（逐字节），只用于证明能红 |
| `fixtures/fake_ocs_instance.py` | 自家无头实例夹具（真格式描述符 + 真协议；也用来起第二个靶子） |
| `fixtures/wrap_ocs_bin.sh` | `OCS_BIN` 替身：`--mcp` → 真二进制，`--new-instance` → 无头实例夹具 |
| `fixtures/README.md` | 夹具用途与「不是现行版本」的警告（靠文件最近处） |
