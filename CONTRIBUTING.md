# Contributing to Onda

Thanks for helping. Onda is voice dictation that types wherever the cursor is, built with Tauri 2:
a Preact interface over a Rust workspace. This page gets you from a fresh clone to a pull request.
[AGENTS.md](AGENTS.md) holds the exact commands, [ARCHITECTURE.md](ARCHITECTURE.md) the
boundaries, [TESTING.md](TESTING.md) what each test level proves, and [docs/README.md](docs/README.md)
which document to read for what.

## Work on the interface only

The interface runs in a browser against a labelled mock (`ui/src/mock/`), so it needs no Rust, no
speech models and no native toolchain. Node 24 is enough:

```bash
npm ci
npm run dev
```

Then open http://127.0.0.1:1420. Add `?mock=empty|onboarding|error`, `?theme=dark` or
`?platform=linux|macos`, or open `/pill.html?pill=listening` for the dictation pill. Read
[DESIGN.md](DESIGN.md) and [docs/frontend-handoff.md](docs/frontend-handoff.md) first.

Run the browser suite with `npm run test:ui`, after `npx playwright install chromium webkit` once.

## Build the whole app

Every platform needs Rust from `rust-toolchain.toml` (rustup installs it) and Node 24. Then:

- **Windows 11 x64:** Visual Studio 2022 C++ Build Tools with the Windows SDK, CMake and LLVM. In
  PowerShell run `. ./scripts/toolchain.ps1` (it finds CMake and libclang), `npm ci`,
  `./scripts/prepare.ps1`, then `npm run desktop`.
- **Linux x64:** the packages listed in [docs/linux.md](docs/linux.md), then `npm ci`,
  `scripts/prepare.sh` and `npm run tauri dev`.
- **macOS:** Xcode Command Line Tools and CMake, then `npm ci`, `scripts/prepare.sh` and
  `npm run tauri dev`. CI builds and tests macOS on every pull request, but it is not yet a
  supported platform ([ADR 007](docs/adr/007-platform-support.md)). On a Mac, Onda pastes into
  other apps only after you turn it on under Privacy & Security › Accessibility; until then
  dictation is copied for ⌘V. A rebuilt, unsigned app may need that switch turned off and on again.

`prepare` builds the real speech and shortcut workers that the app embeds; never replace them
with placeholders. In debug builds, `ONDA_DEV_PROFILE=<absolute folder>` keeps your test profile
apart from an installed Onda.

## Check your change

Run the levels your change touches. CI runs all of them.

| Level | What it checks | Command |
|---|---|---|
| L0 | formatting, lints, types | `cargo fmt --all --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings`; `npx tsc --noEmit` |
| L1, L3 | Rust units and real I/O (files, SQLite, local servers) | `cargo test --locked --workspace` |
| L1, L2 | interface units; the mock and Rust agree | `npm run test:unit`; `npm run contracts:check` |
| L4, L5 | journeys and layout in chromium and webkit, against the mock | `npm run test:ui` |
| L6 | the real app launches and its interface reaches Rust | `npm run build:e2e`, then `npm run test:e2e` |

The L6 suite uses a throwaway profile under `artifacts/e2e/`, so it can run beside an installed
Onda. Types in `ui/src/generated/` come from Rust: after changing a contract type, run
`npm run contracts` instead of editing them.

## The rules

The project follows [Keel](TESTING.md). In plain words:

1. **Say what was tested, where, and by which run.** "Tested" names the level, the platform and
   the CI run or the exact local command. Anything less is "built". Proven claims get a row in
   [VERIFICATION.md](VERIFICATION.md), which is only ever appended to.
2. **Show a new test failing.** Break the code it guards, watch it fail, restore the code, watch
   it pass, and paste the failure message into the pull request.
3. **Replace before you remove.** A test is never deleted in the pull request that adds its
   replacement; a later pull request retires it and names the replacement.
4. **Change the contract on its own.** `src-tauri/src/api.rs`, `crates/onda-core/src/contracts.rs`,
   the regenerated `ui/src/generated/`, the mock and `contracts/` change together, in a small pull
   request of their own. Implementations follow in later ones.
5. **Quarantine flaky tests; don't retry them.** Open an issue with how often it fails, where and
   the message, mark the test ignored or `test.fixme`, and keep it running. Only the real-app (L6)
   and installer (L7) jobs may retry, once.
6. **Pixel baselines come from CI.** They are captured by the snapshots workflow on the runners,
   never on your machine.
7. **Coverage is counted per user journey** (the table in [TESTING.md](TESTING.md)). A change that
   removes a level from a journey needs a replacement that proves the same behaviour first.

And for the code:

- Rust owns every policy and every sensitive operation. The interface reaches it only through
  `ui/src/api.ts` and holds no secrets or business rules.
- Never log, store or send dictated text, audio or keys beyond what
  [docs/security.md](docs/security.md) allows. Tests never use real tokens; fixtures say they are
  test data.
- Code for one operating system goes in a `windows_*`, `linux_*` or `macos_*` module behind the
  existing module API, so the rest stays testable everywhere.

## Pull requests

1. Branch from `master` and keep each pull request to one change.
2. Fill in the template: what changes, the evidence table, the failure message for new tests.
3. Write commit messages that describe the change in plain words, with no AI attribution lines.
4. `master` merges only when the Tier 1 checks pass: `tier1 windows-x64`, `tier1 linux-x64`,
   `ui windows-x64`, `ui linux-x64` and `dependency policy`. The macOS and ARM jobs (Tier 2)
   report without blocking; say in the pull request if your change turns one red.

Decisions that need explaining go in the pull request as
`Ruling: <what> — <why> — <cost if wrong>`; lasting ones go in [PLAN.md](PLAN.md) or a new
[decision record](docs/adr/).

## Issues

Use the bug report or flaky test form. Report security problems privately, as
[SECURITY.md](SECURITY.md) explains, never in a public issue.

## License

Onda is licensed under [Apache-2.0](LICENSE). Contributions you submit are licensed under the
same terms, as section 5 of the license says.
