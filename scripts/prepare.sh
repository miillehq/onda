#!/usr/bin/env bash
# Linux and macOS counterpart of prepare.ps1: builds the speech workers and the shortcut helper
# that Tauri bundles as sidecars (on Linux the helper talks to the desktop's shortcuts portal).
#   scripts/prepare.sh [--release] [--with-vulkan]
# Runs on macOS's bash 3.2, which treats an empty array as unset under `set -u`: optional
# arrays expand as ${name[@]+"${name[@]}"}.
set -euo pipefail

release=0
with_vulkan=0
for argument in "$@"; do
  case "$argument" in
    --release) release=1 ;;
    --with-vulkan) with_vulkan=1 ;;
    *) echo "Unknown option: $argument" >&2; exit 2 ;;
  esac
done

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository"
mode=debug
release_arguments=()
if [[ $release -eq 1 ]]; then
  mode=release
  release_arguments=(--release)
fi
triple="$(rustc -vV | sed -n 's/^host: //p')"
case "$triple" in
  x86_64-*) x86=1 ;;
  *) x86=0 ;;
esac

# .cargo/config.toml forces MSVC release flags for whisper.cpp; GCC and Clang need their own.
# The same `--config` override is how prepare.ps1 switches on AVX2 below.
unix_flags=(
  --config "env.CMAKE_C_FLAGS_RELEASE.value='-O3 -DNDEBUG'"
  --config "env.CMAKE_CXX_FLAGS_RELEASE.value='-O3 -DNDEBUG'"
)
avx2_flags=()
for flag in GGML_AVX GGML_AVX2 GGML_FMA GGML_F16C GGML_SSE42 GGML_BMI2; do
  avx2_flags+=(--config "env.$flag.value='ON'")
done

cargo build --locked -p onda-input --bin onda-input ${release_arguments[@]+"${release_arguments[@]}"}
cargo build --locked -p onda-asr --features native-asr --bin onda-asr-worker "${unix_flags[@]}" ${release_arguments[@]+"${release_arguments[@]}"}
avx2_target="$repository/target/avx2"
if [[ $x86 -eq 1 ]]; then
  # The default worker is SSE2-only so it runs on any x86-64 CPU; this one needs AVX2/FMA/F16C.
  cargo build --locked -p onda-asr --features native-asr --bin onda-asr-worker --target-dir "$avx2_target" \
    "${unix_flags[@]}" "${avx2_flags[@]}" ${release_arguments[@]+"${release_arguments[@]}"}
else
  # AVX2 is x86 only. Tauri's sidecar list still names the AVX2 worker, so ARM gets the generic
  # worker under that name; the engine selects it only on an x86 CPU with AVX2, never here.
  mkdir -p "$avx2_target/$mode"
  cp "target/$mode/onda-asr-worker" "$avx2_target/$mode/onda-asr-worker"
fi

binaries="$repository/src-tauri/binaries"
mkdir -p "$binaries"
install -m 755 "target/$mode/onda-input" "$binaries/onda-input-$triple"
install -m 755 "target/$mode/onda-asr-worker" "$binaries/onda-asr-worker-$triple"
install -m 755 "$avx2_target/$mode/onda-asr-worker" "$binaries/onda-asr-avx2-worker-$triple"
install -m 755 "$avx2_target/$mode/onda-asr-worker" "target/$mode/onda-asr-avx2-worker"

if [[ $with_vulkan -eq 1 ]]; then
  command -v glslc >/dev/null || { echo "The Vulkan worker needs glslc and the Vulkan headers (Vulkan SDK or libvulkan-dev + glslc)." >&2; exit 1; }
  vulkan_target="$repository/target/vulkan"
  cargo build --locked -p onda-asr --features vulkan --bin onda-asr-worker --target-dir "$vulkan_target" \
    "${unix_flags[@]}" ${release_arguments[@]+"${release_arguments[@]}"}
  install -m 755 "$vulkan_target/$mode/onda-asr-worker" "$binaries/onda-asr-vulkan-worker-$triple"
  install -m 755 "$vulkan_target/$mode/onda-asr-worker" "target/$mode/onda-asr-vulkan-worker"
fi
