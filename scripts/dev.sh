#!/usr/bin/env bash
# 开发模式：装配插件 → tauri dev（合并 dev 配置放宽 CSP 以支持 Vite HMR）。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

"$ROOT/scripts/build-plugin.sh" zannen-debugger

cd "$ROOT/app-shell"
exec pnpm exec tauri dev --features mock -c src-tauri/tauri.dev.conf.json
