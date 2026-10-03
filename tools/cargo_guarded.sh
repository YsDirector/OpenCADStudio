#!/usr/bin/env bash
# =============================================================================
# tools/cargo_guarded.sh —— 本机受护栏的 cargo 包装器（唯一允许的构建入口）
#
# 用法:
#   tools/cargo_guarded.sh <cargo 子命令与参数…>
#     tools/cargo_guarded.sh check -p ocs_ocsm
#     tools/cargo_guarded.sh test --workspace --locked
#     tools/cargo_guarded.sh --no-cap check -p ocs_ocsm    # 逃生口：去掉内存封顶
#
# 四条规矩（为什么这样包）:
#   ① 固定 CARGO_BUILD_JOBS=4 + CARGO_INCREMENTAL=0
#      —— 并发链接器 rust-lld 是内存真凶；本机 27GiB 内存 / nproc=16，
#         并发一高会把整台机器搞死（真实发生过）。
#   ② 默认套 systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0
#      —— 交换是 zram（住内存）不能当泄压阀；硬封顶 + 禁换出，
#         构建 OOM 只杀这个 scope，不杀宿主机。
#      （--scope 里 PATH 是干净的，脚本显式把 $HOME/.cargo/bin 等塞回去。）
#   ③ 挂载守卫: target 落在 /mnt/ 下时，要求
#         /mnt/ocsm-cache/.ocsm-mount-ok 存在 且 目标盘确实挂载着，
#         否则明确报错拒绝构建 —— 防“盘没挂上、产物写进根分区”。
#   ④ 单构建锁: flock -n ~/.cache/ocsm-cargo.lock（放 ~/.cache，不放 /tmp —— tmpfs）。
#      已有构建在跑 ⇒ 立刻报错退出，不排队（排队会让人以为卡住）。
#
# 逃生口:
#   --no-cap 或 CARGO_GUARD_NO_CAP=1  去掉 systemd-run 那层（应急）。
#   本机 systemd-run 不可用 ⇒ 自动降级为无封顶运行，并打印警告。
#
# 环境变量:
#   CARGO_GUARD_MEM_MAX=16G   内存封顶（仅 systemd-run 层）
#   CARGO_GUARD_LOCK=…        锁文件路径（默认 ~/.cache/ocsm-cargo.lock）
#   CARGO_GUARD_CARGO=…       cargo 可执行文件（默认 $HOME/.cargo/bin/cargo）
#
# 退出码: 0/cargo 原样; 69=挂载守卫拒绝; 75=已有构建占用锁; 127=找不到 cargo。
# =============================================================================
set -u

GUARD_PATH="$HOME/.cargo/bin:/usr/local/bin:/usr/bin:/bin"
MEM_MAX="${CARGO_GUARD_MEM_MAX:-16G}"
MARKER="/mnt/ocsm-cache/.ocsm-mount-ok"

usage() {
  cat <<'USAGE'
用法: tools/cargo_guarded.sh [--no-cap] <cargo 子命令与参数…>
  tools/cargo_guarded.sh check -p ocs_ocsm
  tools/cargo_guarded.sh test --workspace --locked
  --no-cap   应急：去掉 systemd-run 内存封顶（systemd-run 不可用时自动降级并警告）
说明见脚本头部注释。
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

NO_CAP=0
[[ "${CARGO_GUARD_NO_CAP:-0}" == "1" ]] && NO_CAP=1
CARGO_ARGS=()
for arg in "$@"; do
  case "$arg" in
    --no-cap) NO_CAP=1 ;;
    *) CARGO_ARGS+=("$arg") ;;
  esac
done
if (( ${#CARGO_ARGS[@]} == 0 )); then
  usage >&2
  exit 2
fi

# ---- cargo 可执行文件 ------------------------------------------------------
cargo_bin="${CARGO_GUARD_CARGO:-$HOME/.cargo/bin/cargo}"
if [[ ! -x "$cargo_bin" ]]; then
  cargo_bin="$(command -v cargo 2>/dev/null || true)"
fi
if [[ -z "$cargo_bin" || ! -x "$cargo_bin" ]]; then
  echo "✗ cargo_guarded: 找不到 cargo（试过 \$HOME/.cargo/bin/cargo 与 PATH）" >&2
  exit 127
fi

# ---- 挂载守卫（规矩③） -----------------------------------------------------
repo_root="$(git rev-parse --show-toplevel 2>/dev/null || true)"
[[ -n "$repo_root" ]] || repo_root="$PWD"
target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"
for ((i = 0; i < ${#CARGO_ARGS[@]}; i++)); do
  case "${CARGO_ARGS[i]}" in
    --target-dir=*) target_dir="${CARGO_ARGS[i]#--target-dir=}" ;;
    --target-dir)
      if (( i + 1 < ${#CARGO_ARGS[@]} )); then target_dir="${CARGO_ARGS[i + 1]}"; fi
      ;;
  esac
done
target_real="$(readlink -f -- "$target_dir" 2>/dev/null || true)"
[[ -n "$target_real" ]] || target_real="$target_dir"
repo_real="$(readlink -f -- "$repo_root" 2>/dev/null || true)"
[[ -n "$repo_real" ]] || repo_real="$repo_root"

guard_refuse() {
  echo "✗ cargo_guarded 挂载守卫拒绝构建: $1" >&2
  echo "  target=$target_real" >&2
  echo "  修好挂载（并确认 $MARKER 在）后再试；逃生口 --no-cap 只去掉内存封顶，不绕过本守卫。" >&2
  exit 69
}

case "$target_real" in
  /mnt/*)
    [[ -e "$MARKER" ]] || guard_refuse "target 在 /mnt/ 下，但 $MARKER 不存在（数据盘没挂上？）"
    if [[ ! -d "$repo_real" ]]; then
      guard_refuse "仓库目录 $repo_real 不存在（数据盘没挂上？）"
    fi
    probe="$target_real"
    while [[ ! -d "$probe" && "$probe" != "/" ]]; do
      probe="$(dirname -- "$probe")"
    done
    mnt="$(stat -c '%m' -- "$probe" 2>/dev/null || true)"
    if [[ -z "$mnt" || "$mnt" == "/" ]]; then
      guard_refuse "目标盘未挂载（target 会落到根分区；探测点 $probe 的挂载点=${mnt:-?}）"
    fi
    echo "[cargo_guarded] 挂载守卫通过: target=$target_real 位于 $mnt"
    ;;
esac

# ---- 单构建锁（规矩④） -----------------------------------------------------
lock_file="${CARGO_GUARD_LOCK:-$HOME/.cache/ocsm-cargo.lock}"
mkdir -p -- "$(dirname -- "$lock_file")" || {
  echo "✗ cargo_guarded: 无法创建锁目录 $(dirname -- "$lock_file")" >&2
  exit 75
}
exec 9>>"$lock_file" || {
  echo "✗ cargo_guarded: 无法打开锁文件 $lock_file" >&2
  exit 75
}
if ! flock -n 9; then
  echo "✗ cargo_guarded: 已有构建在跑（锁 $lock_file 被占）——不排队，直接退出。" >&2
  echo "  等它结束再来；若确信没有游离构建，检查锁文件内容里的 pid。" >&2
  exit 75
fi
printf 'pid=%s at=%s cargo %s\n' "$$" "$(date -Is 2>/dev/null || date)" "${CARGO_ARGS[*]}" >&9 2>/dev/null || true
echo "[cargo_guarded] 已获构建锁 $lock_file"

# ---- 固定 jobs / incremental（规矩①） --------------------------------------
export CARGO_BUILD_JOBS=4
export CARGO_INCREMENTAL=0

# ---- 运行（规矩②；--no-cap 或 systemd-run 不可用则降级） -------------------
run_plain() {
  env PATH="$GUARD_PATH" "$cargo_bin" "${CARGO_ARGS[@]}"
}
run_capped() {
  systemd-run --user --scope -q -p "MemoryMax=$MEM_MAX" -p MemorySwapMax=0 -- \
    env PATH="$GUARD_PATH" "$cargo_bin" "${CARGO_ARGS[@]}"
}

rc=0
if ((NO_CAP)); then
  echo "[cargo_guarded] ⚠ --no-cap：跳过 systemd-run 内存封顶（应急模式）" >&2
  run_plain || rc=$?
elif ! command -v systemd-run >/dev/null 2>&1; then
  echo "[cargo_guarded] ⚠ systemd-run 不存在：自动降级为无内存封顶运行（请尽快修复）" >&2
  run_plain || rc=$?
elif ! systemd-run --user --scope -q -p "MemoryMax=$MEM_MAX" -p MemorySwapMax=0 -- true >/dev/null 2>&1; then
  echo "[cargo_guarded] ⚠ systemd-run --user --scope 探测失败：自动降级为无内存封顶运行（请尽快修复）" >&2
  run_plain || rc=$?
else
  run_capped || rc=$?
fi

echo "[cargo_guarded] cargo ${CARGO_ARGS[*]} -> rc=$rc (jobs=4, incremental=0)"
exit "$rc"
