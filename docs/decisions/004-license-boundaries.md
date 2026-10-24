# ADR 004: Apache project with narrowly reviewed MPL dependencies

Status: accepted, 3 October 2026.

Onda's own code is Apache-2.0. The dependency policy denies unapproved licenses, registries and Git sources. GPL/AGPL are not approved. Dual-licensed dependencies with an LGPL alternative are used under their permissive MIT/Apache alternative. Model licensing is reviewed separately.

The chosen Tauri toolchain depends on four MPL-2.0 CSS parser/build packages (`cssparser` 0.37.0, `cssparser-macros` 0.7.1, `dtoa-short` 0.3.5, `selectors` 0.38.0) and `option-ext` 0.2.0, used through Tauri's directory handling in the runtime and build chain. Retaining Tauri's maintained Windows/WebView integration is a stronger engineering reason than replacing that stack to avoid these small, unmodified dependencies. No MPL source has been copied into Onda files.

MPL has file-level source obligations and permits combining covered files with separately licensed larger works. This does not change Onda's own Apache license. The exact unmodified MPL packages remain available under MPL, with notices retained. `deny.toml` grants exceptions only to these five exact reviewed package versions; MPL is not globally allowed. Any new package/version must be reviewed again.

`scripts/collect-mpl-source.ps1` copies these exact package source trees from locked Cargo metadata and local installers bundle `licenses/MPL-source.zip`; workflows also attach corresponding source separately. A distributor must retain those source/notice resources and clearly identify the ZIP as the corresponding MPL dependency source. Build-only packages are included too, so downstream reviewers have the complete reviewed boundary. ZIP timestamps/entry order are normalized, but original source bytes remain unchanged.

The generated `THIRD_PARTY_NOTICES.txt` and `docs/dependency-inventory.json` inventory the exact Rust and npm locks, including build/optional/all-target dependencies. Inventory membership is broader than distributed Windows code. Direct native libraries (CPAL, windows-rs, whisper.cpp/whisper-rs, SQLite, ring transport/resampling) use permissive terms; see the generated notices and model manifest for exact versions/licenses.

References: [Mozilla MPL FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/), [MPL 2.0 text](https://www.mozilla.org/en-US/MPL/2.0/), [cargo-deny license policy](https://embarkstudios.github.io/cargo-deny/checks/licenses/cfg.html).
