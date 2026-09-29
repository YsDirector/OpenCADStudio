# 夹具说明（`ocs_drive_guard/fixtures/`）

* `ocs_drive_pre_guard.py` —— ★ commit `98b472f9` 的 `tools/ocs_drive.py` 的**逐字节快照**
  （sha256 `4f095f8c64969d86cce40fc423e0110e0aa3020f090b7bd8f77f40218cce93cb`，152 行 / 5736 B）。
  ★ **只用于证明断言能红，不是可运行的现行版本**：它没有默认守卫，直接跑会连上发现到的任意实例。
  为避免 sha256 对不上，说明文字不放夹具文件头（见上一级 `README.md`）。
  重新生成：`git show 98b472f9:tools/ocs_drive.py > tools/tests/ocs_drive_guard/fixtures/ocs_drive_pre_guard.py`
  （只有刻意换夹具时才做；之后必须同步 `run_tests.py` 的 `FIXTURE_SHA256` 与 README 的行数/体积并重跑红证）。
* `fake_ocs_instance.py` —— 自家无头实例夹具（真格式描述符 + 真协议），由 `run_tests.py` 驱动。
* `wrap_ocs_bin.sh` —— `OCS_BIN` 替身（`--mcp` → 真二进制；`--new-instance` → 无头实例夹具）。

★ `fixtures/*` 保持 `644`；需要执行的 `wrap_ocs_bin.sh` 由测试拷进临时目录再 `chmod 755`。
