# 无头 open E2E（`../ocs_session_open_e2e.sh`）

把「真·无头宿主 + 开图 + 查询/切换」这条手工驱动路线固化成一个仓库脚本：用
`tools/ocs_session.py`（自带 XDG 隔离 + 只认自己启动的宿主）驱动真二进制 `--serve` 无头宿主，
全程不开 GUI、不碰用户正在编辑的文件。

## 跑法

```sh
tools/tests/ocs_session_open_e2e.sh          # 仓库根目录或任意目录均可；手工跑，不接 CI
OCS_E2E_KEEP=1 tools/tests/ocs_session_open_e2e.sh    # 保留临时目录，便于看日志
```

`OCS_BIN` 缺省按仓库 `target/release/OpenCADStudio` 推导（它只是被 `headless_host.py` 桥接的
「真宿主」）；`OCS_E2E_EXPECT_LINES` 可覆盖模板 line 条数期望（缺省 22）。

## 覆盖什么

| 步骤 | 断言 |
| --- | --- |
| `open "<mktemp -d>/倒角 样例（优化）.dxf"`（仓库只读模板 `crates/ocs_ocsm/bom/明细表模板.dxf` 的拷贝，路径含空格/中文/全角括号、带引号） | 打印 document_id / revision / title；切到新文档 |
| `query line` | 条数 == 模板真实值（22，非空） |
| `run LINE 0,0 10,0` | `status=completed`，revision 递增 |
| 再 `query line` | 条数 +1（22 → 23） |
| `open <不存在的文件>` | 打印「文件不存在」 |
| `open <被 flock 占住的拷贝>`（`hold_lock.py` 写 `pid=<活着的自己>` 并持有真 lock） | 拒绝并点名持有者 pid，不打开 |
| 收尾 | 删临时目录；`pgrep -x OpenCADStudio` 与基线快照做差集 == 空；打印 PASS/FAIL 与 rc |

## 文件

| 路径 | 作用 |
| --- | --- |
| `../ocs_session_open_e2e.sh` | 入口（mktemp 隔离、拷贝模板、起锁、跑 `ocs_session.py`、断言、收尾） |
| `headless_host.py` | 把 `--new-instance`（GUI）桥成真 `--serve` 无头宿主：ready 横幅取真 session_id、写 `transport.rs` 格式描述符、TCP 中转 |
| `wrap_ocs_bin.sh` | `OCS_BIN` 替身：`--new-instance` → 桥；`--mcp` → 真二进制 |
| `hold_lock.py` | 造一个「别人的实例」持有的真 flock 编辑锁（sidecar 命名/内容照 `src/io/edit_lock.rs`） |

★ 仓库里这些夹具保持 644；E2E 脚本先拷进临时目录再 `chmod 755`，不动仓库里的模式位。
