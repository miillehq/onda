# Product behavior

Eagle operates from the tray. Closing the main diagnostics window hides it; quitting cancels work and releases native handles. A second launch opens the existing window rather than registering another hook. Branding's canonical desktop identity is `src-tauri/tauri.conf.json`; the core build reads its product name/identifier and verifies its version matches Cargo.

## Default dictation

1. Focus a supported editable control and hold Ctrl + Win + Space.
2. Eagle snapshots the exact target, starts the selected/default microphone and records locally. Maximum duration is 180 seconds by default, configurable up to 600.
3. Release the shortcut. Recording is finalized and local ASR runs using the chosen profile and language.
4. Conservative local whitespace/filler normalization, exact vocabulary/replacements and snippets produce usable text. Meaningful repetition stays intact; protected URL/email/number/code spans are preserved.
5. If text enhancement was explicitly enabled, send transcript and permitted bounded context through the active ChatGPT profile. Invalid output, quota, loss of connection and timeout fall back to local text.
6. Revalidate the target and paste. If insertion cannot be confirmed safely, show a recovery result instead of losing the text.

A new dictation is refused while an operation is processing. A failed dictation that did not reach its target does not block the next one: its text is kept in History (and offered for copying on the pill's error card), and retrying its insertion remains available while it is the current result. Escape cancels an active operation, with no global interception while idle. Cancellation prevents future stages; input already accepted by Windows cannot be recalled. Focus loss never silently retargets insertion. A failed or cancelled recording is discarded; no transcript can be recovered from ASR that never produced one.

Push-to-talk uses genuine hook key-down/key-up events. A held trigger produces one start, ignores auto-repeat and ends on release; a timer never infers release from the asynchronous state of the consumed key. Escape and desktop loss cancel the operation while keeping the held trigger latched until release, so a held key cannot restart recording. Hook reinstallation is restricted to fully idle operations and keys. A rare missed physical release requires releasing the trigger again or replacing the helper; the configured recording cap remains the bound on capture.

An empty recognized transcript reports **No speech was recognized**, with microphone-level guidance. Missing selection for a transform or spoken edit reports **Select text** before recording/cloud work. Empty preview input, oversized text and invalid null characters have separate errors. A short or empty speech result is never described as exceeding 128 KiB.

Fast is the initial profile because measured generic CPU latency matters more than model size. Automatic language detection uses a multilingual model. Selecting `en` permits the corresponding English-only model. Balanced/Best quality increase ASR cost; advanced model IDs are explicit catalog choices. Auto probes appropriate discrete Vulkan hardware when the optional worker exists. CPU only never probes it. GPU preferred may try supported integrated adapters and transparently falls back to CPU.

Downloading a model installs its asset without changing the selected profile. Settings › Speech and language shows the native resolved selection separately from installation status. Choosing a quality card that is not installed downloads it and then selects it; **All models › Use** beside an installed model saves an explicit Custom selection. If the resolved model is missing, recording fails before microphone access with the model name and instructions to download it or select an installed model. The operation status displays the full error.

## Dictionary and snippets

Vocabulary communicates authoritative spelling and supplies relevant ASR/AI context; it is not a promise of phonetic recognition. Replacement rules supply deterministic recurring corrections. The compiled matcher is longest-match-first with Unicode boundaries, canonical/case-insensitive lookup, protected spans and no cascading within a pass. Snippets expand exact triggers in a separate deterministic pass. Duplicate normalized triggers are rejected; imports skip existing triggers and validate the archive before committing. Entry source metadata distinguishes manual/imported/auto-learned records. Edit/disable/delete applies immediately.

Learning is off by default. When enabled, the app observes a bounded focused-field scope after its own insertion at most three times over twelve seconds. A candidate must be one distinctive, nearby spelling change inside the uniquely identified inserted text; all other scope text must be unchanged. Common-word rewrites, protected tokens and ambiguous changes are ignored. The app proposes a correction and requires the user's Add correction action to save it. No keylogging, background scraping or automatic dictionary poisoning occurs.

## Transforms and spoken editing

Ctrl + Alt + P transforms the selection using the chosen default transform. Built-ins cover polish, grammar, clarity, concision, formality, casual tone, bullets, English translation and prompt improvement. Users can edit instructions, create transforms, assign a model preference and add a custom chord. Only configured actions are sent from the helper. A transform may explicitly disable protected-token preservation when its purpose needs that freedom; ordinary dictation never disables it.

Ctrl + Alt + C records an editing instruction and applies its local transcript to the selection captured at recording start. The selection remains untrusted data. The locally transcribed command remains in memory as commandText, and cloud failure retains the instruction for retryTransformation without another recording. The diagnostics panel also runs a typed-text transform preview without inserting it into another app.

A failed cloud transformation can retry its frozen instruction/model policy while preserving its original. Preview retries remain previews; selection retries revalidate the original selection after a minimum three-second focus-return opportunity. Completed inserted transforms require a new user selection to avoid duplicate writes.

The result includes original/output and a linear Unicode-safe prefix/suffix diff. Copy intentionally changes the clipboard because the user requested it. Retry paste and Undo give three seconds to return focus to the original control, then revalidate. Undo selects a uniquely identifiable inserted range only if the bounded post-paste document is unchanged. Duplicated text, intervening edits or inaccessible controls refuse undo. Eagle does not issue blind Ctrl+Z.

## Privacy and failures

Audio is ephemeral process memory/pipes. Default settings disable context, learning and enhancement. Dictionary/settings/snippets/transforms stay in SQLite; tokens stay DPAPI protected. Dictation history is encrypted per Windows user and kept for the configured retention (default 3 days; 0 keeps it only until Onda quits). There is no remote telemetry. Diagnostics use an allowlist of versions, hardware evidence, preference modes and content-free timings. Local performance metrics do not contain transcript text.

UIA-inaccessible controls, secure fields, elevated targets, unsupported clipboard ownership formats and changed selections fail visibly. The backend reports input acceptance and a delayed-render read receipt, which cannot cryptographically prove that a target application edited its document. The cross-app matrix in testing.md remains the distribution gate. RDP, sleep/resume, Electron/terminal variants and a real eligible ChatGPT authorization require interactive certification.

Recoverable audio discontinuities keep the microphone stream active and increment a content-free diagnostic counter. Real device loss stops capture; a changed or invalidated stream asks the user to select an input and retry. Permission and driver failures have their own recovery messages. A recording never silently follows a newly routed microphone. The level test reports signal statistics and discontinuities without saving or transcribing audio.
