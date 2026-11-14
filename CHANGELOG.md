# Changelog

What changed in each Onda release, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/); 0.x releases are pre-releases. What was tested, on
which platform and by which run is recorded in [VERIFICATION.md](VERIFICATION.md).

## [Unreleased]

## [0.3.0] - 2026-11-14

The first release with installers for every platform: Windows and Linux on x64 and ARM64, and
macOS on Apple Silicon and Intel (experimental). The installers are not code-signed yet: on
Windows, SmartScreen asks once (**More info › Run anyway**); on a Mac, allow Onda once under
**System Settings › Privacy & Security › Open Anyway**. Check each download against
`SHA256SUMS.txt`.

### Added

- macOS support, experimental: Apple Silicon and Intel, the menu-bar icon, launch at login, and
  keys stored in the Keychain (#11).
- On a Mac, dictation starts with Control + Shift + Space, because Control + Command + Space opens
  Emoji & Symbols (#22). The interface uses Mac key names (Control, Option, Command) and Mac
  wording, and Esc alone cancels (#23).
- On a Mac, Onda types into the app you're using once you allow it under **Privacy & Security ›
  Accessibility**, from **Settings › Shortcuts › Typing into apps**. It refuses password fields and
  anything typed while secure keyboard input is on, and puts back what you had copied. Without the
  permission, dictation is copied for ⌘V. Experimental: it has not yet been tried on a real Mac
  (#28).
- Windows on ARM and Linux on ARM64 builds (#11), with installers for both, and for Intel Macs
  (#29).
- Each release lists SHA-256 checksums and carries the third-party notices and the source of the
  MPL-licensed components (#29).
- A security policy with private vulnerability reporting, and contributor and issue guidance (#27).

### Changed

- The README was rewritten with downloads, how Onda works, privacy and an FAQ (#31), and every
  other document now describes 0.3.0 (#32).

### Fixed

- The Stats page's activity grid no longer draws month labels over its first row of days (#30).
- On a Mac, Onda's icon is the brand kit's Mac icon, sized on Apple's icon grid like other apps,
  instead of filling the whole tile (#34).
- On a Mac, the Balanced and Quality speech profiles no longer fall back to the Fast models:
  Onda now reads how much memory the Mac has (#33).

### For contributors

- The project follows Keel's testing conventions: `keel.json`, a test strategy per level, a plan
  of slices and an evidence ledger (#2, #16, #20, #24, #26).
- CI runs in tiers: Windows and Linux x64 must pass; macOS, Windows on ARM and Linux on ARM64 run
  the same suites and report without blocking (#1, #3). The installers workflow installs,
  launches and removes every bundle on its own platform (#12, #17, #29).
- New suites: real-shell journeys in the debug app with WebdriverIO (#4), interface unit tests (#6),
  browser tests on chromium and webkit with structure, geometry and per-OS pixel baselines (#9, #18,
  #19), and contract examples shared by the development mock and the real backend (#5, #14).
- The test audit's gaps were closed with tests that were each shown to fail first (#7, #8, #10,
  #13, #15).
- The WebView2 smoke check retries once after a timeout and reports where the time went (#21, #26).
- On Windows on ARM, `npm run release` builds the installer: the speech worker now builds in a
  process of its own, so its ARM64 compiler setup no longer breaks the build steps after it (#29),
  and the installer carries Visual Studio 2026's C++ runtime where 2022 is not installed, as on
  GitHub's ARM runners (#36).
- A `v*` tag on `master` publishes a release from the installers that passed their tests on that
  commit, once the versions agree and all six installer jobs are green (#29).

### Known issues

- The installers are unsigned (see above). On a Mac, the unsigned app can lose its Accessibility
  permission after an update: switch Onda off and on again in that list.
- Mac typing into other apps has not yet been tried on a real Mac; GitHub's Mac runners check that
  it builds, starts and reports the permission. Undoing a dictation is not available on a Mac yet,
  and keyboard layouts that move the V key (such as Dvorak) are untested.
- The installers include the CPU speech engines only; GPU acceleration (Vulkan) needs a build from
  source.

## [0.2.0]

The first public source release, for Windows and Linux. No installers were published.

[Unreleased]: https://github.com/miillehq/onda/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/miillehq/onda/releases/tag/v0.3.0
[0.2.0]: https://github.com/miillehq/onda/tree/eaecdfd
