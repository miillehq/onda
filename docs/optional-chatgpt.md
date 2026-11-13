# Optional ChatGPT availability and default model selection

Local dictation initializes independently of ChatGPT. The runtime retains an optional native `ChatGptService` and a redacted initialization error. Unavailable protected account storage leaves local recording, transcription, dictionary, snippets, settings, and insertion available. An explicit cloud operation returns a recoverable error; ordinary dictation enhancement preserves its local transcript when that operation fails.

## Startup and account metadata

`ChatGptService::new` creates the account directory and HTTP client without contacting OpenAI or reading every stored profile. Account/host metadata is loaded lazily. Successful construction therefore does not prove that a later profile read will succeed.

The runtime snapshot handles initialization and metadata-read failures as optional subsystem errors. It returns an empty account list plus `cloudError` while retaining its local capabilities. The UI can show that error beside ChatGPT account controls without rejecting the entire snapshot or implying that account storage was repaired.

Current `list_profiles` deliberately fails the whole listing when host metadata or any protected registration is unreadable. This preserves a clear fail-closed boundary. Other registrations remain on disk. Account selection never silently changes to another registration. A future partial listing would need an explicit warning contract and must preserve the selected account identity even if its record cannot be read.

Only a missing host file creates a new stable host identifier. Invalid host JSON, unsupported metadata, malformed registration, or a protected-storage failure (DPAPI, keyring or Keychain) does not trigger regeneration, deletion, replacement, or an automatic fresh registration. Repair storage permissions or restore a backup using the same user account, then retry. Preserve damaged files for diagnosis. When credentials cannot be read for remote revocation, the user can disconnect the relevant app registration in ChatGPT settings before explicitly replacing local account storage.

Errors exposed to the renderer are typed and redacted. They contain neither token values, protected-record bytes, local filesystem paths, nor parser excerpts. Token records remain native protected data. An error warning is information about availability, never authority to overwrite credentials.

## Choosing an account model

The selected account's current catalog supplies model slugs, display names, and ordering. The application chooses a model in this order:

1. The transform's explicit preferred slug, when available.
2. The saved account slug, when available.
3. The first model in server order whose display name contains the complete alphanumeric word `fast`, `mini`, `nano` or `luna`, matched case-insensitively.
4. The first model returned by the account catalog.

The hint words are centralized in `FAST_MODEL_NAME_HINTS` in `src-tauri/src/runtime.rs` ("luna" is the light model of the GPT-6 family); no model slug is hard-coded. `recommended_model` applies the same rule for inference and for `listAvailableModels`, which marks that model `recommended`, so Settings and onboarding can show "Recommended (GPT-6 Luna)" instead of an unnamed default. A missing explicit preference still allows a valid saved account preference. Whole-word matching avoids treating unrelated names such as “Breakfast” or “Administration” as speed labels. An empty catalog remains an actionable error and ordinary dictation keeps its local transcript.

The runtime reuses an account's model list for 30 minutes and refreshes it while the user is still speaking, so polish does not wait on the catalog, a lookup that otherwise took one to four seconds before each request. Display-name hints are a selection heuristic. Actual latency comes from first-token and completion measurements for the selected slug. Server names, eligibility, ordering, and access can change. Automatic fallback does not overwrite a user's persisted preference; a later catalog may restore that choice. [Official account model discovery](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference).

## Storage-failure verification

`crates/onda-cloud/src/oauth/availability_tests.rs` exercises four local-only failure cases: an account-directory collision, corrupt host identity, an unreadable protected registration, and invalid client metadata. Tests assert redacted errors, unchanged host/profile bytes, preserved unrelated registrations, and no automatic account selection. They use test-only token sentinels and require no live account, browser, or OpenAI request.

The native smoke (`ui/src/smoke.ts`) exercises a fresh profile without an authorized cloud account, actionable cloud errors, local controls, and retention of the original text after a failed transform; on Windows, `scripts/smoke.ps1 -UnavailableCloudStorage` runs it with an unusable account store. Corrupt-storage byte preservation is covered by the native tests above. A real browser consent, eligible account, serving policy, and live inference are in the manual matrix ([testing.md](testing.md)).
