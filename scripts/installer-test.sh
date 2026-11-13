#!/usr/bin/env bash
# L7 on Linux and macOS: install the release bundle the way a user would, launch the installed
# app, check that it is still running after six seconds, quit it, and remove it.
#   scripts/installer-test.sh        (after scripts/build.sh)
set -euo pipefail
repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bundle="$repository/target/release/bundle"
log="${RUNNER_TEMP:-/tmp}/onda-installer-launch.log"

ok() { echo "ok: $*"; }
fail() { echo "FAIL: $*" >&2; exit 1; }

# $1: the command that starts the installed app. Only processes it started are stopped.
launch() {
  bash -c "$1" >"$log" 2>&1 &
  local starter=$!
  sleep 6
  if pgrep -x onda >/dev/null; then
    ok "the installed app is running after 6 s ($1)"
    pkill -x onda || true
    sleep 2
  else
    cat "$log" || true
    fail "the installed app did not keep running ($1)"
  fi
  wait "$starter" 2>/dev/null || true
}

case "$(uname -s)" in
  Linux)
    export DISPLAY="${DISPLAY:-:99}"
    deb=$(ls "$bundle"/deb/*.deb 2>/dev/null | head -1); [ -f "$deb" ] || fail "no .deb in $bundle/deb"
    sudo DEBIAN_FRONTEND=noninteractive apt-get install -y "$deb" >/dev/null; ok "installed $(basename "$deb")"
    command -v onda >/dev/null || fail "onda is not on PATH after installing the .deb"
    for sidecar in onda-input onda-asr-worker onda-asr-avx2-worker; do
      dpkg -L onda | grep -q "/$sidecar\$" || fail "the package does not install $sidecar"
    done
    ok "the package installs the shortcut helper and both speech workers"
    launch "dbus-run-session -- onda"
    sudo apt-get remove -y onda >/dev/null; ok "removed the .deb"
    if command -v onda >/dev/null; then fail "onda is still on PATH after removal"; fi
    appimage=$(ls "$bundle"/appimage/*.AppImage 2>/dev/null | head -1); [ -f "$appimage" ] || fail "no AppImage in $bundle/appimage"
    chmod +x "$appimage"
    launch "dbus-run-session -- '$appimage' --appimage-extract-and-run"
    ;;
  Darwin)
    dmg=$(ls "$bundle"/dmg/*.dmg 2>/dev/null | head -1); [ -f "$dmg" ] || fail "no .dmg in $bundle/dmg"
    mount=$(mktemp -d)
    hdiutil attach "$dmg" -nobrowse -readonly -mountpoint "$mount" >/dev/null; ok "mounted $(basename "$dmg")"
    app=$(ls -d "$mount"/*.app | head -1); [ -d "$app" ] || fail "no .app in the disk image"
    destination="$HOME/Applications"; mkdir -p "$destination"; rm -rf "$destination/Onda.app"
    cp -R "$app" "$destination/Onda.app"
    hdiutil detach "$mount" >/dev/null; ok "copied Onda.app to $destination"
    xattr -cr "$destination/Onda.app" || true
    launch "open -W -n '$destination/Onda.app'"
    rm -rf "$destination/Onda.app"; ok "removed Onda.app"
    ;;
  *) fail "unsupported OS: $(uname -s)" ;;
esac
echo "installer tests passed"
