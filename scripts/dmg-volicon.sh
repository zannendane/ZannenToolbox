#!/usr/bin/env bash
# DMG 卷图标后处理：把卷图标替换为安装包（📦）变体 icns。
# 用法: scripts/dmg-volicon.sh <dmg路径>
# 依赖: macOS（hdiutil / SetFile / iconutil 体系）。CI release 在 DMG 产出后调用。
set -euo pipefail

DMG="${1:?用法: dmg-volicon.sh <dmg路径>}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VOLICON_SRC="$ROOT/app-shell/src-tauri/installer/installer-volume.icns"

# 由安装包变体母版生成 icns（幂等）
if [[ ! -f "$VOLICON_SRC" ]]; then
  mkdir -p /tmp/zannen-volicon.iconset
  for s in 16 32 128 256 512; do
    sips -z $s $s "$ROOT/branding/installer-dark.png" --out "/tmp/zannen-volicon.iconset/icon_${s}x${s}.png" >/dev/null
    s2=$((s * 2))
    if (( s2 <= 1024 )); then
      sips -z $s2 $s2 "$ROOT/branding/installer-dark.png" --out "/tmp/zannen-volicon.iconset/icon_${s}x${s}@2x.png" >/dev/null
    fi
  done
  iconutil -c icns /tmp/zannen-volicon.iconset -o "$VOLICON_SRC"
  echo "生成卷图标: $VOLICON_SRC"
fi

RW_DMG="${DMG%.dmg}.rw.dmg"
hdiutil convert "$DMG" -format UDRW -o "$RW_DMG" >/dev/null
ATTACH_OUT="$(hdiutil attach -readwrite "$RW_DMG")"
VOL="$(echo "$ATTACH_OUT" | tail -1 | awk '{for(i=3;i<=NF;i++) printf "%s%s", $i, (i<NF?" ":"")}')"
echo "挂载于: $VOL"
cp "$VOLICON_SRC" "$VOL/.VolumeIcon.icns"
SetFile -a C "$VOL"
hdiutil detach "$VOL" >/dev/null
hdiutil convert "$RW_DMG" -format UDZO -o "$DMG" >/dev/null
rm -f "$RW_DMG"
echo "卷图标已替换: $DMG"
