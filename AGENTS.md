# Onda

Onda by miille is voice dictation that types wherever the cursor is. Speech recognition runs
locally (whisper.cpp in an isolated worker); optional text cleanup and selection rewrites use the
AI provider the user chooses. It is a Tauri 2 app: a Preact and Vite UI over a Rust workspace.
Windows and Linux on x64 are supported; macOS (experimental) and the ARM64 builds are Tier 2
([ADR 007](docs/adr/007-platform-support.md)).
Decisions live in `docs/adr/`, product behaviour in `docs/product-behavior.md`, the visual
design in `DESIGN.md`.

## Commands

- setup: `npm ci`, then `./scripts/prepare.ps1` (Windows) or `scripts/prepare.sh` (Linux, macOS). It
  builds the real speech and shortcut workers that Tauri embeds; never substitute placeholders.
  On Windows, dot-source `./scripts/toolchain.ps1` first so CMake and libclang are found.
- lint (L0): `cargo fmt --all --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings`;
  `cargo clippy --locked -p onda-asr --features native-asr --all-targets -- -D warnings`; `npx tsc --noEmit`
- unit and integration (L1, L3): `cargo test --locked --workspace`; `npm run test:unit` (Vitest)
- contracts (L2): `npm run contracts:check` and `npm run test:unit`; regenerate with `npm run contracts`
- real-shell journeys (L6): `npm run build:e2e`, then `npm run test:e2e`. The suite launches the debug
  app on a throwaway profile under `artifacts/e2e/`, so it runs beside an installed Onda.
- browser UI and visual (L4, L5): `npm run test:ui` (Playwright starts `npm run dev`; run
  `npx playwright install chromium webkit` once). Pixel baselines: the snapshots workflow only.
- quality fixtures: `npm run quality`
- real-model tests: `docs/testing.md` (`cargo test -p onda-asr --test golden -- --ignored` with a
  downloaded model)
- UI against the labelled mock: `npm run dev`, then http://127.0.0.1:1420 (`?mock=empty|onboarding|error`,
  `?theme=dark`, `?platform=linux|macos`, `/pill.html?pill=listening`)
- the app: `npm run desktop` (Windows); `npm run tauri dev` after `scripts/prepare.sh` (Linux, macOS)
- packages: `npm run release` (Windows NSIS); `scripts/build.sh --bundles deb,appimage` (Linux) or
  `--bundles app,dmg` (macOS), then `scripts/installer-test.sh` installs, launches and removes them (L7)
- a failed CI run: `gh run view <id> --log-failed`

## Layout

- `ui/`: the interface. It reaches the backend only through `ui/src/api.ts` (native IPC in the
  shell, the labelled mock in `ui/src/mock/` in a browser).
- `ui/src/generated/`: TypeScript generated from Rust by ts-rs. Never edit by hand; regenerate.
- `src-tauri/`: the app shell. `src/api.rs` is the contract; `src/runtime*` orchestrates.
- `crates/onda-core`: domain and SQLite. `crates/onda-platform`: audio, focused-field identity,
  insertion, at-rest protection. `crates/onda-input`: the shortcut helper. `crates/onda-asr`:
  models and speech workers. `crates/onda-cloud`: ChatGPT sign-in, AI providers, API keys.
- `tests/e2e/`: the real-shell suite (WebdriverIO). Its plugins exist only in debug builds made
  with `--features e2e`, through the `src-tauri/tauri.e2e.conf.json` overlay.
- `tests/fixtures/`: the ASR clip and quality fixtures. `scripts/`: prepare, build, smoke and
  installer checks.
- `ARCHITECTURE.md` (boundaries), `TESTING.md` (strategy), `PLAN.md` (slices),
  `VERIFICATION.md` (evidence), `docs/TEST-AUDIT.md` (the audit the plan came from).

## Conventions

- Rust owns every policy and sensitive operation; the UI holds no secrets and no business rules.
- Never log, store or send dictated text, audio or keys outside what `docs/security.md` allows.
  Tests never use real tokens; fixtures are labelled as test data.
- New OS-specific code goes behind the existing module API in a `windows_*`, `linux_*` or
  `macos_*` module selected by `cfg`, so the rest stays testable without it.
- Commit messages describe the change in plain words and carry no AI attribution trailers.
- Work lands through pull requests to `master`. Tier 1 CI (Windows x64, Linux x64) must be green
  to merge.

<!-- keel:start -->
## Keel

This repository follows Keel's testing conventions. `keel.json` lists the command for each
testing level (`null` where a level has no suite yet) and the platform tiers from ADR 007.
`TESTING.md` explains the levels L0 to L8. `VERIFICATION.md` is the append-only ledger of what
has been proven, where and by which run.

Rules that are not negotiable:

1. A claim of "tested" names the level, the platform and the run (a CI run link or the exact
   local command). Anything less is "built". The ledger is append-only.
2. A new or rewritten test proves it can fail before it is trusted: break the code path it
   guards, see it red, restore it, see it green, and put the red message in the PR.
3. No existing test is deleted in the PR that adds its replacement. The old test is retired in a
   later PR that names the replacement and the behaviour both guard.
4. The contract changes only through its own small PR: `src-tauri/src/api.rs`,
   `crates/onda-core/src/contracts.rs`, the regenerated `ui/src/generated/`, the mock and
   `contracts/` together, with the contract checks green. Implementations follow in later PRs.
5. Flaky tests are quarantined (`#[ignore]` or `test.fixme`, with an issue that records the
   pattern), never deleted, and never hidden by a retry or a wider timeout. Only real-shell (L6)
   and installer (L7) jobs may retry, once.
6. Pixel baselines are captured by the snapshots workflow on the runners, never on a laptop.
7. Coverage is tracked per user journey (`TESTING.md`), not as a percentage. A PR that lowers a
   journey's coverage does not merge.
<!-- keel:end -->
