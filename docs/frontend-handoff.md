# Frontend handoff

Read architecture.md and product-behavior.md first, then this file, security.md, windows-integration.md and openai-sign-in.md, and [DESIGN.md](../DESIGN.md) for the visual system. The Onda interface in `ui/` is Preact with signals: `ui/src/lib/store.ts` holds native state and actions, `ui/src/views/` the pages, `ui/src/pill/` the dictation pill. No dictation, OAuth, replacement, persistence, validation or insertion policy belongs in the frontend.

## Windows and their responsibilities

- `main` (`ui/index.html`): first-run setup, Home, Dictionary, Snippets, Rewrites and Settings. Its window theme follows `settings.theme`; native code tints the title bar to match (`apply_theme`).
- `pill` (`ui/pill.html`): created unfocusable and click-through, shown and hidden only by native code (`src-tauri/src/pill.rs`) with non-activating Win32 calls, so dictation never loses its target. The webview only draws state from events. It becomes clickable only for the error card, whose actions call `openMainWindow { section }`, `copyResult`, `copySuggestion` or `dismissError`. Previews and recovery started from the focused main window do not show it.
- Start, stop and error cues play natively when `settings.playSounds` is on.

## Contract

Rust derives TypeScript with ts-rs 12. Generated files are `ui/src/generated/core.ts` and `api.ts`. `npm run contracts` regenerates them; `contracts:check` detects drift. JSON wire properties are camelCase. Enum tags are string discriminants, optional properties are represented explicitly by null, and byte counts/durations are JSON numbers. `contractVersion` is currently 1.

Use `call(request)` from `ui/src/api.ts`. It invokes the single ACL-scoped native facade `application_call` with a tagged `ApiRequest`; response is a tagged `ApiResponse`. Rejections are `{code,message,retryable}`. Do not infer success from promise resolution of a start command: background lifecycle arrives through events.

Snapshot `selectedModelId` is the native resolution of the current quality profile and language, or null for an invalid legacy custom choice. Show this independently from `installedModels`; downloads do not select a model. Selecting an installed catalog row saves `qualityProfile: "custom"` and its `modelId` through `updateSettings`. Refresh the snapshot and model panel after selection or settings changes. Render `state.error.message` beside the phase so shortcut failures remain actionable.

```typescript
await call({command: 'startModelDownload', args: {modelId: 'whisper-tiny'}});
await call({command: 'startChatgptSignIn', args: {profileId: null}});
await call({command: 'saveDictionaryEntry', args: {entry: {
  id: crypto.randomUUID(), kind: 'vocabulary', phrase: 'OpenAI',
  replacement: null, enabled: true, starred: true, source: 'manual'
}}});
```

| Area | Commands | Behavior |
|---|---|---|
| Initial state | getSnapshot, getSettings, updateSettings | Snapshot includes version, hardware, settings, operation, memory result, installed models, safe account metadata and optional cloudError. Cloud storage errors must leave local controls usable. Settings validate atomically; changes cancel current operation and replace helper configuration. A changed modelDirectory performs verified migration before switching. |
| Dictionary | getDictionary, saveDictionaryEntry, deleteDictionaryEntry, importDictionary, exportDictionary | Save is an upsert by ID; duplicate normalized triggers error. Archive import is versioned and all-or-nothing. Search is local presentation over returned entries. |
| Snippets | getSnippets, saveSnippet, deleteSnippet | Trigger expansion is native deterministic logic. |
| Transforms | getTransforms, saveTransform, deleteTransform, previewTransform, transformSelection | Custom instruction/chord/model preference validated natively. Preview returns a result without native insertion. Global shortcuts capture the selection before cloud work. |
| Dictation | startDictation, stopDictation, cancelOperation | UI-start targets the currently focused native control; prefer configured shortcuts for another app. commandMode selects spoken editing. |
| Models | getModelCatalog, getInstalledModels, startModelDownload, cancelModelDownload, uninstallModel | Never reconstruct model paths/URLs in UI. Installation excludes partial files. Download progress is asynchronous; unsubscribe cleanly. |
| Microphone | listAudioDevices, testMicrophone | Endpoint IDs are opaque. Null settings microphone uses system default. Level test is 0.5–10 seconds, refuses overlapping recording. |
| ChatGPT | startChatgptSignIn, enableChatgptPlanUsage, cancelChatgptSignIn, listChatgptProfiles, selectChatgptProfile, signOutChatgptProfile, removeChatgptProfile, listAvailableModels, setChatgptModel | Use the approved Continue with ChatGPT label. Tokens/OAuth URLs never enter renderer. Disabled plan permission needs an explicit reconsent action. Show unconfirmed remote revocation after local sign-out. Account models are rediscovered; preference is a slug. `CloudModel.recommended` marks the model used when none is chosen. |
| Recovery | retryResult, retryTransformation, undoLastTransform, copyResult, copySuggestion, dismissError, clearResult | Retry paste/undo start a three-second focus-return window. retryTransformation reuses the frozen native instruction and original input after cloud failure; commandText retains a spoken editing instruction. Already inserted transforms cannot be retried in place. Never steal focus or submit a message. Undo requires unchanged document/unique output. Memory result may be discarded. `copySuggestion` copies ChatGPT's version that failed the checks (`OperationResult.suggestion`); `copyHistorySuggestion` does the same for a History entry. `dismissError { operationId }` returns that failed operation to idle, keeping its result. |
| Learning | acceptLearning | Proposals are ephemeral and must be approved by the user. Never auto-submit acceptance. |
| Support | exportDiagnostics, checkUpdate, installUpdate | Diagnostics are redacted native JSON. Update buttons must be disabled unless updaterConfigured is true. Installation requires a verified signature and idle operation. |
| Usage | getUsage | Content-free daily counts (`dictations`, `words`, `audioMs`) keyed by local day, newest first, with the native `today`. Words are counted from the local transcript. |
| Teaching | teachWord | One 1–6 s take (the UI uses 2.5 s) transcribed exactly as dictation would, returning `heard` (trimmed) and `written` (after the dictionary). Refused while dictating or testing the microphone; `noSpeech` when silent; `noModel` before recording when no model is installed. Streams `audioLevel` while recording. Never inserts text or writes history. |
| History | getHistory, copyHistoryEntry, deleteHistoryEntry, clearHistory | Newest first. Entries are decrypted natively; copy goes through the native clipboard. `errorCode` with `inserted: false` means the text never reached its app. Retention is `settings.historyDays` (0 = until quit, max 30). Deleting the current result's entry also clears the result when idle. |
| Windows | openMainWindow, dismissPill | `section` is a route such as `settings/microphone`; native code emits `navigate` after showing the main window. |
| Desktop settings | updateSettings | `theme`, `launchAtLogin` (per-user Run key, `--background`), `showPill`, `playSounds` and `onboardingCompleted` apply natively on save. |

## Events and operation states

Subscribe using `onEvent`. The event channel is `onda://event` and envelopes are typed `RuntimeEvent`. `domain` wraps an `AppEvent`: stateChanged, resultReady, learningProposed, metric, error, cloudDelta. Other envelopes are downloadProgress, oauthChanged, shortcutStatus, audioLevel (short-window RMS about 15–20 times a second while the microphone is open, for recording and the microphone test), navigate, historyUpdated (one entry added or changed) and historyReset (fetch the list again). `OperationState.handsFree` is true when the toggle shortcut started the recording.

```json
{"type":"domain","payload":{"type":"stateChanged","payload":{"operationId":"opaque-uuid","phase":"transcribing","mode":"dictation","route":"local","error":null,"fallbackUsed":false}}}
```

```
idle/terminal → recording → finalizingAudio → transcribing
                                          → enhancing? → inserting → completed
any active phase → cancelled/error
selection transform → enhancing → inserting → completed
typed preview → enhancing → completed
```

Only one active operation exists. A cancelled generation's callbacks must never replace a newer UI result; key state and result rendering by operationId. cloudDelta is provisional and may be rejected after stream completion: display it distinctly from the validated final result. Clear provisional UI on a new operation. Routes are local/chatgptPlan and may return to local on fallback. `fallbackUsed` is a first-class reliability indicator.

Downloads have their own modelId and downloading/verifying/installed stages. CompletedBytes/totalBytes represent byte progress; verification may take time at 100% download. OAuth authorizing/connected/failed/selected/signedOut and helper starting/ready/recovering/unavailable indicate recoverable loading/error states. Refresh the snapshot when connecting/selecting an account or installing models. Never poll native context as part of a frontend render or interval.

## Accessibility and security

Use labelled controls, keyboard navigation, visible focus, polite live regions for phase/progress, and an assertive actionable error surface where useful. Announce recording start/stop without repeatedly speaking token deltas. Keep shortcuts configurable and reserve Escape for active cancellation. Do not make a non-activating dictation indicator steal the target app's focus.

Treat all text, imported JSON, account/model display names and error bodies as text, never HTML. The backend already redacts OS/network errors; do not log selected text or transcript contents. Never persist text in WebView storage, implement a parallel fetch client, expose shell/filesystem plugins, handle OAuth secrets, choose billable providers, paste directly or accept provisional streams as final.

Microphone level reports include a scalar `discontinuities` count. A recoverable notification is not a disconnect; the native service keeps recording. Render the distinct `audioDisconnected`, `audioRouteChanged`, `audioPermissionDenied` and `audioBackend` messages without guessing their causes from device names. Do not select or switch input endpoints automatically.

Render `noSpeech` as a microphone/recognition recovery case, and `selectionRequired` as a prompt to select target text. Do not conflate them with text-size validation. Keep the current operation state when applying `shortcutStatus` events; `ready` following desktop restoration is availability, not a new recording.

The diagnostics UI's editable JSON is deliberately primitive. A polished editor should retain typed native validation and explicit privacy settings, with onboarding that downloads a usable local model before the first recording. Preserve the original and result on cloud failure and make Copy/Retry discoverable. Final layout, animation, CSS and icon design remain frontend work.
