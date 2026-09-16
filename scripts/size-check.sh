#!/usr/bin/env bash
# 产物体积检查（对照 docs/ARCHITECTURE.md 的预算）。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

fmt_kb() { awk -v b="$1" 'BEGIN { printf "%.1f KB", b/1024 }'; }
fmt_mb() { awk -v b="$1" 'BEGIN { printf "%.2f MB", b/1048576 }'; }

echo "== 壳前端 dist =="
if [[ -d "$ROOT/app-shell/dist" ]]; then
  TOTAL=$(find "$ROOT/app-shell/dist" -type f -exec stat -f%z {} + | awk '{s+=$1} END {print s}')
  find "$ROOT/app-shell/dist" -type f | sort | while read -r f; do
    echo "  $(fmt_kb "$(stat -f%z "$f")")  ${f#"$ROOT"/}"
  done
  echo "  小计: $(fmt_kb "$TOTAL")"
else
  echo "  (未构建)"
fi

echo "== 插件 zannen.debugger 装配产物 =="
if [[ -d "$ROOT/app-shell/src-tauri/plugins" ]]; then
  TOTAL=$(find "$ROOT/app-shell/src-tauri/plugins" -type f -exec stat -f%z {} + | awk '{s+=$1} END {print s}')
  find "$ROOT/app-shell/src-tauri/plugins" -type f | sort | while read -r f; do
    SIZE=$(stat -f%z "$f")
    echo "  $(fmt_kb "$SIZE")  ${f#"$ROOT"/}"
  done
  echo "  小计: $(fmt_mb "$TOTAL")"
else
  echo "  (未装配)"
fi

echo "== Rust release 产物 =="
for f in "$ROOT/target/release/zannen-toolbox" "$ROOT/target/release/libzannen_debugger.dylib"; do
  if [[ -f "$f" ]]; then
    echo "  $(fmt_mb "$(stat -f%z "$f")")  ${f#"$ROOT"/}"
  fi
done

echo
echo "预算: 安装包 ≤ 25MB · 壳前端 ≤ 1MB(gzip) · 插件前端 ≤ 1.5MB(gzip)"
