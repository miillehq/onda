#!/usr/bin/env bash
# Linux counterpart of build.ps1: release speech workers, contracts and notices, then the Tauri
# bundles from tauri.linux.conf.json (deb and AppImage by default).
#   scripts/build.sh [--with-vulkan] [--bundles deb|appimage|deb,appimage]
set -euo pipefail
repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository"
prepare=(--release)
bundles=deb,appimage
while [[ $# -gt 0 ]]; do
  case "$1" in
    --with-vulkan) prepare+=(--with-vulkan); shift ;;
    --bundles) bundles="$2"; shift 2 ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
  esac
done
scripts/prepare.sh "${prepare[@]}"
npm run contracts
node scripts/generate-notices.mjs
overlay=()
if [[ " ${prepare[*]} " == *" --with-vulkan "* ]]; then
  mkdir -p .artifacts
  printf '%s\n' '{"bundle":{"externalBin":["binaries/onda-asr-worker","binaries/onda-asr-avx2-worker","binaries/onda-asr-vulkan-worker"]}}' > .artifacts/linux-bundle-config.json
  overlay=(--config .artifacts/linux-bundle-config.json)
fi
npm run tauri build -- --bundles "$bundles" "${overlay[@]}"
