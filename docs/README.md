# Where to read what

New here? Read [CONTRIBUTING.md](../CONTRIBUTING.md), then [ARCHITECTURE.md](../ARCHITECTURE.md).
[AGENTS.md](../AGENTS.md) holds the exact commands.

## The product

- [product-behavior.md](product-behavior.md): what Onda does: dictation, the dictionary and snippets, transforms and spoken editing, the profile, AI providers, privacy and failures.
- [DESIGN.md](../DESIGN.md), [frontend-handoff.md](frontend-handoff.md) and [design/](design/): the visual design and how the interface is built.
- [adr/](adr/): decision records, from the runtime and isolation (001) to platform support (007).

## Testing and evidence

- [TESTING.md](../TESTING.md): what each level proves, where it runs, the critical journeys.
- [VERIFICATION.md](../VERIFICATION.md): what has been proven, on which platform, by which run.
- [PLAN.md](../PLAN.md): the slices of work, their status and the rulings made along the way.
- [TEST-AUDIT.md](TEST-AUDIT.md): the audit the plan came from.
- [testing.md](testing.md): the manual release matrix and the opt-in tests (real models, desktop).
- [benchmarks.md](benchmarks.md): measured speed and quality, with the hardware.

## Platforms

- [windows-integration.md](windows-integration.md): microphone capture, the focused window and field, the clipboard and typing, the keyboard helper and the WebView2 processes on Windows.
- [linux.md](linux.md): what works on Linux, how to build and run it, and what is not checked.
- macOS (experimental): the Mac sections of [product-behavior.md](product-behavior.md) and [security.md](security.md), signing in [release.md](release.md), [ADR 007](adr/007-platform-support.md) and slices 11 to 15 in [PLAN.md](../PLAN.md).

## Speech, text and AI

- [asr.md](asr.md): local speech recognition and the model files.
- [dictionary.md](dictionary.md): vocabulary, replacements, normalization and correction learning.
- [ai-providers.md](ai-providers.md) and [local-models.md](local-models.md): the AI providers for polish and rewrites.
- [openai-sign-in.md](openai-sign-in.md) and [optional-chatgpt.md](optional-chatgpt.md): signing in with ChatGPT.

## Security, release and licenses

- [security.md](security.md): what Onda handles, where it may go, and the threat boundaries. Reports: [SECURITY.md](../SECURITY.md).
- [release.md](release.md): building, signing and shipping on each platform.
- [third-party-licenses.md](third-party-licenses.md) and [dependency-inventory.json](dependency-inventory.json): what Onda ships and under which licenses.

## Research and provenance

- [references.md](references.md): the sources read and the engineering and licensing decisions
  they led to.
