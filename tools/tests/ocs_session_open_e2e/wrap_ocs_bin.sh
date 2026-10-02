#!/bin/sh
# OCS_BIN 替身（无头 open E2E 用）：
#   --new-instance（GUI 宿主）→ headless_host.py（把真二进制桥成 --serve 无头宿主）
#   其余参数（--mcp）        → 真二进制原样
# 环境变量：OCS_REAL = 真二进制路径（由 ocs_session_open_e2e.sh 导出）
# ★ 仓库里这份保持 644：E2E 脚本会把它和 headless_host.py 一起拷进临时目录再 chmod 755。
OCS_E2E_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
case "${1:-}" in
  --new-instance)
    shift
    exec python3 "$OCS_E2E_DIR/headless_host.py" "$@"
    ;;
  *)
    exec "${OCS_REAL:?OCS_REAL 未设置}" "$@"
    ;;
esac
