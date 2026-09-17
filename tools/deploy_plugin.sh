#!/usr/bin/env bash
# 部署 OCSM 插件到 ~/.config/OpenCADStudio/plugins/opencad.ocsm/
#
# 为什么要有这个脚本：`plugin.toml` 里有两个**部署时占位符**，宿主对 API≥4 的插件
# 两道门禁都靠它们，漏填一个插件就被静默拒绝加载（命令行只报一句
# `Plugin built for acadrust @unknown, but this host uses @…` / rustc 不匹配）：
#
#   __RUSTC_VERSION__   ← `rustc --version` 原串（Rust 无稳定 ABI，宿主按编译器卡）
#   __ACADRUST_SOURCE__ ← Cargo.lock 里 `name = "acadrust"` 的 source 串
#                         （形如 git+https://…cadcodec.git?rev=8a28c21#<40位commit>）
#                         上游文档：docs/plugin-architecture.md「acadrust_source is also
#                         required for API v4 and later … reading acadrust_source out of Cargo.lock」
#
# 用法：tools/deploy_plugin.sh [--skip-build]
set -euo pipefail

cd "$(dirname "$0")/.."
export PATH="$PATH:$HOME/.cargo/bin"

PLUGIN_DIR="${OCSM_PLUGIN_DIR:-$HOME/.config/OpenCADStudio/plugins/opencad.ocsm}"

if [[ "${1:-}" != "--skip-build" ]]; then
    echo "==> 编插件（release）"
    cargo build --release -p ocs_ocsm
fi

ACADRUST_SRC=$(python3 - <<'PY'
import re, sys, pathlib
lock = pathlib.Path('Cargo.lock').read_text(encoding='utf-8')
m = re.search(r'name = "acadrust"\nversion = "[^"]+"\nsource = "([^"]+)"', lock)
if not m:
    sys.exit('Cargo.lock 里找不到 acadrust 的 source')
src = m.group(1)
hash_part = src.rsplit('#', 1)[-1]
if len(hash_part) != 40 or any(c not in '0123456789abcdef' for c in hash_part):
    sys.exit(f'acadrust source 末尾不是 40 位 commit：{src}')
print(src)
PY
)
RUSTC_VER=$(rustc --version)

mkdir -p "$PLUGIN_DIR/handbook" "$PLUGIN_DIR/frame"

echo "==> 装 .so + plugin.toml（rustc=${RUSTC_VER}；acadrust=…${ACADRUST_SRC: -40}）"
cp target/release/libocs_ocsm.so "$PLUGIN_DIR/"
sed -e "s|__RUSTC_VERSION__|${RUSTC_VER}|" \
    -e "s|__ACADRUST_SOURCE__|${ACADRUST_SRC}|" \
    crates/ocs_ocsm/plugin.toml > "$PLUGIN_DIR/plugin.toml"
cp crates/ocs_ocsm/handbook/*.md "$PLUGIN_DIR/handbook/"

# 自检：两个占位符都已替换（别让宿主用一句含糊的话拒绝加载）
if grep -q "__RUSTC_VERSION__\|__ACADRUST_SOURCE__" "$PLUGIN_DIR/plugin.toml"; then
    echo "✗ plugin.toml 仍有未替换的占位符" >&2
    exit 1
fi
grep -q '^acadrust_source = "git+' "$PLUGIN_DIR/plugin.toml" || {
    echo "✗ plugin.toml 的 acadrust_source 形状不对" >&2
    exit 1
}
echo "✓ 部署完成：$PLUGIN_DIR"
echo "  重启 OCS 后命令行应出现：Loaded plugin: OCSMechanical 机械工具包 (opencad.ocsm 0.2.0)"
