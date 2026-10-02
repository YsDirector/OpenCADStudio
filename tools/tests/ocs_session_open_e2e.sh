#!/bin/sh
# 无头 open E2E：`tools/ocs_session.py` 的 open / query / run 全链路（真二进制 --serve 无头宿主 + 真 MCP）。
#
# 跑法（仓库根目录或任意目录均可）：
#     tools/tests/ocs_session_open_e2e.sh
#
# 覆盖：
#   * open <仓库只读模板的拷贝>（路径含空格/中文/全角括号、带引号）⇒ 打印 document_id / revision / title
#   * query line ⇒ 条数 == 模板真实值（见 EXPECT_LINES，非空）
#   * run LINE 0,0 10,0 ⇒ completed 且 revision 递增
#   * 再 query line ⇒ 条数 +1
#   * 错误路径：open <不存在> ⇒「文件不存在」；open <被别人的活锁占住> ⇒「正被另一个实例打开」并点名 pid
#   * 收尾：删临时目录 + 与基线快照做差集确认无新增 OpenCADStudio 进程 + 打印 PASS/FAIL 与 rc
#
# 安全：图纸一律用「仓库内只读文件的 mktemp -d 拷贝」，绝不用用户正在编辑的那份；
#       ocs_session.py 自带 XDG 隔离（只认自己启动的宿主）；不开 GUI（--new-instance 被桥成 --serve）。
# 手工测试，不接 CI。
#
# 环境变量：
#   OCS_BIN=<真二进制>       缺省按仓库 target/release/OpenCADStudio 推导
#   OCS_E2E_EXPECT_LINES=<n> 模板 line 条数期望（缺省 EXPECT_LINES；模板改了才需要动）
#   OCS_E2E_KEEP=1           保留临时目录（调试）
#   OCS_E2E_TIMEOUT=<秒>     ocs_session.py 的超时（缺省 240）
set -u

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$HERE/../.." && pwd)
TOOL="$ROOT/tools/ocs_session.py"
E2E_DIR="$HERE/ocs_session_open_e2e"
TEMPLATE="$ROOT/crates/ocs_ocsm/bom/明细表模板.dxf"
REAL="${OCS_BIN:-$ROOT/target/release/OpenCADStudio}"
EXPECT_LINES="${OCS_E2E_EXPECT_LINES:-22}"     # 明细表模板.dxf 的 Line 条数（实测值）
KEEP="${OCS_E2E_KEEP:-}"
TIMEOUT_SECS="${OCS_E2E_TIMEOUT:-240}"

FAIL=0
ok()  { echo "  PASS  $1"; }
bad() { echo "  FAIL  $1"; FAIL=$((FAIL + 1)); }

echo "== 0. 前置检查 =="
[ -f "$TOOL" ]     && ok "驱动存在：$TOOL"              || bad "驱动缺失：$TOOL"
[ -f "$TEMPLATE" ] && ok "只读模板存在：$TEMPLATE"      || bad "只读模板缺失：$TEMPLATE"
[ -x "$REAL" ]     && ok "真二进制（无头宿主桥真身）：$REAL" || bad "真二进制不可执行：$REAL"
if [ "${OCS_ALLOW_EXISTING:-}" = "1" ] || [ "${OCS_ALLOW_EXISTING:-}" = "true" ]; then
  bad "OCS_ALLOW_EXISTING 已设：E2E 必须走隔离模式，拒绝运行"
  exit 1
fi
[ "$FAIL" -eq 0 ] || { echo "FAIL 前置检查未过，rc=1"; exit 1; }

BASELINE=$(pgrep -x OpenCADStudio 2>/dev/null | sort -n | tr '\n' ' ')
echo "  基线：跑动前 OpenCADStudio 进程 [${BASELINE:-无}] —— 本次只断言「无新增」，不把它们算作残留"

WORK=$(mktemp -d "${TMPDIR:-/tmp}/ocs-open-e2e.XXXXXX") || { echo "FAIL mktemp 失败，rc=2"; exit 2; }
HOLDER_PID=""
on_signal() {
  [ -n "${HOLDER_PID:-}" ] && kill "$HOLDER_PID" 2>/dev/null
  [ -n "${KEEP:-}" ] || rm -rf "$WORK"
  exit 130
}
trap on_signal INT TERM HUP

mkdir -p "$WORK/noplugins"
COPY1="$WORK/倒角 样例（优化）.dxf"
COPY2="$WORK/被占用 的拷贝.dxf"
cp "$TEMPLATE" "$COPY1"
cp "$TEMPLATE" "$COPY2"
cp "$E2E_DIR/headless_host.py" "$WORK/headless_host.py"
cp "$E2E_DIR/wrap_ocs_bin.sh"  "$WORK/wrap_ocs_bin.sh"
cp "$E2E_DIR/hold_lock.py"     "$WORK/hold_lock.py"
chmod 755 "$WORK/wrap_ocs_bin.sh"
ok "临时目录与图纸拷贝（含空格/中文/括号）：$WORK"

# 手工造一个被占用的编辑锁：活进程 + 真 flock（内容/命名与 src/io/edit_lock.rs 同构）
python3 "$WORK/hold_lock.py" "$COPY2" 600 > "$WORK/holder.out" 2>&1 &
HOLDER_PID=$!
i=0
while [ ! -e "$WORK/.被占用 的拷贝.dxf.ocs.lock" ] && [ "$i" -lt 100 ]; do
  i=$((i + 1)); sleep 0.05
done
if [ -e "$WORK/.被占用 的拷贝.dxf.ocs.lock" ]; then
  ok "被占用的锁已就位：sidecar pid=$HOLDER_PID（flock 持有中）"
else
  bad "锁夹具没起来（$WORK/.被占用 的拷贝.dxf.ocs.lock 不存在）"
fi

export OCS_BIN="$WORK/wrap_ocs_bin.sh"
export OCS_REAL="$REAL"
export OCS_E2E_SERVE_LOG="$WORK/serve.log"
export OCS_PLUGINS_DIR="$WORK/noplugins"     # 空插件目录：E2E 不依赖用户装了什么
unset OCS_ALLOW_EXISTING OCS_KEEP_HOST OCS_SESSION_STATE_DIR OCS_NO_SEED OCS_SEED_FROM 2>/dev/null || true

printf 'open "%s"\nquery line\nrun LINE 0,0 10,0\nquery line\nopen %s\nopen %s\n' \
  "$COPY1" "$WORK/不存在的文件.dxf" "$COPY2" > "$WORK/ops.txt"

echo ""
echo "== 1. 跑 tools/ocs_session.py（隔离 + 无头宿主） =="
timeout "$TIMEOUT_SECS" python3 "$TOOL" < "$WORK/ops.txt" > "$WORK/out.log" 2>&1
RC=$?
echo "  ocs_session 退出码 rc=$RC（日志 $WORK/out.log）"

SESS=$(grep -m1 '^# session=' "$WORK/out.log" || true)
OKLINE=$(grep -m1 '  open OK ' "$WORK/out.log" || true)
RUNLINE=$(grep -m1 "^  run 'LINE 0,0 10,0'" "$WORK/out.log" || true)
MISS=$(grep -m1 -F '文件不存在' "$WORK/out.log" || true)
LOCKERR=$(grep -m1 -F '正被另一个实例打开' "$WORK/out.log" || true)
N1=$(awk '/^    \(/ {n=$0; gsub(/[^0-9]/, "", n); print n; exit}' "$WORK/out.log")
N2=$(awk '/^    \(/ {n=$0; gsub(/[^0-9]/, "", n); last=n} END {print last}' "$WORK/out.log")
DOC=$(printf '%s\n' "$OKLINE"  | sed -n 's/.* doc=\([0-9][0-9]*\) .*/\1/p')
OREV=$(printf '%s\n' "$OKLINE" | sed -n 's/.* rev=\([0-9][0-9]*\) .*/\1/p')
RREV=$(printf '%s\n' "$RUNLINE" | sed -n 's/.* rev=\([0-9][0-9]*\) .*/\1/p')
NOPEN=$(grep -c '  open OK ' "$WORK/out.log" || true)

echo ""
echo "== 2. 断言 =="
[ "$RC" -eq 0 ] && ok "脚本退出码 0" || bad "脚本退出码 $RC"
[ -n "$SESS" ] && ok "会话行：$SESS" || bad "没有 # session= 行"
[ -n "$OKLINE" ] && ok "open 成功行：$OKLINE" || bad "没有 open OK 行"
if printf '%s\n' "$OKLINE" | grep -q "name='倒角 样例（优化）.dxf'"; then
  ok "打开的是目标拷贝（name=倒角 样例（优化）.dxf）"
else
  bad "打开的不是目标拷贝：$OKLINE"
fi
if [ -n "$DOC" ] && [ "$DOC" != "1" ]; then
  ok "document_id 已切到新文档（doc=$DOC，初始 doc=1）"
else
  bad "document_id 没切到新文档（doc=${DOC:-?}）"
fi
TITLE=$(printf '%s\n' "$OKLINE" | sed -n "s/.* title='\([^']*\)'.*/\1/p")
[ -n "$TITLE" ] && ok "title 非空：$TITLE" || bad "title 为空"
if [ "$N1" = "$EXPECT_LINES" ]; then
  ok "open 后 query line 条数 == 模板真实值（$N1）"
else
  bad "open 后 query line 条数 $N1 != 期望 $EXPECT_LINES（模板改了？改 EXPECT_LINES）"
fi
if printf '%s\n' "$RUNLINE" | grep -q 'status=completed'; then
  ok "run LINE 0,0 10,0 成功：$RUNLINE"
else
  bad "run LINE 0,0 10,0 没成功：${RUNLINE:-（无该行）}"
fi
if [ -n "$OREV" ] && [ -n "$RREV" ] && [ "$RREV" -eq "$((OREV + 1))" ]; then
  ok "revision 递增：$OREV → $RREV"
else
  bad "revision 没递增：open rev=${OREV:-?} run rev=${RREV:-?}"
fi
if [ -n "$N1" ] && [ -n "$N2" ] && [ "$N2" -eq "$((N1 + 1))" ]; then
  ok "run 后 query line 条数 +1：$N1 → $N2"
else
  bad "run 后 query line 条数没 +1：${N1:-?} → ${N2:-?}"
fi
[ -n "$MISS" ] && ok "错误路径（不存在）：$MISS" || bad "不存在文件没有被拒绝"
if [ -n "$LOCKERR" ] && printf '%s\n' "$LOCKERR" | grep -qF "pid=$HOLDER_PID"; then
  ok "错误路径（被占用的锁）点名 pid=$HOLDER_PID：$LOCKERR"
else
  bad "被占用的锁没被拒绝或没点名 pid=$HOLDER_PID：${LOCKERR:-（无该行）}"
fi
[ "$NOPEN" -eq 1 ] && ok "全程只有 1 次 open 成功（被锁/不存在的都没打开）" || bad "open 成功次数 $NOPEN != 1"

echo ""
echo "== 3. 收尾 =="
kill "$HOLDER_PID" 2>/dev/null
wait "$HOLDER_PID" 2>/dev/null
rm -f "$WORK/.被占用 的拷贝.dxf.ocs.lock"
sleep 1
CUR=$(pgrep -x OpenCADStudio 2>/dev/null | sort -n | tr '\n' ' ')
NEW=""
for p in $CUR; do
  case " $BASELINE " in *" $p "*) ;; *) NEW="$NEW$p " ;; esac
done
if [ -z "$NEW" ]; then
  ok "无新增 OpenCADStudio 进程（当前 [${CUR:-无}]；基线 [${BASELINE:-无}]）"
else
  bad "发现新增 OpenCADStudio 进程：$NEW（基线 [${BASELINE:-无}]；先杀掉再退出）"
  for p in $NEW; do kill "$p" 2>/dev/null; done
fi
BRIDGE_LEAK=$(pgrep -f "$WORK/headless_host.py" 2>/dev/null | tr '\n' ' ')
if [ -z "$BRIDGE_LEAK" ]; then
  ok "无残留的无头宿主桥进程"
else
  bad "残留无头宿主桥进程：$BRIDGE_LEAK"; for p in $BRIDGE_LEAK; do kill "$p" 2>/dev/null; done
fi
if [ -n "$KEEP" ]; then
  echo "  （保留临时目录 OCS_E2E_KEEP=1：$WORK）"
else
  rm -rf "$WORK"
  if [ -d "$WORK" ]; then
    bad "临时目录删除失败：$WORK"
  else
    ok "临时目录已删除"
  fi
fi

echo
if [ "$FAIL" -eq 0 ]; then
  echo "PASS  ocs_session open E2E（0 失败）  rc=0"
  exit 0
fi
echo "FAIL  ocs_session open E2E（$FAIL 失败）  rc=1"
exit 1
