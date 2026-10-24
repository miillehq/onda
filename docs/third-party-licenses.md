# Third-party licenses and provenance

Eagle is licensed Apache-2.0; see `LICENSE` and `NOTICE`. No proprietary artwork, branding, private protocol, or competitor source was copied. Native competitor implementation research and pinned source links are recorded in `native-references.md`. WhimprFlow and Handy informed API/workflow decisions, not a source-code transplant.

The authoritative dependency versions are `Cargo.lock` and `package-lock.json`. `node scripts/generate-notices.mjs` builds `THIRD_PARTY_NOTICES.txt` and `docs/dependency-inventory.json` from the installed locked source trees, preserving available LICENSE/COPYING/NOTICE files. `scripts/build.ps1` generates these before bundling and the NSIS resources include the license/notices. The inventory includes build-only, optional and other-platform dependencies; it is not a claim that every package is shipped in the Windows executable.

Important permissive dependencies include Tauri and windows-rs (MIT/Apache-2.0), CPAL (Apache-2.0), whisper-rs/sys (Unlicense), vendored whisper.cpp (MIT), rubato (MIT), rtrb (MIT/Apache-2.0), Tokio/serde/SQLite bindings and frontend tooling. The bundled SQLite amalgamation is public domain; its Rust bindings have their own permissive license. Preserve each package's exact generated notices. Do not infer that a similarly named model has the library's license.

`deny.toml` approves MIT/Apache/BSD/ISC/Zlib, Unicode data terms, public-domain-equivalent licenses, Boost, the LLVM exception, and the reviewed certificate-data license. Unapproved licenses fail. GPL and AGPL are not approved; a package offering an LGPL **alternative** alongside MIT/Apache is selected under a permissive option rather than permitting LGPL.

Five exact MPL-2.0 Tauri dependencies are narrow exceptions: `cssparser` 0.37.0, `cssparser-macros` 0.7.1, `dtoa-short` 0.3.5, `selectors` 0.38.0, and `option-ext` 0.2.0. Four primarily serve the Tauri HTML/CSS build chain; `option-ext` is also reachable at runtime via directory handling. Their unmodified covered source stays MPL. The architectural reason and distribution obligations are documented in [ADR 004](decisions/004-license-boundaries.md). `scripts/collect-mpl-source.ps1` creates the exact source ZIP with stable entry paths/timestamps and no modified source bytes; local installers include `licenses/MPL-source.zip`. CI also attaches source separately for review. New MPL versions do not inherit an automatic exception.

Local model assets are not committed or bundled. Current built-in Whisper GGML assets are licensed MIT and downloaded only after selection, with a pinned repository revision, expected size and SHA-256 in `crates/eagle-asr/src/manifest.rs`. Audio fixture provenance is in `tests/fixtures/asr/README.md`; the checked-in JFK speech fixture is used by the real transcription test. Optional future Parakeet/ONNX models require a fresh model/license review; no such asset is silently fetched here.

WebView2 is a Microsoft runtime prerequisite installed through its official bootstrapper, separately from Eagle's Apache source. Vulkan SDK tools are build prerequisites, not application assets. Release workflows download only the requested pinned SDK, verify its published SHA-256 and Authenticode status, and do not distribute the SDK installer. A GPU driver remains the user's/vendor's runtime dependency.

## Microsoft native runtime boundary

The release executables import MSVC's native C/C++ runtime. `scripts/collect-msvc-runtime.ps1` discovers the installed Visual Studio 2022 release `VC/Redist/MSVC/<version>/x64/Microsoft.VC143.CRT` directory through vswhere, verifies x64 PE architecture and Microsoft Authenticode signatures, copies unmodified DLLs into ignored artifacts, verifies copied SHA-256, and records recursive dumpbin imports. The bundler installs these DLLs beside the shell/workers. Debug runtime directories and System32 files are never copied. Windows 10+ provides the UCRT/API sets; Vulkan's driver loader is optional and belongs to the vendor/runtime installation.

These Microsoft binaries remain under Microsoft's software terms, **not Apache-2.0**. The current collected set is `concrt140`, `msvcp140`, `msvcp140_1`, `msvcp140_2`, `msvcp140_atomic_wait`, `msvcp140_codecvt_ids`, `vccorlib140`, `vcruntime140`, `vcruntime140_1`, and `vcruntime140_threads` (all `.dll`). The implementation host's servicing file version was 14.44.35211.0; per-build exact versions, source-relative names, hashes and import classifications are in the installed `licenses/Microsoft-runtime-manifest.json`.

Distribution is limited to publishers with applicable valid Visual Studio licensing and its redistributable-code rights. Installing Build Tools alone does not establish those rights: its current license separately permits certain third-party OSI open-source C++ dependency builds but otherwise depends on a valid Visual Studio Product license. Release publishers must satisfy the applicable edition's terms. The collector carries the unchanged official runtime and Build Tools license documents plus the installed REDIST/third-party notices under `licenses/`. It does not invent a sublicense or assert publisher eligibility.

Microsoft generally prefers central runtime installation for independent servicing, but explicitly permits app-local deployment for users without administrator rights. Eagle selects app-local deployment to preserve its per-user install path and assumes responsibility for updating/releasing those DLLs when Microsoft services them. Certificate/signature verification and hash provenance do not replace that maintenance obligation. See [redistribution rules](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files?view=msvc-170), [the VS 2022 distributable list](https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution), [deployment choices](https://learn.microsoft.com/en-us/cpp/windows/choosing-a-deployment-method?view=msvc-170), and [Microsoft's license directory](https://visualstudio.microsoft.com/license-terms/).

Optional Vulkan builds retain per-file copyright/SPDX/hash evidence from the actual `Include/vulkan` and `Include/vk_video` headers, with complete Apache/MIT terms and available SDK notices under `licenses/`. The compiler, SDK helper executables and debugging layers are not shipped. Portable SDK extractions still need those header notices; the collector refuses an unreviewed header license.

Audit commands:

```powershell
cargo install cargo-deny --version 0.19.7 --locked
cargo deny --locked check licenses advisories sources
cargo audit
npm audit
node scripts/generate-notices.mjs
```

At implementation review, the Windows all-feature dependency policy passed licenses/advisories/sources. `cargo audit` reported zero vulnerability advisories, with informational warnings for the transitive unmaintained `proc-macro-error` and an other-platform `glib` unsoundness advisory in the all-target lock. The Windows graph excludes glib. `npm audit` reported zero vulnerabilities. Audit results expire as databases change; CI fetches the current RustSec advisory database on each run. Transitive unmaintained advisories are reported by full-lock audit; the deny policy fails unmaintained direct workspace dependencies and actual Windows vulnerability advisories.
