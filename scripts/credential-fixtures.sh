#!/usr/bin/env bash
# Only the temporary desktop store and the fixture's loopback provider receive test data.
set -euo pipefail
[[ "${DISPOSABLE_RUNNER:-}" == github-hosted && "${GITHUB_ACTIONS:-}" == true ]] || {
  echo "Credential fixtures require a disposable hosted user." >&2; exit 1;
}

run_case() {
  local package=$1 name=$2 report
  if ! report=$(cargo test --locked -p "$package" --lib "$name" -- --ignored --exact --test-threads=1); then
    printf '%s\n' "$report"; return 1
  fi
  printf '%s\n' "$report"
  grep -Fq '1 passed; 0 failed' <<<"$report"
}

fixtures() {
  export ONDA_TEST_CREDENTIAL_STORE=isolated
  run_case onda-cloud oauth::tests::full_registration_models_responses_signout_and_returning_identity
  run_case onda-cloud oauth::tests::serializes_refresh_across_independent_service_instances_and_detects_crash
  run_case onda-platform protect::linux_tests::keyring_sealed_bytes_round_trip_and_stay_apart_by_purpose
}

if [[ "${1:-}" == private-bus ]]; then
  gnome-keyring-daemon --foreground --unlock --components=secrets < "$XDG_RUNTIME_DIR/fixture-password" >/dev/null &
  daemon=$!
  trap 'kill "$daemon" 2>/dev/null || true' EXIT
  gdbus wait --session --timeout=15 org.freedesktop.secrets
  fixtures
  run_case onda-cloud keys::tests::secret_service_round_trip
  exit
fi

work=$(mktemp -d "$RUNNER_TEMP/onda-credential-fixture.XXXXXXXX")
case "$(uname -s)" in
  Linux)
    trap 'rm -rf -- "$work"' EXIT
    export XDG_CONFIG_HOME="$work/config" XDG_DATA_HOME="$work/data" XDG_RUNTIME_DIR="$work/runtime"
    mkdir -m 700 "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_RUNTIME_DIR"
    openssl rand -hex 24 | tr -d '\n' > "$XDG_RUNTIME_DIR/fixture-password"
    chmod 600 "$XDG_RUNTIME_DIR/fixture-password"
    dbus-run-session -- bash "$0" private-bus
    ;;
  Darwin)
    # Search paths are not secrets. Save each complete path, including spaces, for restoration.
    previous=$(security default-keychain -d user | sed -E 's/^[[:space:]]*"//;s/"[[:space:]]*$//')
    paths=()
    while IFS= read -r path; do paths+=("$path"); done < <(security list-keychains -d user | sed -E 's/^[[:space:]]*"//;s/"[[:space:]]*$//')
    chain="$work/fixtures.keychain-db"
    password=$(openssl rand -hex 24)
    cleanup() {
      security default-keychain -d user -s "$previous"
      security list-keychains -d user -s "${paths[@]}"
      security delete-keychain "$chain"
      rm -rf -- "$work"
    }
    security create-keychain -p "$password" "$chain"
    trap cleanup EXIT
    security unlock-keychain -p "$password" "$chain"
    security list-keychains -d user -s "$chain"
    security default-keychain -d user -s "$chain"
    fixtures
    run_case onda-cloud keys::tests::keychain_round_trip
    ;;
  *) echo "Windows runs the same account cases in its normal DPAPI suite." >&2; exit 1 ;;
esac
