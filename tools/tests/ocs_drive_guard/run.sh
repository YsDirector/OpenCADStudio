#!/bin/sh
# 一键跑：ocs_drive「默认不猜目标实例」守卫回归测试（零 GUI、零 cargo、全程临时目录）。
#
# 用法：
#   tools/tests/ocs_drive_guard/run.sh
#   OCS_GUARD_KEEP=1 tools/tests/ocs_drive_guard/run.sh   # 保留临时工作目录，便于事后看日志
#   OCS_GUARD_WORK=/tmp/xxx tools/tests/ocs_drive_guard/run.sh   # 指定工作目录（默认 mktemp -d）
#
# 退出码：0 = 没有失败断言（SKIP 不计入通过）；1 = 有断言失败。
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PATH="$PATH:$HOME/.cargo/bin"
LC_ALL=C LANG=C LANGUAGE=C
export PATH LC_ALL LANG LANGUAGE
exec python3 "$HERE/run_tests.py" "$@"
