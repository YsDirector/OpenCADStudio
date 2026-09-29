#!/bin/sh
# OCS_BIN 替身（测试夹具；run_tests.py 会把它拷进临时目录再 chmod 755，不动仓库里这份的模式位）：
#   --mcp             → 真二进制（真 MCP 服务器 + 真描述符发现 / hello 握手链路）
#   --new-instance    → 伪造宿主（真宿主是 GUI，会开窗上屏，本测试禁止上屏）
# 环境变量（由 run_tests.py 的 env_for() 提供）：
#   OCS_REAL       真二进制路径（target/release/OpenCADStudio）
#   OCS_FAKE_HOST  fixtures/fake_ocs_host.py
#   OCS_WRAP_LOG   收到 argv 的日志（用来判定脚本有没有自己起宿主）
echo "argv=$*" >> "${OCS_WRAP_LOG:-/dev/null}"
case "$1" in
  --mcp) exec "$OCS_REAL" "$@" ;;
  *)     exec python3 "$OCS_FAKE_HOST" "$@" ;;
esac
