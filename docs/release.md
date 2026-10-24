# Windows release engineering

## Support policy and prerequisites

Production architecture is x86-64/MSVC. Windows 11 is the primary target; the minimum project support target is Windows 10 22H2 build 19045, with WebView2 at least 120.0.2210.55. The API/dependency choice is compatible with that baseline, but it remains a release qualification target until the clean-machine matrix is recorded. Windows 7/8 and ARM64 are not certified. ARM64 can follow after native/ASR/installer qualification without changing domain contracts.

End users need no Node, Rust, SDK, API key or administrator account. The installer includes native helpers and approved app-local Microsoft C++ runtime DLLs. The OS supplies UCRT/Windows API sets. The official WebView2 bootstrapper downloads only when needed; a fully offline first install with no runtime requires Tauri's offline runtime installer option. Models are separate explicit downloads; installed local dictation works offline. Optional Vulkan releases need a functional vendor driver and fall back to CPU after accelerator failure.

Build prerequisites: Rust 1.95.0, Node 24, Visual Studio 2022 C++ Build Tools with Windows SDK/CMake/Ninja, and LLVM's `libclang.dll`. `scripts/toolchain.ps1` discovers CMake/LLVM, or accepts `LIBCLANG_PATH`. The implementation host also used an isolated libclang package outside Git; no developer tool package is shipped. Do not set `WHISPER_DONT_GENERATE_BINDINGS=1` on Windows: pinned bundled bindings do not match Windows libc ABI. `.cargo/config.toml` intentionally disables native-host/AVX assumptions.

## Build and install

```powershell
npm ci
./scripts/build.ps1                  # unsigned CPU release
$env:VULKAN_SDK = 'C:\VulkanSDK\1.4.363.0'
$env:EAGLE_VULKAN_TARGET_DIR = 'C:\evn'
./scripts/build.ps1 -WithVulkan      # CPU plus isolated GPU worker
```

The Vulkan helper captures/imports only build-relevant MSVC variables, selects Ninja and uses a short separate target. MSBuild shader subprojects exceeded MAX_PATH even in shorter build directories. Preparation restores the prior CMake generator, and builds CPU first. Compiler/SDK tools remain build-only.

Preparation stages genuine release workers with Tauri target-triple names. The build regenerates types and complete locked dependency notices, packages an exact/reproducible MPL source ZIP, and collects verified Microsoft runtime resources/licenses. Local installers include these resources. Model binaries, credentials, signing keys and debug profiles never enter the bundle.

Artifacts are `target/release/bundle/nsis/*setup.exe` with adjacent SHA-256. Installation is per user with shortcuts/uninstaller. Upgrade/reinstall preserves the identifier's local data directory, SQLite schema and model assets. External configured model directories are preserved too. Keep `identifier` stable through branding changes unless implementing an explicit data migration. Uninstall does not destroy user assets by default.

After final linking or against installed executables, run `collect-msvc-runtime.ps1 -ExecutablePaths @(...) -WithVulkan` to audit the full import closure. It verifies Microsoft signatures/x64 architecture/hashes, never copies System32/debug runtime DLLs, and records relative provenance. The publisher's redistribution eligibility and app-local servicing obligation are in third-party-licenses.md.

## Signing and updates

Unsigned builds work without secrets. Authenticode and updater signatures serve different purposes. A public release requires a real publisher certificate/private signing capability, an HTTPS channel and a separate Tauri signing key.

- `EAGLE_WINDOWS_CERT_THUMBPRINT`: accessible publisher certificate/private key in the Windows store. `sign-windows.ps1` signs/validates helpers; Tauri signs the shell/installer with SHA-256 and timestamping. This path awaits a real certificate.
- `EAGLE_WINDOWS_SIGN_COMMAND`: alternative trusted distributor/HSM command for Tauri's signCommand. Its operator must also sign helpers using its HSM procedure. Never embed passwords/private bytes in this variable.
- `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: updater signing material, release environment/CI secrets only.
- `EAGLE_UPDATE_ENDPOINT`, `EAGLE_UPDATE_PUBLIC_KEY`: public build-time channel configuration. Endpoint must be HTTPS.

```powershell
./scripts/build.ps1 -SignedUpdates -WithVulkan
```

No configured endpoint/key means disabled UI actions and refused backend update commands. The plugin has an explicit empty unsigned configuration so startup still works. Signed releases require version-bearing signatures (`requireSignedVersion:true`) and newer-version comparison. This prevents a fabricated manifest pairing a signed old installer with a claimed new version. Legacy artifacts lacking a trusted version comment need re-signing before use. Insecure HTTP/TLS and unsigned installation are never enabled. Real certificate/channel publication/tamper qualification remain external gates; no signer identity was invented.

## Verification and CI

Use testing.md, including real-model gates and opt-in controlled desktop insertion. `scripts/smoke.ps1` runs a real hidden WebView2 window against an isolated debug profile. `-UnavailableCloudStorage` seeds an immutable directory collision and proves local controls still work. It owns/stops its Vite process; release builds reject its reporting command and ignore its profile environment variable.

CI prepares workers, checks tests/types/quality/dependencies, builds NSIS and verifies silent install/uninstall, DLL hashes/source-license layout/import closure. The release workflow builds tag/manual candidates with optional GPU/signing inputs. Both use pinned action commits and Windows Server 2022, uploading artifacts without automatic publication. Hosted success is not claimed from local workflow lint.

Before public distribution record the Windows/hardware/app/physical-microphone/OAuth matrix. The implementation host proves real CPU/GPU recognition, controlled Win32 insertion and native IPC, but is not a substitute for all supported machines. Observed final build/installer status belongs in testing.md.

Update download/check requests have a ten-minute bound. Installation reserves operation admission, verifies downloaded bytes before shutting down native services, joins the hook supervisor, and relies on kill-on-close Windows Job Objects to contain inference children even during an immediate installer exit. These mechanics are compiled and locally reviewed; a real signed update channel still needs certificate-backed qualification.
