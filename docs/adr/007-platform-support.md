# ADR 007: Platform support

Status: accepted · Date: 2026-11-13

## Tiers

| Tier | Platforms | Promise | Checked |
|---|---|---|---|
| 1 | windows-x64, linux-x64 | green for every merge | every PR: L0 to L6; installers (L7) on `master` |
| 2 | macos-arm64, windows-arm64, linux-arm64, macos-x64 | green for every release | every PR as an advisory job while being ported, nightly, release: the levels that exist on each |
| 3 | Fedora, Debian and Arch containers (deb, AppImage) | advisory | release |

Windows 10 22H2 x64 stays the documented minimum (README) and is covered by windows-x64 builds;
the runners test Windows Server 2022. 32-bit systems are not supported, by decision: no user asked
and the speech workers target 64-bit.

## What macOS needs before it can rise

macOS starts in Tier 2 because Onda has no macOS code yet. The port adds: an event-tap shortcut
helper in `onda-input`; Accessibility-based focused-field identity, password refusal and Cmd+V
paste in `onda-platform`; Keychain storage for API keys, accounts and the history key; launch at
login; the menu-bar icon; microphone permission text; a Metal speech worker; and a dmg.

## Moving between tiers

- macos-arm64 rises to Tier 1 when its L1 to L6 suites have been green on `macos-latest` for a
  week. Dictation into other apps stays on the Unverified list in `VERIFICATION.md` until it is
  checked on a real Mac, because hosted runners cannot grant Accessibility or Input Monitoring.
- windows-arm64 and linux-arm64 rise to Tier 1 when their suites have been green for two weeks.
- macos-x64 stays Tier 2 while GitHub offers an Intel macOS runner (`macos-15-intel`); when it is
  retired, the platform becomes "build only, unverified".
- Any platform drops a tier when its runner is retired or its suite stays quarantined for more
  than two weeks.

## Cost

Public repository: standard hosted runners, including macOS and ARM, are free, so the constraint
is wall clock, not money. Per PR: two Tier 1 jobs (budget 20 minutes, which moves the NSIS build
and installer test to `master` and releases) plus the advisory Tier 2 matrix. Per release: every
tier, installers on all three OS families.
