#!/usr/bin/env bash
# Called inside the named disposable distribution image, with candidate bundles mounted read-only.
set -euo pipefail
case "$DISTRIBUTION" in
  debian)
    apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq /packages/deb/*.deb xvfb xauth procps dbus-daemon dbus-x11 >/dev/null
    root=/usr/bin
    launch=(/usr/bin/onda)
    ;;
  fedora)
    dnf install -y -q webkit2gtk4.1 gtk3 alsa-lib xorg-x11-server-Xvfb dbus-daemon procps-ng util-linux >/dev/null
    ;;
  arch)
    pacman -Syu --noconfirm --needed --quiet webkit2gtk-4.1 gtk3 alsa-lib xorg-server-xvfb dbus procps-ng util-linux >/dev/null
    ;;
  *) echo "Unknown distribution $DISTRIBUTION" >&2; exit 1 ;;
esac

if [[ -z "${root:-}" ]]; then
  cd /tmp
  cp /packages/appimage/*.AppImage onda.AppImage
  chmod +x onda.AppImage
  ./onda.AppImage --appimage-extract >/dev/null
  root=/tmp/squashfs-root/usr/bin
  launch=(/tmp/onda.AppImage --appimage-extract-and-run)
fi
for name in onda onda-input onda-asr-worker onda-asr-avx2-worker; do
  [[ -x "$root/$name" ]] || { echo "Missing binary $name" >&2; exit 1; }
  if [[ "$DISTRIBUTION" == debian ]]; then
    libraries=$(ldd "$root/$name")
  else
    libraries=$(LD_LIBRARY_PATH=/tmp/squashfs-root/usr/lib:/tmp/squashfs-root/usr/lib64 ldd "$root/$name")
  fi
  if [[ "$libraries" == *"not found"* ]]; then echo "$libraries" >&2; exit 1; fi
done

useradd --create-home --shell /bin/bash onda-fixture
runtime=$(mktemp -d)
chown onda-fixture "$runtime"
chmod 700 "$runtime"
Xvfb :99 -screen 0 1120x780x24 -nolisten tcp > /tmp/onda-xvfb.log 2>&1 &
display_server=$!
trap 'kill "$display_server" 2>/dev/null || true' EXIT
for _ in {1..50}; do [[ -S /tmp/.X11-unix/X99 ]] && break; sleep 0.1; done
[[ -S /tmp/.X11-unix/X99 ]] || { cat /tmp/onda-xvfb.log; exit 1; }
runuser -u onda-fixture -- env DISPLAY=:99 XDG_RUNTIME_DIR="$runtime" NO_AT_BRIDGE=1 \
  bash /onda-check/installer-launch-linux.sh "${launch[@]}"
echo "Dependencies resolved and the candidate launched on $DISTRIBUTION."
