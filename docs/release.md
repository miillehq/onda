# Release engineering

## What a release contains

0.3.0 is a GitHub pre-release with unsigned packages for:

| Platform | Package | Support |
| --- | --- | --- |
| Windows 10 22H2 (build 19045) and 11, x64 and ARM64 | per-user NSIS installer (`*-setup.exe`) with a SHA-256 file beside it | x64 supported, ARM64 Tier 2 |
| Linux, x64 and ARM64 | deb and AppImage | x64 supported, ARM64 Tier 2 |
| macOS 12 or later, Apple Silicon and Intel | dmg | experimental (Tier 2) |

The tiers are in [ADR 007](adr/007-platform-support.md). Because nothing is signed, Windows
SmartScreen warns on first launch, and on macOS the user allows Onda under System Settings ›
Privacy & Security › Open Anyway. Which packages have passed the installer test (L7) is recorded
in [VERIFICATION.md](../VERIFICATION.md). On `master`, the `installers` workflow builds and
installer-tests every package above on its own platform. Before a release, run the manual matrix
in [testing.md](testing.md); what it does not cover goes under "Unverified" in the ledger.

End users need no Node, Rust, SDK, API key or administrator account. The Windows installer
includes the native helpers and approved app-local Microsoft C++ runtime DLLs; the OS supplies
UCRT and the Windows API sets. The official WebView2 bootstrapper downloads WebView2 (at least
120.0.2210.55) only when needed; a fully offline first install without WebView2 needs Tauri's
offline runtime installer option. The deb declares the libraries it needs. Models are separate
explicit downloads; after that, local dictation works offline. Windows 7 and 8 and 32-bit systems
are not supported.

## Build

Windows (Rust from `rust-toolchain.toml`, Node 24, Visual Studio 2022 or 2026 C++ Build Tools with the
Windows SDK, CMake and Ninja, and LLVM's `libclang.dll`; `scripts/toolchain.ps1` finds CMake and
LLVM or accepts `LIBCLANG_PATH`):

```powershell
npm ci
./scripts/build.ps1                  # unsigned CPU release (npm run release)
$env:VULKAN_SDK = 'C:\VulkanSDK\1.4.363.0'
$env:ONDA_VULKAN_TARGET_DIR = 'C:\evn'
./scripts/build.ps1 -WithVulkan      # CPU plus the isolated GPU worker
```

On Windows on ARM, `prepare.ps1` builds the speech worker with `scripts/arm64-worker.ps1` in a
separate PowerShell process, which loads `scripts/arm64-toolchain.ps1`, because whisper.cpp builds
there only with clang-cl; the ARM64 developer environment it sets up never reaches the caller's
shell. Do not set `WHISPER_DONT_GENERATE_BINDINGS=1` on Windows: the bundled
bindings do not match the Windows ABI. The Vulkan helper imports only build-relevant MSVC
variables, selects Ninja and uses a short separate target, because MSBuild's shader subprojects
exceed MAX_PATH; preparation builds the CPU workers first and restores the CMake generator.

Linux and macOS (see [CONTRIBUTING.md](../CONTRIBUTING.md) and [linux.md](linux.md) for the
packages each needs):

```bash
npm ci
scripts/build.sh --bundles deb,appimage   # Linux; add --with-vulkan for the GPU worker
scripts/build.sh --bundles app,dmg        # macOS
scripts/installer-test.sh                 # install, launch and remove the result (L7)
```

`build.sh` runs `prepare.sh --release`, the contracts and the notices, then `tauri build` with
`tauri.linux.conf.json` or `tauri.macos.conf.json`.

Preparation stages the release workers under Tauri's target-triple names: the SSE2 baseline CPU
worker, the AVX2 worker that Onda prefers on CPUs that support it (on ARM, the baseline worker
under that name), and optionally the Vulkan worker. The build regenerates types and the complete
locked dependency notices. On Windows it also packages an exact, reproducible MPL source ZIP and
collects the verified Microsoft runtime DLLs and licenses. Model files, credentials, signing keys
and debug profiles never enter a package.

Packages land in `target/release/bundle/` (`nsis/`, `deb/`, `appimage/`, `dmg/`). Installation is
per user. Upgrading or reinstalling keeps the data directory, the SQLite schema and the models,
and external model directories; uninstalling does not delete user data. Keep the `identifier`
(`com.miille.onda`) stable through branding changes unless implementing an explicit data
migration.

After final linking, or against installed executables, run
`collect-msvc-runtime.ps1 -ExecutablePaths @(...) -WithVulkan` to audit the full import closure. It
verifies Microsoft signatures, the build machine's architecture (x64 or ARM64) and hashes, never copies System32 or debug runtime
DLLs, and records relative provenance. The publisher's redistribution eligibility and app-local
servicing obligation are in [third-party-licenses.md](third-party-licenses.md).

### Installer branding

The NSIS installer and uninstaller use the Onda brand (`tauri.conf.json` › `bundle.windows.nsis`):

- `installerIcon` and `uninstallerIcon`: `icons/icon.ico`, the brand package's `onda.ico` unchanged.
- `headerImage` and `uninstallerHeaderImage`: `icons/installer/header.bmp` (150×57). The black
  lockup, 22 px tall and 102 px wide (the brand minimum is 100 px), on white with more than its x
  of clear space.
- `sidebarImage`: `icons/installer/sidebar.bmp` (164×314). The dark app icon at 88 px and the white
  wordmark on the graphite tile (#262628 → #0F0F10 → #040404), for the Welcome and Finish pages.

Regenerate the bitmaps with `scripts/installer-art.sh` (rsvg-convert and ImageMagick) from the brand
masters in `icons/installer/source/`, copied unchanged from the brand package. NSIS needs
uncompressed 24-bit BMP3 files without alpha; Windows stretches them at higher display scaling, as
it does for every MUI2 installer. How the pages look at 100–200 % scaling and in high-contrast
themes is checked by hand ([testing.md](testing.md)).

## Signing and updates

Unsigned builds work without secrets. Authenticode and updater signatures serve different purposes. A public signed release requires a real publisher certificate or signing service, an HTTPS update channel and a separate Tauri signing key.

- `ONDA_WINDOWS_CERT_THUMBPRINT`: an accessible publisher certificate and private key in the Windows store. `sign-windows.ps1` signs and validates the helpers; Tauri signs the shell and installer with SHA-256 and timestamping. This path awaits a real certificate.
- `ONDA_WINDOWS_SIGN_COMMAND`: an alternative trusted distributor/HSM command for Tauri's `signCommand`. Its operator must also sign the helpers using its HSM procedure. Never embed passwords or private bytes in this variable.
- `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: updater signing material, release environment/CI secrets only.
- `ONDA_UPDATE_ENDPOINT`, `ONDA_UPDATE_PUBLIC_KEY`: public build-time channel configuration. The endpoint must be HTTPS.

```powershell
./scripts/build.ps1 -SignedUpdates -WithVulkan
```

Without a configured endpoint and key, the update actions are disabled in the interface and the backend refuses update commands. The plugin has an explicit empty unsigned configuration so startup still works. Signed releases require version-bearing signatures (`requireSignedVersion: true`) and a newer-version comparison, so a fabricated manifest cannot pair a signed old installer with a claimed new version. Insecure HTTP/TLS and unsigned installation are never enabled.

Update download and check requests have a ten-minute bound. Installation reserves operation admission, verifies the downloaded bytes before shutting down native services, joins the hook supervisor, and relies on kill-on-close Windows Job Objects to contain inference children even during an immediate installer exit. No signed update channel exists yet, so this path has not run against a real one.

## CI

`ci.yml` runs on every pull request ([TESTING.md](../TESTING.md)). `installers.yml` runs on
`master` and on demand: it builds the packages and installs, launches and removes them, and checks
that the deb and AppImage resolve their libraries in Fedora, Debian and Arch containers.
All of them use pinned action commits and read-only repository permissions; only the release
job below may write.

`release.yml` publishes a release from a `v*` tag. Its gate job checks that the tag,
`package.json`, `tauri.conf.json` and the Cargo workspace carry one version, that the tagged
commit is on `master`, and that one `installers` run passed on that commit with all six package
jobs green; the Tier 2 jobs are checked by name, because they never fail their run. The publish
job then downloads that run's packages (no rebuild), adds `SHA256SUMS.txt`, the third-party
notices and the MPL source archive, takes the notes from the version's `CHANGELOG.md` section,
and creates the GitHub release, as a pre-release for 0.x versions. To ship:

1. Bump the version in `package.json` (`npm version --no-git-tag-version`), `Cargo.toml`
   (`[workspace.package]`, then `cargo update --workspace`) and `src-tauri/tauri.conf.json`, and
   write the `CHANGELOG.md` section, in one pull request.
2. After it merges, wait for the `installers` run on that `master` commit to pass on all six
   platforms, and record it in the ledger.
3. Tag that commit `v<version>` and push the tag. If the gate fails, nothing is published: fix,
   merge, and tag the new commit.

Run on demand, `release.yml` instead builds a Windows x64 candidate for review, with optional
Vulkan and updater-signing inputs, and publishes nothing.

`scripts/smoke.ps1` runs a real hidden WebView2 window against an isolated debug profile;
`-UnavailableCloudStorage` seeds an unusable account store and proves local controls still work.
It owns and stops its Vite process; release builds reject its reporting command and ignore its
profile environment variable.

## macOS signing and notarization

Not wanted for now (the owner's ruling, PLAN.md slice 15): the dmg ships unsigned. The
`installers` workflow builds it on `macos-latest` and the installer test clears the quarantine
attribute before launching it. On a user's Mac, Gatekeeper refuses the first launch; the user
allows Onda under System Settings › Privacy & Security › Open Anyway. The unsigned app also costs
the Accessibility permission that lets Onda paste: macOS ties it to the exact build, so after an
update Onda can show as on under Privacy & Security › Accessibility and still not be allowed until
the user switches it off and on again. A Developer ID signature keeps the permission across
updates. When signing is wanted, it needs an Apple Developer Program membership first. Then:

1. Export a "Developer ID Application" certificate as a .p12 and add repository secrets
   `APPLE_CERTIFICATE` (the .p12 as base64), `APPLE_CERTIFICATE_PASSWORD`,
   `APPLE_SIGNING_IDENTITY` (for example `Developer ID Application: Name (TEAMID)`), and for
   notarization `APPLE_ID`, `APPLE_PASSWORD` (an app-specific password) and `APPLE_TEAM_ID`.
2. Pass those secrets as environment variables to the `scripts/build.sh --bundles app,dmg` step.
   `tauri build` signs the app, the sidecars and the disk image, and notarizes them, when they are
   set.
3. Re-sign before recording evidence: Accessibility and Input Monitoring grants belong to the
   signing identity, so a grant made to an unsigned development build does not carry over.
