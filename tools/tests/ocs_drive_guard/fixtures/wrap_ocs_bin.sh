#!/bin/sh
# OCS_BIN 替身（测试夹具；run_tests.py 会把它拷进临时目录再 chmod 755，仓库里这份保持 644）：
#   --mcp          → 真二进制（tools/ocs_session.py 用它做 MCP 客户端 / 实例发现）
#   --new-instance → 自家无头实例夹具（真宿主是 GUI，会开窗上屏，本测试禁止上屏）
# 环境变量（由 run_tests.py 提供）：
#   OCS_REAL          真二进制路径（target/release/OpenCADStudio）
#   OCS_FAKE_INSTANCE fixtures/fake_ocs_instance.py
#   OCS_WRAP_LOG      收到 argv 的日志（用来判定会话工具有没有自己起宿主）
echo "argv=$*" >> "${OCS_WRAP_LOG:-/dev/null}"
case "$1" in
  --mcp) exec "$OCS_REAL" "$@" ;;
  *)     exec python3 "$OCS_FAKE_INSTANCE" "$@" ;;
esac
