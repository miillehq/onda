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
| 2026-11-13 | L2 | the development mock answers all 15 contract examples (fresh-install defaults, 9 settings refusals, 5 profile-name cases) | windows-x64, local | `npm run test:unit` |
| 2026-11-13 | L2 probe | each audit drift turns the mock red: code-point name counting fails "the name limit counts graphemes"; demo settings on a fresh install fail "a fresh profile starts from Rust's default settings"; skipping validation fails all 9 refusals | windows-x64, local | the old behaviour restored one at a time in `ui/src/mock/`, `npx vitest run` |
| 2026-11-13 | L6 | the real app answers the same 15 examples on a fresh profile and shows no mock badge; all 3 spec files pass | windows-x64, local | `npm run build:e2e`; `npm run test:e2e` |
| 2026-11-13 | L6 probe | a changed Rust message fails `00-contract.spec.ts` ("History can be kept for 30 days at most." vs the example) | windows-x64, local | planted in `crates/onda-core/src/contracts.rs`, rebuilt, run red, restored |
| 2026-11-13 | L1 | the interface logic in `ui/src/lib` (word diff, shortcut keys, activity grid, formatting, microphone level, error routes): 20 tests | windows-x64, local | `npm run test:unit` |
| 2026-11-13 | L1 probe | one planted bug per module turns its tests red: diff ties, Esc accepted, Sunday-first weeks, the MB threshold, an error action to a missing route, swapped attack and release | windows-x64, local | each planted in `ui/src/lib/`, `npx vitest run tests/unit/lib.test.ts`, restored |
| 2026-11-13 | L1, L3 | probes A1 (model checksum before inference), A3 (CPU-only never probes a GPU), C1 (PKCE verifier, code and redirect checked by the token mock), C2 (the 256 KiB cap is per event) now caught; the OAuth callback, JWT and pre-cancel tests run beyond Windows; the Job Object kill test runs unignored; `audio_context` is tested in the library. onda-cloud 42 passed and 1 ignored, onda-asr 18 passed | windows-x64, local | `cargo test --locked -p onda-cloud -p onda-asr` |
| 2026-11-13 | probe | every new, strengthened or moved test above failed with its bug planted (checksum skipped; CPU-only dropped; nonce, state or a wrong redirect sent; the frame counter not reset; state and nonce checks removed; kill-on-close removed; the context margin removed) and passed once restored | windows-x64, local | planted one at a time in a worktree |
| 2026-11-13 | L1 | probes P1 (paste waits while Onda still owns the unread clipboard), P3 (silence, speech and clipping from `RecordingBuffer::finish`) and I1 (an auto-repeat never starts a shortcut) now caught; strengthened: the paste chord's KEYUP flags and marker, the 16 KiB helper line cap, the local day key and `civil_from_days`. onda-platform 28 passed and 1 ignored, onda-input 13 and 1 passed | windows-x64, local | `cargo test --locked -p onda-platform -p onda-input` |
| 2026-11-13 | probe | each of those failed with its bug planted (owner check inverted, KEYUP dropped, marker zeroed, silence threshold inverted, energy not summed, clipping without `abs`, repeat filter dropped, size check removed or off by one, year and month offsets, leap-day arithmetic) and passed once restored | windows-x64, local | planted one at a time in a worktree |
| 2026-11-13 | L4, L5 | browser suite against the labelled mock: onboarding, dictionary add and cancel, theme switch, error display on Home and in the pill; tier 1 ARIA and tier 2 geometry baselines shared by both engines; 26 passed, 8 pixel checks skipped (no runner baselines yet); stable in 3 of 3 runs | windows-x64, local, chromium and webkit | `npm run test:ui` |
| 2026-11-13 | L4, L5 probe | a renamed button fails the onboarding behaviour and its tier 1 check; a wider sidebar fails both tier 2 checks; a dictionary save that does nothing fails "adding a word"; removing the mock badge fails all 17 tests | windows-x64, local, chromium | planted one at a time, `npx playwright test --project=chromium`, restored |

## Unverified

- No GitHub Actions run has passed yet. The first (run 43344083850) failed on Linux before any
  test: `scripts/prepare.sh` was committed without its executable bit.
- Linux: nothing run since 2026-10-29 (VPS, `docs/testing.md`), including the Linux-only tests.
- macOS: no code yet; nothing built.
- Windows arm64 and Linux arm64: nothing built.
- L5 tier 3: no pixel baselines until the snapshots workflow runs on the runners. L6: the WebView2
  smoke (`scripts/smoke.ps1`) has not run since 0.1.x.
- L7: the NSIS install test has not passed in CI yet; Linux packages have never been installed by
  a script.
- Real-model golden tests: not run since 2026-10-29.
- Dictation into real applications, physical microphones and real provider accounts on any OS:
  manual matrix, not run for 0.2.0.
- The 11 missed probes above are unguarded until PLAN.md slice 4.
- Linux only, written on Windows and first run by Linux CI: the strengthened `/proc` identity test, the Linux day key, and the keyring seal framing test (checked only in an extracted copy with an in-memory keyring).
