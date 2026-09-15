#!/usr/bin/env bash
# 构建并安装「通用网页面板」插件到 OpenCADStudio 插件目录。
#
#   bash crates/ocs_webpanel/install.sh
#
# 产物（三者必须同目录）：
#   libocs_webpanel.so   插件本体（宿主加载）
#   ocs-webpanel-host    面板宿主进程（GTK3 + WebKitGTK，负责网页与 X11 停靠）
#   plugin.toml          宿主启动时读的元数据（rustc_version 必须与构建它的一致）
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST="${OCS_PLUGIN_DIR:-$HOME/.config/OpenCADStudio/plugins/opencad.webpanel}"
cd "$REPO"

echo "== 构建 ocs_webpanel（release）=="
source "$HOME/.cargo/env" 2>/dev/null || true
cargo build --release -p ocs_webpanel

LIB="$REPO/target/release/libocs_webpanel.so"
BIN="$REPO/target/release/ocs-webpanel-host"
[ -f "$LIB" ] || { echo "缺少 $LIB"; exit 1; }
[ -f "$BIN" ] || { echo "缺少 $BIN"; exit 1; }

echo "== 安装到 $DEST =="
mkdir -p "$DEST"
cp "$LIB" "$DEST/libocs_webpanel.so"
cp "$BIN" "$DEST/ocs-webpanel-host"
chmod +x "$DEST/ocs-webpanel-host"

# rustc_version 必须是构建这个 .so 的编译器版本：宿主 v2026.36+ 会用它做闸门。
RUSTC_VERSION="$(rustc --version)"
cat > "$DEST/plugin.toml" <<EOF
# 宿主加载时读取。与 src/lib.rs 里的 MANIFEST 保持一致。
[plugin]
id = "opencad.webpanel"
name = "Web Panel 网页面板"
version = "0.1.0"
description = "把任意网页（pi-web / opencode / 本地服务）停靠进 OCS 侧边栏（通用，与 OCSM 无关）"

[opencad]
api_version = 5
rustc_version = "$RUSTC_VERSION"
ribbon_order = 200
command_prefixes = ["WEBPANEL", "WP", "WEBPANELOFF"]
EOF

echo "== 完成 =="
ls -la "$DEST"
echo
echo "用法（OCS 里敲命令）："
echo "  WEBPANEL http://127.0.0.1:30141 right   # 打开 pi-web 到右侧栏"
echo "  WEBPANEL OFF                            # 关闭"
echo
echo "注意：要让网页真正嵌进窗口（而不是独立窗口），OCS 必须跑在 X11/XWayland 上："
echo "  env -u WAYLAND_DISPLAY ./target/release/OpenCADStudio"
