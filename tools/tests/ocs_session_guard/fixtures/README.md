# fixtures/ —— 冻结夹具（★ 不要运行它 ✗）

`ocs_session_pre_guard.py` = commit `6da03e1d` 的 `tools/ocs_session.py` 的 **逐字节快照**
。
* 166 行 / 6945 B / sha256 `ed2a5357d0228e6a…`（完整值见 `../README.md`）
* 核对方法：`git show 6da03e1d:tools/ocs_session.py | sha256sum` ⇒ ★ 必须与本文件 sha256 **完全相同** ✗

## ★ 为什么不给它加头注释 ✗

因为**夹具的全部价值就在"sha256 可核对"** ✗ —— 一旦加注释（哪怕只是说明文字 ✓），sha256 立刻对不上 ✓，
"冻结的就是那一版"这句话就不再成立 ✗。
（这一批真踩到过：给夹具加了 15 行说明 ⇒ 它变成 `2c2dce15…`，与真身 `ed2a5357…` 不符 ✗，
   虽然逻辑一字未改 ✓，但"逐字快照"的声明就是失实的 ✓ ⇒ 说明一律写在 README 里 ✓，`.py` 本体保持纯净 ✓。）

## ★ 它是干什么用的 ✗

只用于证明断言**能红** ✗：同一布局下，它会**连上在 `automation/` 里发现到的任意实例**
（★ 包括别人正在编辑的那一张 ✗），而现行版在同样布局下 `connects == 0` ✓。
⇒ ★ **绝对不要直接运行它** ✗ —— 它没有隔离发现、没有归属校验、没有 `OCS_ALLOW_EXISTING` 逃生口 ✓，
   直接运行 = 可能把写操作打进用户的图 ✓。（仅供 `../run_tests.py` 在隔离环境里当"旧版"跑 ✓。）

## 重新生成（★ 只有刻意换夹具时才做 ✗）

```bash
git show <要冻结的那版 rev>:tools/ocs_session.py > fixtures/ocs_session_pre_guard.py
# ★ 然后：① 同步更新此处与 ../README.md 里的 sha256 / 行数 / 体积
#        ② 重跑 ../run.sh，确认"能红"那条仍然红（connects > 0）
```

其余文件：`fake_ocs_host.py`（替身宿主 ✓，用于不想起真进程的路径 ✓）、
`wrap_ocs_bin.sh`（把 `OCS_BIN` 换成本仓库夹具的包装 ✓）。
