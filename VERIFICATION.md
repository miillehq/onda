# Verification ledger

Append-only. One row per claim; a claim names the level, the platform and the run (a CI run link
or the exact local command and commit). Anything not here is "built", not "tested". Records from
before Keel are in [docs/testing.md](docs/testing.md) and were not re-verified.

Commits: the 2026-11-12 runs used `852860d`, which the attribution cleanup rewrote to `60eb6d6`
with an identical tree.

| Date | Level | Claim | Platforms | Evidence |
|---|---|---|---|---|
| 2026-11-12 | L0 | rustfmt and clippy (workspace, warnings as errors) are clean | windows-x64, local | `cargo fmt --all --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings` |
| 2026-11-12 | L1, L3 | workspace suite: 166 passed, 0 failed, 7 ignored | windows-x64, local | `cargo test --locked --workspace` |
| 2026-11-12 | L2 | generated TypeScript contracts match the Rust types | windows-x64, local | `npm run contracts:check` |
| 2026-11-12 | L1 | quality fixtures pass; 10,000-entry index builds in 68 ms | windows-x64, local | `npm run quality` |
| 2026-11-12 | probe | 17 planted bugs: 6 caught, 11 missed (A1, A3, C1, C2, K3, K4, K5, K6, P1, P3, I1); K3 is caught by `npm run quality` only | windows-x64, local | `cargo test -p <crate>` per bug in a copy of `852860d`; list in `docs/TEST-AUDIT.md` |
| 2026-11-13 | L0 to L3 | after moving `target/` to the D: build drive and a cold rebuild: fmt, both clippy runs, 166 passed and 7 ignored, contracts, TypeScript and Vite build, quality | windows-x64, local | `scripts/prepare.ps1` then the commands above on `60eb6d6` |
| 2026-11-13 | L6 | real app: a dictionary word and the dark theme survive a restart (2 spec files, 5 tests); stable in 8 of 8 runs, about 7 s each | windows-x64, local | `npm run build:e2e`; `npm run test:e2e` |
| 2026-11-13 | L6 probe | the restart spec fails when settings saves are dropped (`ON CONFLICT DO NOTHING`): "Expected: dark, Received: system" in `02-restart.spec.ts` | windows-x64, local | planted in `crates/onda-core/src/persistence.rs`, rebuilt, run red, restored, run green |

## Unverified

- No GitHub Actions run has passed yet. The first (run 43344083850) failed on Linux before any
  test: `scripts/prepare.sh` was committed without its executable bit.
- Linux: nothing run since 2026-10-29 (VPS, `docs/testing.md`), including the Linux-only tests.
- macOS: no code yet; nothing built.
- Windows arm64 and Linux arm64: nothing built.
- L4 and L5: no suites exist. L6: the WebKitGTK smoke has not passed in CI yet; the WebView2 smoke
  (`scripts/smoke.ps1`) has not run since 0.1.x.
- L7: the NSIS install test has not passed in CI yet; Linux packages have never been installed by
  a script.
- Real-model golden tests: not run since 2026-10-29.
- Dictation into real applications, physical microphones and real provider accounts on any OS:
  manual matrix, not run for 0.2.0.
- The 11 missed probes above are unguarded until PLAN.md slice 4.
