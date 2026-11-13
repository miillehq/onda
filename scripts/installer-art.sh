#!/usr/bin/env bash
# Rebuilds the NSIS installer bitmaps from the brand masters in src-tauri/icons/installer/source
# (copied unchanged from the Onda brand package). Needs rsvg-convert and ImageMagick.
#   header.bmp   150×57  installer and uninstaller page header: black lockup on white
#   sidebar.bmp  164×314 Welcome and Finish pages: dark app icon and white wordmark on the
#                        graphite tile (#262628 → #0F0F10 → #040404)
# NSIS needs uncompressed 24-bit BMP files without an alpha channel.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../src-tauri/icons/installer" && pwd)"
source="$here/source"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Lockup 22 px tall is 102 px wide (brand minimum 100 px), with more than x clear around it.
rsvg-convert --height 22 "$source/onda-lockup-black.svg" -o "$work/lockup.png"
magick -size 150x57 xc:white "$work/lockup.png" -gravity west -geometry +20+0 -composite \
  -alpha off -type TrueColor "BMP3:$here/header.bmp"

# The dimensional icon at 88 px (well above its 32 px minimum) and the wordmark 74 px wide.
magick "$source/onda-icon-dark-512px.png" -filter Lanczos -resize 88x88 "$work/icon.png"
rsvg-convert --width 74 "$source/onda-wordmark-white.svg" -o "$work/wordmark.png"
magick -size 164x173 "gradient:#262628-#0F0F10" -size 164x141 "gradient:#0F0F10-#040404" -append \
  "$work/icon.png" -gravity north -geometry +0+72 -composite \
  "$work/wordmark.png" -gravity north -geometry +0+178 -composite \
  -alpha off -type TrueColor "BMP3:$here/sidebar.bmp"
