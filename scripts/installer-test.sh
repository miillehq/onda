#!/usr/bin/env bash
# Install, launch and remove only the candidate supplied by this checkout.
set -euo pipefail
repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bundle="${1:-$repository/target/release/bundle}"
work="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/onda-installer.XXXXXXXX")"
mounted=
installed_by_test=0

fail() { echo "FAIL: $*" >&2; exit 1; }

cleanup() {
  if [[ -n "$mounted" ]]; then hdiutil detach "$mounted" >/dev/null || return; fi
  if [[ "$installed_by_test" == 1 ]]; then sudo apt-get remove -y onda >/dev/null || return; fi
  rm -rf -- "$work"
}
trap cleanup EXIT

one_bundle() {
  local found= candidate
  for candidate in "$@"; do
    [[ -e "$candidate" ]] || continue
    [[ -z "$found" ]] || fail "multiple matching bundles in $bundle"
    found="$candidate"
  done
  [[ -n "$found" ]] || fail "missing bundle matching $1"
  printf '%s\n' "$found"
}

case "$(uname -s)" in
  Linux)
    export DISPLAY="${DISPLAY:-:99}"
    export XDG_DATA_HOME="$work/data" XDG_CONFIG_HOME="$work/config" XDG_CACHE_HOME="$work/cache"
    deb="$(one_bundle "$bundle"/deb/*.deb)"
    image="$(one_bundle "$bundle"/appimage/*.AppImage)"
    if [[ "$(dpkg-query -W -f='${Status}' onda 2>/dev/null || true)" == "install ok installed" ]]; then
      fail "a clean-install check requires a machine without an existing Onda package"
    fi
    installed_by_test=1
    sudo DEBIAN_FRONTEND=noninteractive apt-get install -y "$deb" >/dev/null
    paths="$(dpkg-query -L onda)"
    for component in onda onda-input onda-asr-worker onda-asr-avx2-worker; do
      grep -Fxq "/usr/bin/$component" <<<"$paths" || fail "the package is missing $component"
    done
    bash "$repository/scripts/installer-launch-linux.sh" /usr/bin/onda
    sudo apt-get remove -y onda >/dev/null
    installed_by_test=0
    [[ ! -e /usr/bin/onda ]] || fail "the installed binary survived removal"
    chmod +x "$image"
    bash "$repository/scripts/installer-launch-linux.sh" "$image" --appimage-extract-and-run
    ;;
  Darwin)
    dmg="$(one_bundle "$bundle"/dmg/*.dmg)"
    mount="$work/mount"
    mkdir "$mount" "$work/Applications"
    hdiutil attach "$dmg" -nobrowse -readonly -mountpoint "$mount" >/dev/null
    mounted="$mount"
    app="$(one_bundle "$mount"/*.app)"
    installed="$work/Applications/Onda.app"
    ditto "$app" "$installed"
    hdiutil detach "$mount" >/dev/null
    mounted=
    swiftc -parse-as-library "$repository/scripts/installer-launch-macos.swift" -o "$work/launch-check"
    "$work/launch-check" "$installed"
    rm -rf -- "$installed"
    [[ ! -e "$installed" ]] || fail "the copied application survived removal"
    ;;
  *) fail "unsupported OS: $(uname -s)" ;;
esac
echo "installer tests passed"
