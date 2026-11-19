---
name: verify-onda
description: Verify an Onda checkout or pull request using its real speech workers, browser journeys, desktop IPC, hosted installers and Keel verification ledger.
---

# Verify Onda

Read the affected journey in `TESTING.md` and the platform tiers in `keel.json`. Run the commands from the repository root. On Windows, use native PowerShell and dot-source `./scripts/toolchain.ps1`. Install the locked frontend with `npm ci`. Prepare the genuine sidecars with `./scripts/prepare.ps1` on Windows or `scripts/prepare.sh` on Linux and macOS.

- For source or contract changes, use `npm run lint`, `npm run contracts:check`, `npm run test:unit` and the affected Cargo suite. The manifest lists the workspace commands.
- For renderer changes, run `npm run test:ui` and `npm run test:production-ui`. The first uses an identified mock in Chromium and WebKit. The second loads the shipped renderer with the bridge absent. New pixel baselines come from the snapshots workflow.
- For desktop behavior, build with `npm run build:e2e` and drive the executable with `npm run test:e2e`. A single WDIO invocation owns one isolated profile across its process-restart cases. It writes screenshots under `artifacts/e2e`. A subsequent run prepares its own fixture. Use the pinned public-domain WAV and verified model for automated speech transcription.
- For packages, `npm run package:desktop` and `npm run test:installers` select the host implementation. The hosted installer workflow runs six native packages and three additional Linux distributions. Its `upgrade_from` option uses an existing public release and its published digests.

Use `npm run triage -- <run-id>` to read actual job and step outcomes. Inspect the native contact sheet and any visual diffs. Record the platform, revision, test level and run in `VERIFICATION.md`. Label physical microphones, third-party target applications, live accounts and signing according to the evidence in the release matrix.
