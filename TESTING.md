# Testing strategy

Strategy only: what each level proves, the tool, the command and where it runs. Evidence goes in
[VERIFICATION.md](VERIFICATION.md). The audit behind this plan is [docs/TEST-AUDIT.md](docs/TEST-AUDIT.md);
pre-Keel validation records and the manual release matrix are in [docs/testing.md](docs/testing.md).

## Levels

Every check runs at the cheapest level that can catch the bug it guards.

| Level | Proves | Tool | Command | Runs | Today |
|---|---|---|---|---|---|
| L0 | well-formed code | rustfmt, clippy, tsc | `keel.json` `lint` | every push | yes |
| L1 | a function or module does what it says | `cargo test`, Vitest | `cargo test --locked --workspace`; `npm run test:unit` | every push | Rust; Vitest harness |
| L2 | the mock and the real backend agree | ts-rs drift check; `contracts/examples.ts` run against the mock (Vitest) and the real app (L6) | `npm run contracts:check`; `npm run test:unit` | every push | types, defaults, validation, profile names |
| L3 | modules work against real I/O: SQLite files, temp dirs, localhost servers, child processes, DPAPI or keyring | `cargo test`; golden real-model tests | `cargo test --locked --workspace` | every push | yes |
| L4 | a user can do the journey, against the labelled mock | Playwright, chromium and webkit (`tests/ui/`) | `npm run test:ui` | every PR, 3 OSes | onboarding, dictionary, settings, error display |
| L5 | structure, layout and pixels have not drifted | ARIA snapshot, geometry snapshot, per-OS pixels | with L4 | with L4 | tiers 1 and 2; tier 3 once the snapshots workflow runs |
| L6 | the debug app launches and the UI reaches the real backend | WebdriverIO embedded provider (`tests/e2e/`); `ui/src/smoke.ts` | `npm run build:e2e`, then `npm run test:e2e` | every PR | settings and dictionary across a restart |
| L7 | the bundle installs, launches and uninstalls | NSIS, deb, AppImage and dmg scripts | planned: installers workflow | `master`, release | Windows NSIS step |
| L8 | other distributions, assistive tech | containers, manual | release checklist | release | manual matrix |

Rules for the mock: it implements the same `Backend` interface from the same generated types,
shows a visible badge that every browser test asserts first, and is checked against the real
dispatcher by the contract suite before any test depends on it.

## Critical journeys

| Journey | Proven at |
|---|---|
| Hold the shortcut, speak, release: text lands in the focused field (Windows hook; Linux portal and paste; macOS event tap) | L1 matcher and state, L3 real-model ASR, L6 replay into an owned field, manual on real apps |
| AI polish with the chosen provider; any failure keeps the original text and calls no other provider | L1 validation, L2 error mapping, L3 localhost providers, L4 error display, quality fixtures |
| ChatGPT sign-in, refresh and sign-out | L3 on every OS that stores accounts |
| Download, verify and select a model | L3 resume, checksum and verification before use; L4 model page |
| Dictionary, snippets and settings survive a restart | L1, L2 defaults and validation, L3 persistence, L4, L6 |
| History kept N days, encrypted; stats and streaks | L3 persistence, pruning and protection; L4 History and Stats |
| Install, upgrade, uninstall | L3 migrations, L7 on each OS family |

Coverage is tracked per journey in this table, never as a percentage. A change that removes a
level from a journey needs a replacement that proves the same behaviour first.

## Visual tiers

- Tier 1, structure: ARIA snapshots, one baseline shared by every OS and engine.
- Tier 2, geometry: rounded boxes of elements marked `data-geo`, one JSON baseline, 8 px tolerance.
- Tier 3, pixels: full-page PNG per engine per OS, `maxDiffPixelRatio` 0.002 (0.001 for the pill),
  animations disabled. Baselines come only from the snapshots workflow on the runners
  (`.github/workflows/snapshots.yml`); a screen without one skips its pixel check and says so.
- Every browser test fixes the clock (2026-11-13 10:00) so greetings and dates never move a
  baseline. Refresh tiers 1 and 2 after an intended change with
  `UPDATE_SNAPSHOTS=1 npx playwright test -g "tier 1|tier 2" --update-snapshots=all`.

## Proving a test

A new or rewritten test is trusted only after it has failed: break the code it guards, run it
red, restore, run it green, and record the red message in the PR. The audit's 17 planted bugs
(A1 to I2 in `docs/TEST-AUDIT.md`) are the first probes; each missed one gets a test.

## Flaky tests

A test that fails without a code change is reproduced first (a loop around `cargo test`, or
`--repeat-each=30` in Playwright, on the failing platform), then quarantined with `#[ignore]` or
`test.fixme` and an issue that records the rate and the message, and kept running in an advisory
job. It returns only after a fix that is shown red without it. Retries are allowed only in L6 and
L7 jobs, once; below that a retry hides a bug.

## Budgets

- L0 to L3 on one runner: under 3 minutes with a warm cache (2 minutes on Windows on 2026-11-12).
- Whole PR wall clock: 20 minutes. The NSIS build and installer test run on `master` and releases.
- L4 and L5 per OS: under 5 minutes. L6 per platform: under 15 minutes including the debug build.

## Deliberately not automated

Physical microphones and hot-plug, typing into third-party apps (Notepad, browsers, Electron,
terminals), elevated and RDP sessions, suspend and resume, real ChatGPT accounts and real
provider keys, GPU vendors beyond the measured ones, and macOS Accessibility prompts. These stay
in the manual matrix in `docs/testing.md`, recorded per release in `VERIFICATION.md`.
