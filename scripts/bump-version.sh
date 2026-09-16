#!/usr/bin/env bash
# 版本递增工具（版本治理见 docs/ARCHITECTURE.md §10）：
#
#   scripts/bump-version.sh shell X.Y.Z      壳产品版本：tauri.conf.json、
#       app-shell/package.json、src-tauri/Cargo.toml、根 package.json，
#       以及随壳的内部实现（crates/*/Cargo.toml、packages/plugin-sdk/package.json）
#   scripts/bump-version.sh plugin <name> X.Y.Z
#       单个插件独立版本：plugins/<name>/{Cargo.toml,plugin.toml,frontend/package.json}
#       与装配副本 app-shell/src-tauri/plugins/<id>/plugin.toml
#
# 插件版本只在自身内容变化时递增，与壳版本解耦。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

mode="${1:-}"
[[ "$mode" == "shell" || "$mode" == "plugin" ]] || { echo "usage: bump-version.sh shell X.Y.Z | bump-version.sh plugin <name> X.Y.Z" >&2; exit 1; }

# 读取某文件的当前版本号（Cargo.toml/plugin.toml 的 ^version 或 package.json 的 "version"）
current_version() {
  grep -m1 -E '^version = "|^  "version": "' "$1" | sed -E 's/.*"([0-9]+\.[0-9]+\.[0-9]+)".*/\1/'
}

# 原地替换版本号
bump_file() {
  local file="$1" from="$2" to="$3"
  [[ -f "$file" ]] || { echo "  skip (missing): $file"; return; }
  sed -i '' \
    -e "s/^version = \"$from\"/version = \"$to\"/" \
    -e "s/^  \"version\": \"$from\"/  \"version\": \"$to\"/" \
    -e "s/^    \"version\": \"$from\"/    \"version\": \"$to\"/" \
    "$file"
}

report_leftovers() {
  local from="$1"; shift
  local left=0
  for f in "$@"; do
    [[ -f "$f" ]] || continue
    if grep -q "\"$from\"" "$f"; then
      echo "  !! leftover $from in $f"
      left=1
    fi
  done
  return $left
}

case "$mode" in
  shell)
    to="${2:-}"
    [[ -n "$to" ]] || { echo "missing target version" >&2; exit 1; }
    files=(
      Cargo.toml
      package.json
      app-shell/package.json
      app-shell/src-tauri/Cargo.toml
      app-shell/src-tauri/tauri.conf.json
      crates/zannen-core/Cargo.toml
      crates/zannen-plugin-api/Cargo.toml
      packages/plugin-sdk/package.json
    )
    # 以 src-tauri Cargo.toml 为基准取当前版本
    from="$(current_version app-shell/src-tauri/Cargo.toml)"
    [[ -n "$from" ]] || { echo "无法读取当前壳版本"; exit 1; }
    [[ "$from" != "$to" ]] || { echo "已是 ${to}，无需变更"; exit 0; }
    for f in "${files[@]}"; do bump_file "$f" "$from" "$to"; done
    # 根 Cargo.toml 是 workspace 清单，可能无 version 字段——tauri.conf.json 单独保险
    sed -i '' "s/\"version\": \"$from\"/\"version\": \"$to\"/" app-shell/src-tauri/tauri.conf.json
    report_leftovers "$from" "${files[@]}" || true
    echo "shell: $from -> $to"
    ;;
  plugin)
    name="${2:-}"
    to="${3:-}"
    [[ -n "$name" && -n "$to" ]] || { echo "missing plugin name or target version" >&2; exit 1; }
    id="$(grep -m1 '^id = ' "plugins/$name/plugin.toml" | sed -E 's/id = "(.*)"/\1/')"
    files=(
      "plugins/$name/Cargo.toml"
      "plugins/$name/plugin.toml"
      "plugins/$name/frontend/package.json"
      "app-shell/src-tauri/plugins/$id/plugin.toml"
    )
    from="$(current_version "plugins/$name/plugin.toml")"
    [[ -n "$from" ]] || { echo "无法读取插件 $name 当前版本"; exit 1; }
    [[ "$from" != "$to" ]] || { echo "已是 ${to}，无需变更"; exit 0; }
    for f in "${files[@]}"; do bump_file "$f" "$from" "$to"; done
    report_leftovers "$from" "${files[@]}" || true
    echo "plugin ${name} (${id}): ${from} -> ${to}（dylib 需重新构建装配）"
    ;;
  *)
    echo "未知模式: $mode" >&2
    exit 1
    ;;
esac
