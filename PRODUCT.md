# Onda

Onda lets people dictate into the desktop app they already have open. It recognizes speech
locally. People can enable an AI provider for polishing a dictation or rewriting a selection.

## User journeys

- Hold the dictation shortcut, speak and release it. Onda returns the words to the original
  editable field or keeps the result available for recovery.
- Teach a name, maintain replacements and snippets, and restore a dictionary from a backup.
- Preview a rewrite or apply it to a selected passage, with a choice of provider and preservation
  rules for links, numbers and other protected text.
- Choose a microphone and speech model, control retention, revisit recent results, and share
  content-free activity statistics.

## Product commitments

The product specification defines the shortcut on each OS, recording limits, focus and
cancellation behavior, storage, and the conditions for counting a dictation. It also defines
which operations may send text to the selected provider. [Product behavior](docs/product-behavior.md)

Windows and Linux x64 are Tier 1. Windows and Linux ARM64 and both macOS architectures are
Tier 2. The additional Linux distributions have advisory checks. [Platform decision](docs/adr/007-platform-support.md)

Each critical journey has named automated checks and explicit manual cases. One release
review covers the fixed Keel probes and the numerical and privacy promises in the product
specification. Evidence belongs in the dated ledger. [Testing strategy](TESTING.md) · [Verification](VERIFICATION.md)
