#!/usr/bin/env bash
# Run the supplied package in its own process group and bus session. An AppImage launcher may
# fork, so verify the Onda executable within that group as well as the launcher.
set -euo pipefail
set +m
program=$1
shift
startup_seconds=6
setsid --wait dbus-run-session -- "$program" "$@" &
group=$!
stop_owned_group() {
  if kill -0 -- "-$group" 2>/dev/null; then
    kill -TERM -- "-$group" 2>/dev/null || true
    for _ in {1..50}; do kill -0 -- "-$group" 2>/dev/null || break; sleep 0.1; done
    if kill -0 -- "-$group" 2>/dev/null; then kill -KILL -- "-$group" 2>/dev/null || true; fi
  fi
  wait "$group" 2>/dev/null || true
}
trap stop_owned_group EXIT
sleep "$startup_seconds"
kill -0 "$group" 2>/dev/null || { echo "The installed launcher exited during startup." >&2; exit 1; }
for process in $(pgrep -g "$group" -x onda || true); do
  executable=$(readlink "/proc/$process/exe") || continue
  if [[ "$executable" == "$(readlink -f "$program")" || "$executable" == */usr/bin/onda ]]; then
    echo "Launched the installed executable $executable, pid $process."
    exit 0
  fi
done
echo "No installed Onda executable belongs to the launched process group." >&2
exit 1
