# Product behavior

Onda runs from the tray (the menu bar on a Mac). Closing the main window closes it and dictation keeps working; quitting from the tray cancels work and releases native handles. A second launch opens the existing window rather than starting another copy. Branding's canonical desktop identity is `src-tauri/tauri.conf.json`; the core build reads its product name/identifier and verifies its version matches Cargo.

## Default dictation

1. Focus a supported editable control and hold Ctrl + Win + Space (Ctrl + Super + Space on Linux, where the desktop may let the user pick other keys; Control + Shift + Space on a Mac, where Control + Command + Space opens Emoji & Symbols).
2. Onda snapshots the exact target, starts the selected/default microphone and records locally. Maximum duration is 180 seconds by default, configurable up to 600.
3. Release the shortcut. Recording is finalized and local ASR runs using the chosen profile and language.
4. Conservative local whitespace/filler normalization, exact vocabulary/replacements and snippets produce usable text. Meaningful repetition stays intact; protected URL/email/number/code spans are preserved.
5. If AI polish was explicitly enabled, send the transcript and permitted bounded context to the chosen provider. Invalid output, quota, loss of connection and timeout fall back to local text.
6. Revalidate the target, read the two characters before the caret, and paste. A dictation that would run into earlier text (after a period, word or closing quote) gets a leading space; none is added after whitespace, an opening bracket or quote, between Chinese, Japanese or Thai characters, or when the text starts with attaching punctuation. A control that exposes no caret gets the text as dictated. If insertion cannot be confirmed safely, show a recovery result instead of losing the text.

On Linux, step 6 pastes once the user allows typing in Settings › Shortcuts › Typing into apps; without that permission, fields that accept edits through accessibility get the text inserted and other apps get it on the clipboard ([linux.md](linux.md)).

On a Mac, step 6 needs the Accessibility permission. Settings › Shortcuts › Typing into apps has an Allow button: macOS shows its own dialog once and Onda opens Privacy & Security › Accessibility in System Settings, where the user turns Onda on; the page checks again when its window gets focus back. Only System Settings turns it off, so the button then reads Open System Settings. With the permission, Onda reads which app, window and field have focus, refuses password fields and anything typed while macOS's secure keyboard input is on (password fields and Terminal's Secure Keyboard Entry turn it on), puts the text on the pasteboard, presses ⌘V and puts back the text the pasteboard held. An app that shows Onda no field, as many Electron and Chromium windows don't, is typed into only while the same app and window stay in front, and its paste cannot be confirmed. App profiles' paste keys are Windows and Linux keys; a Mac pastes with ⌘V everywhere. Without the permission, dictation is copied for ⌘V. Because the dmg is unsigned, macOS ties the permission to one build, and an update can need Onda switched off and on again there.

A new dictation is refused while an operation is processing. A failed dictation that did not reach its target does not block the next one: its text is kept in History (and offered for copying on the pill's error card), and retrying its insertion remains available while it is the current result. Escape cancels an active operation, with no global interception while idle (on Linux, Esc together with the dictation modifiers). Cancellation prevents future stages; input the OS has already accepted cannot be recalled. Focus loss never silently retargets insertion. A failed or cancelled recording is discarded; no transcript can be recovered from ASR that never produced one.

A hold-to-talk recording under 0.6 seconds with no speech is treated as a tap: the pill says "Hold to talk" and to keep the keys held, rather than pointing at the microphone. Onboarding's Try it step shows the keys going down, staying down while the pill listens, and coming back up. If Onda cannot type the words, they go on the clipboard and the pill says to press Ctrl+V (⌘V on a Mac); the pill's error card keeps Dismiss in its corner so its one or two actions always fit. On Windows, push-to-talk uses genuine hook key-down/key-up events. A held trigger produces one start, ignores auto-repeat and ends on release; a timer never infers release from the asynchronous state of the consumed key. Escape and desktop loss cancel the operation while keeping the held trigger latched until release, so a held key cannot restart recording. Hook reinstallation is restricted to fully idle operations and keys. A rare missed physical release requires releasing the trigger again or replacing the helper; the configured recording cap remains the bound on capture.

An empty recognized transcript reports **No speech was recognized**, with microphone-level guidance. Missing selection for a transform or spoken edit reports **Select text** before recording/cloud work. Empty preview input, oversized text and invalid null characters have separate errors. A short or empty speech result is never described as exceeding 128 KiB.

Fast is the initial profile because measured generic CPU latency matters more than model size. Automatic language detection uses a multilingual model. Selecting `en` permits the corresponding English-only model. Balanced and Quality increase ASR cost; advanced model IDs are explicit catalog choices. Auto probes appropriate discrete Vulkan hardware when the optional worker exists; on a Mac the worker uses Metal. CPU only never probes it. GPU preferred may try supported integrated adapters and transparently falls back to CPU.

Downloading a model installs its asset without changing the selected profile. Settings › Speech and language shows the native resolved selection separately from installation status. Choosing a quality card that is not installed downloads it and then selects it; **All models › Use** beside an installed model saves an explicit Custom selection. If the resolved model is missing, recording fails before microphone access with the model name and instructions to download it or select an installed model. The operation status displays the full error.

## Dictionary and snippets

Vocabulary communicates authoritative spelling and supplies relevant ASR/AI context; it is not a promise of phonetic recognition. Replacement rules supply deterministic recurring corrections. The compiled matcher is longest-match-first with Unicode boundaries, canonical/case-insensitive lookup, protected spans and no cascading within a pass. Snippets expand exact triggers in a separate deterministic pass. Duplicate normalized triggers are rejected; imports skip existing triggers and validate the archive before committing. Entry source metadata distinguishes manual/imported/auto-learned records. Edit/disable/delete applies immediately.

Learning is off by default. When enabled, the app observes a bounded focused-field scope after its own insertion at most three times over twelve seconds. A candidate must be one distinctive, nearby spelling change inside the uniquely identified inserted text; all other scope text must be unchanged. Common-word rewrites, protected tokens and ambiguous changes are ignored. The app proposes a correction and requires the user's Add correction action to save it. No keylogging, background scraping or automatic dictionary poisoning occurs.

## Transforms and spoken editing

Ctrl + Alt + P (Control + Option + P on a Mac) transforms the selection using the chosen default transform. Built-ins cover polish, grammar, clarity, concision, formality, casual tone, bullets, English translation and prompt improvement. Users can edit instructions, create transforms, assign a model preference and add a custom chord. Only configured actions are sent from the helper. A transform may explicitly disable protected-token preservation when its purpose needs that freedom; ordinary dictation never disables it.

Ctrl + Alt + C records an editing instruction and applies its local transcript to the selection captured at recording start. The selection remains untrusted data. The locally transcribed command remains in memory as commandText, and cloud failure retains the instruction for retryTransformation without another recording. The Rewrites page also tries a transform on typed text without inserting it into another app.

When the provider's text fails the preservation checks, Onda keeps the original (typing it for dictation, leaving the selection for a rewrite) and keeps the polished version beside it: the pill offers Copy polished version, Home shows it marked against the kept text, and History keeps it behind Show polished version. Dismissing an error on the pill or Home acknowledges it, so the sidebar and tray read Ready again; its text stays in History. A failed cloud transformation can retry its frozen instruction/model policy while preserving its original. Rewrites tried on the Rewrites page, and retries of any rewrite, are kept in History like other edits. Preview retries remain previews; selection retries revalidate the original selection after a minimum three-second focus-return opportunity. Completed inserted transforms require a new user selection to avoid duplicate writes.

The result includes original/output and a linear Unicode-safe prefix/suffix diff. Copy intentionally changes the clipboard because the user requested it. Retry paste and Undo give three seconds to return focus to the original control, then revalidate. Undo selects a uniquely identifiable inserted range only if the bounded post-paste document is unchanged. Duplicated text, intervening edits or inaccessible controls refuse undo. Onda does not issue blind Ctrl+Z. Undo is not available on macOS yet.

## Profile and statistics

A profile name (up to 40 characters, optional) and picture (PNG, JPEG or WebP up to 10 MB, stored as
a 256 px PNG) are kept locally. Home greets by the first word of the name ("Good afternoon, Daniel"),
or just "Good afternoon". Stats counts only content-free numbers:

- **What counts:** dictations Onda delivered (typed, or put on the clipboard where it cannot type).
  Edits of selected text, previews and dictations that were not delivered do not count. Words are
  counted in the local transcript, before AI polish.
- **Days:** the local calendar day when the dictation was delivered, as recorded then. A later time
  zone change does not move earlier days.
- **Streak:** consecutive days with at least one dictation, ending today, or yesterday when nothing
  has been dictated yet today. Longest streak and active days look back over the last 400 days that
  had dictation; lifetime totals never expire.
- **Pace:** words divided by minutes of recorded audio, including pauses, shown after one minute.
- Days before Onda kept daily totals are not shown or estimated.

Share stats renders a 1200×630 PNG with those numbers and the activity grid. Name and picture are
off until switched on, and native code asks where to save it.

## AI providers

AI polish, rewrites and spoken edits use exactly one provider: ChatGPT (sign-in), an
OpenAI-compatible server, Anthropic, or a model server on this computer. A failure keeps the local
text, and Onda never moves to another provider. See [ai-providers.md](ai-providers.md) and
[local-models.md](local-models.md).

## Privacy and failures

Audio is ephemeral process memory/pipes. Default settings disable context, learning and enhancement. Dictionary/settings/snippets/transforms stay in SQLite; tokens and keys stay in the OS's protected storage (DPAPI, the desktop keyring or the Keychain). Dictation history is encrypted for the signed-in user and kept for the configured retention (default 3 days; 0 keeps it only until Onda quits). There is no remote telemetry. Diagnostics use an allowlist of versions, hardware evidence, preference modes and content-free timings. Local performance metrics do not contain transcript text.

Controls that accessibility cannot read, secure fields, elevated targets, unsupported clipboard ownership formats and changed selections fail visibly. On Windows the backend reports input acceptance and a delayed-render read receipt, which cannot prove that a target application edited its document. Real apps, Remote Desktop, sleep/resume and a real ChatGPT sign-in are checked by hand before a release ([testing.md](testing.md)).

Recoverable audio discontinuities keep the microphone stream active and increment a content-free diagnostic counter. Real device loss stops capture; a changed or invalidated stream asks the user to select an input and retry. Permission and driver failures have their own recovery messages. A recording never silently follows a newly routed microphone. The level test reports signal statistics and discontinuities without saving or transcribing audio.
