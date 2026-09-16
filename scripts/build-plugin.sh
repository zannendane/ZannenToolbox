#!/usr/bin/env bash
# 装配插件到壳的 plugins/ 目录。
# 用法: scripts/build-plugin.sh <plugin目录名> [--release]
# 例:   scripts/build-plugin.sh zannen-debugger
set -euo pipefail

# cargo 可能不在非交互 shell 的 PATH 中
export PATH="$HOME/.cargo/bin:$PATH"

PLUGIN_DIR_NAME="${1:?用法: build-plugin.sh <plugin目录名> [--release]}"
PROFILE_FLAG=""
CARGO_PROFILE="debug"
if [[ "${2:-}" == "--release" ]]; then
  PROFILE_FLAG="--release"
  CARGO_PROFILE="release"
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/plugins/$PLUGIN_DIR_NAME"
TOML_FILE="$SRC/plugin.toml"

[[ -f "$TOML_FILE" ]] || { echo "缺少 $TOML_FILE" >&2; exit 1; }

# 从 plugin.toml 提取 id / version / backend.name（格式约定严格，见 docs/PLUGIN-ABI.md）
PLUGIN_ID="$(grep -E '^id[[:space:]]*=' "$TOML_FILE" | head -1 | sed -E 's/^id[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/')"
PLUGIN_VERSION="$(grep -E '^version[[:space:]]*=' "$TOML_FILE" | head -1 | sed -E 's/^version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/')"
BACKEND_NAME="$(awk '/^\[backend\]/{f=1;next} /^\[/{f=0} f&&/^name/{print}' "$TOML_FILE" | sed -E 's/^name[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/')"

[[ -n "$PLUGIN_ID" && -n "$BACKEND_NAME" ]] || { echo "plugin.toml 缺少 id 或 [backend].name" >&2; exit 1; }
[[ -n "$PLUGIN_VERSION" ]] || { echo "plugin.toml 缺少 version" >&2; exit 1; }

# 平台动态库文件名 + 产物命名（{os}-{arch}，与 shell_info.target 一致）
case "$(uname -s)" in
  Darwin) LIB_FILE="lib${BACKEND_NAME}.dylib"; OS_NAME="macos" ;;
  Linux)  LIB_FILE="lib${BACKEND_NAME}.so";    OS_NAME="linux" ;;
  MINGW*|MSYS*|CYGWIN*) LIB_FILE="${BACKEND_NAME}.dll"; OS_NAME="windows" ;;
  *) echo "未知平台: $(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  arm64|aarch64) ARCH_NAME="aarch64" ;;
  x86_64|amd64)  ARCH_NAME="x86_64" ;;
  *) echo "未知架构: $(uname -m)" >&2; exit 1 ;;
esac

CARGO_PKG="${BACKEND_NAME//_/-}"
echo "==> 构建插件后端 $CARGO_PKG ($CARGO_PROFILE)"
( cd "$ROOT" && cargo build -p "$CARGO_PKG" $PROFILE_FLAG )

if [[ -d "$SRC/frontend" ]]; then
  echo "==> 构建插件前端"
  ( cd "$SRC/frontend" && pnpm build )
fi

OUT="$ROOT/app-shell/src-tauri/plugins/$PLUGIN_ID"
echo "==> 装配到 $OUT"
rm -rf "$OUT"
mkdir -p "$OUT"
cp "$TOML_FILE" "$OUT/plugin.toml"
cp "$ROOT/target/$CARGO_PROFILE/$LIB_FILE" "$OUT/$LIB_FILE"
# 供文档/审计参考的数据文件原样带上
for f in devices.json status_rules.json; do
  [[ -f "$SRC/$f" ]] && cp "$SRC/$f" "$OUT/$f" || true
done
if [[ -d "$SRC/frontend/dist" ]]; then
  mkdir -p "$OUT/frontend"
  cp -R "$SRC/frontend/dist" "$OUT/frontend/dist"
fi

echo "==> 完成: $PLUGIN_ID"
ls -la "$OUT"

# 打包并签名可分发产物（.znplugin + .sig；签名密钥见 scripts/sign-plugin.mjs 头部注释）
PKG_OUT="$ROOT/dist/plugins/${PLUGIN_ID}-${PLUGIN_VERSION}-${OS_NAME}-${ARCH_NAME}.znplugin"
echo "==> 打包并签名: $PKG_OUT"
node "$ROOT/scripts/sign-plugin.mjs" "$OUT" "$PKG_OUT"
echo "==> 产物: $PKG_OUT (+ .sig)"
