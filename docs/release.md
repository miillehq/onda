# Release engineering (Windows, Linux packages and macOS)

## Support policy and prerequisites

Production architecture is x86-64/MSVC. Windows 11 is the primary target; the minimum project support target is Windows 10 22H2 build 19045, with WebView2 at least 120.0.2210.55. The API/dependency choice is compatible with that baseline, but it remains a release qualification target until the clean-machine matrix is recorded. Windows 7/8 and ARM64 are not certified. ARM64 can follow after native/ASR/installer qualification without changing domain contracts.

End users need no Node, Rust, SDK, API key or administrator account. The installer includes native helpers and approved app-local Microsoft C++ runtime DLLs. The OS supplies UCRT/Windows API sets. The official WebView2 bootstrapper downloads only when needed; a fully offline first install with no runtime requires Tauri's offline runtime installer option. Models are separate explicit downloads; installed local dictation works offline. Optional Vulkan releases need a functional vendor driver and fall back to CPU after accelerator failure.

Build prerequisites: Rust 1.95.0, Node 24, Visual Studio 2022 C++ Build Tools with Windows SDK/CMake/Ninja, and LLVM's `libclang.dll`. `scripts/toolchain.ps1` discovers CMake/LLVM, or accepts `LIBCLANG_PATH`. The implementation host also used an isolated libclang package outside Git; no developer tool package is shipped. Do not set `WHISPER_DONT_GENERATE_BINDINGS=1` on Windows: pinned bundled bindings do not match Windows libc ABI. `.cargo/config.toml` intentionally disables native-host/AVX assumptions.

## Build and install

```powershell
npm ci
./scripts/build.ps1                  # unsigned CPU release
$env:VULKAN_SDK = 'C:\VulkanSDK\1.4.363.0'
$env:ONDA_VULKAN_TARGET_DIR = 'C:\evn'
./scripts/build.ps1 -WithVulkan      # CPU plus isolated GPU worker
```

The Vulkan helper captures/imports only build-relevant MSVC variables, selects Ninja and uses a short separate target. MSBuild shader subprojects exceeded MAX_PATH even in shorter build directories. Preparation restores the prior CMake generator, and builds CPU first. Compiler/SDK tools remain build-only.

Preparation stages genuine release workers with Tauri target-triple names: the SSE2 baseline CPU worker, the AVX2 CPU worker that Onda prefers on CPUs that support it, and optionally the Vulkan worker. The build regenerates types and complete locked dependency notices, packages an exact/reproducible MPL source ZIP, and collects verified Microsoft runtime resources/licenses. Local installers include these resources. Model binaries, credentials, signing keys and debug profiles never enter the bundle.

Artifacts are `target/release/bundle/nsis/*setup.exe` with adjacent SHA-256. Installation is per user with shortcuts/uninstaller. Upgrade/reinstall preserves the identifier's local data directory, SQLite schema and model assets. External configured model directories are preserved too. Keep `identifier` stable through branding changes unless implementing an explicit data migration. Uninstall does not destroy user assets by default.

After final linking or against installed executables, run `collect-msvc-runtime.ps1 -ExecutablePaths @(...) -WithVulkan` to audit the full import closure. It verifies Microsoft signatures/x64 architecture/hashes, never copies System32/debug runtime DLLs, and records relative provenance. The publisher's redistribution eligibility and app-local servicing obligation are in third-party-licenses.md.

### Installer branding

The NSIS installer and uninstaller use the Onda brand (`tauri.conf.json` › `bundle.windows.nsis`):

- `installerIcon` and `uninstallerIcon`: `icons/icon.ico`, the brand package's `onda.ico` unchanged.
- `headerImage` and `uninstallerHeaderImage`: `icons/installer/header.bmp` (150×57). The black
  lockup, 22 px tall and 102 px wide (the brand minimum is 100 px), on white with more than its x
  of clear space.
- `sidebarImage`: `icons/installer/sidebar.bmp` (164×314). The dark app icon at 88 px and the white
  wordmark on the graphite tile (#262628 → #0F0F10 → #040404), for the Welcome and Finish pages.
- `minimumWebview2Version` moved from the deprecated `nsis` key to `bundle.windows`, with the same
  value.

Regenerate the bitmaps with `scripts/installer-art.sh` (rsvg-convert and ImageMagick) from the brand
masters in `icons/installer/source/`, copied unchanged from the brand package. NSIS needs
uncompressed 24-bit BMP3 files without alpha; Windows stretches them at higher display scaling, as
it does for every MUI2 installer.

Checked on Linux: the config validates against Tauri CLI 2.12.1's schema (`config.schema.json`), and
NSIS 3.10 compiled a MUI2 script that wires the same files the way Tauri's template does. **Not
checked:** building the real installer, and how the pages look on Windows at 100–200% scaling and
in high-contrast themes. See the installer checklist in testing.md.

## Linux packages

`scripts/build.sh [--bundles deb|appimage|deb,appimage] [--with-vulkan]` is the Linux counterpart of
`build.ps1`: release speech workers (`prepare.sh --release`), contracts, notices, then `tauri build`
with `tauri.linux.conf.json`, which bundles only the speech workers (no keyboard helper). The
packages are unsigned. See [linux.md](linux.md) for what the Linux build does.

## Signing and updates

Unsigned builds work without secrets. Authenticode and updater signatures serve different purposes. A public release requires a real publisher certificate/private signing capability, an HTTPS channel and a separate Tauri signing key.

- `ONDA_WINDOWS_CERT_THUMBPRINT`: accessible publisher certificate/private key in the Windows store. `sign-windows.ps1` signs/validates helpers; Tauri signs the shell/installer with SHA-256 and timestamping. This path awaits a real certificate.
- `ONDA_WINDOWS_SIGN_COMMAND`: alternative trusted distributor/HSM command for Tauri's signCommand. Its operator must also sign helpers using its HSM procedure. Never embed passwords/private bytes in this variable.
- `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: updater signing material, release environment/CI secrets only.
- `ONDA_UPDATE_ENDPOINT`, `ONDA_UPDATE_PUBLIC_KEY`: public build-time channel configuration. Endpoint must be HTTPS.

```powershell
./scripts/build.ps1 -SignedUpdates -WithVulkan
```

No configured endpoint/key means disabled UI actions and refused backend update commands. The plugin has an explicit empty unsigned configuration so startup still works. Signed releases require version-bearing signatures (`requireSignedVersion:true`) and newer-version comparison. This prevents a fabricated manifest pairing a signed old installer with a claimed new version. Legacy artifacts lacking a trusted version comment need re-signing before use. Insecure HTTP/TLS and unsigned installation are never enabled. Real certificate/channel publication/tamper qualification remain external gates; no signer identity was invented.

## Verification and CI

Use testing.md, including real-model gates and opt-in controlled desktop insertion. `scripts/smoke.ps1` runs a real hidden WebView2 window against an isolated debug profile. `-UnavailableCloudStorage` seeds an immutable directory collision and proves local controls still work. It owns/stops its Vite process; release builds reject its reporting command and ignore its profile environment variable.

CI prepares workers, checks tests/types/quality/dependencies, builds NSIS and verifies silent install/uninstall, DLL hashes/source-license layout/import closure. The release workflow builds tag/manual candidates with optional GPU/signing inputs. Both use pinned action commits and Windows Server 2022, uploading artifacts without automatic publication. Hosted success is not claimed from local workflow lint.

Before public distribution record the Windows/hardware/app/physical-microphone/OAuth matrix. The implementation host proves real CPU/GPU recognition, controlled Win32 insertion and native IPC, but is not a substitute for all supported machines. Observed final build/installer status belongs in testing.md.

Update download/check requests have a ten-minute bound. Installation reserves operation admission, verifies downloaded bytes before shutting down native services, joins the hook supervisor, and relies on kill-on-close Windows Job Objects to contain inference children even during an immediate installer exit. These mechanics are compiled and locally reviewed; a real signed update channel still needs certificate-backed qualification.

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
