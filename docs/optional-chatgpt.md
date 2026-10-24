# Optional ChatGPT availability and default model selection

Local dictation initializes independently of ChatGPT. The runtime retains an optional native `ChatGptService` and a redacted initialization error. Unavailable protected account storage leaves local recording, transcription, dictionary, snippets, settings, and insertion available. An explicit cloud operation returns a recoverable error; ordinary dictation enhancement preserves its local transcript when that operation fails.

## Startup and account metadata

`ChatGptService::new` creates the account directory and HTTP client without contacting OpenAI or reading every stored profile. Account/host metadata is loaded lazily. Successful construction therefore does not prove that a later profile read will succeed.

The runtime snapshot handles initialization and metadata-read failures as optional subsystem errors. It returns an empty account list plus `cloudError` while retaining its local capabilities. The UI can show that error beside ChatGPT account controls without rejecting the entire snapshot or implying that account storage was repaired.

Current `list_profiles` deliberately fails the whole listing when host metadata or any protected registration is unreadable. This preserves a clear fail-closed boundary. Other registrations remain on disk. Account selection never silently changes to another registration. A future partial listing would need an explicit warning contract and must preserve the selected account identity even if its record cannot be read.

Only a missing host file creates a new stable host identifier. Invalid host JSON, unsupported metadata, malformed registration, or DPAPI failure does not trigger regeneration, deletion, replacement, or an automatic fresh registration. Repair storage permissions or restore a backup using the same Windows account, then retry. Preserve damaged files for diagnosis. When credentials cannot be read for remote revocation, the user can disconnect the relevant app registration in ChatGPT settings before explicitly replacing local account storage.

Errors exposed to the renderer are typed and redacted. They contain neither token values, protected-record bytes, local filesystem paths, nor parser excerpts. Token records remain native DPAPI data. An error warning is information about availability, never authority to overwrite credentials.

## Choosing an account model

The selected account's current catalog supplies model slugs, display names, and ordering. The application chooses a model in this order:

1. The transform's explicit preferred slug, when available.
2. The saved account slug, when available.
3. The first model in server order whose display name contains the complete alphanumeric word `fast`, `mini`, or `nano`, matched case-insensitively.
4. The first model returned by the account catalog.

The hint words are centralized in `FAST_MODEL_NAME_HINTS`; no model slug is hard-coded. A missing explicit preference still allows a valid saved account preference. Whole-word matching avoids treating unrelated names such as “Breakfast” or “Administration” as speed labels. An empty catalog remains an actionable error and ordinary dictation keeps its local transcript.

Display-name hints are a selection heuristic. Actual latency comes from first-token and completion measurements for the selected slug. Server names, eligibility, ordering, and access can change. Automatic fallback does not overwrite a user's persisted preference; a later catalog may restore that choice. [Official account model discovery](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference).

## Storage-failure verification

`crates/onda-cloud/src/oauth/availability_tests.rs` exercises four local-only failure cases: an account-directory collision, corrupt host identity, an invalid DPAPI registration, and invalid client metadata. Tests assert redacted errors, unchanged host/profile bytes, preserved unrelated registrations, and no automatic account selection. They use test-only token sentinels and require no live account, browser, or OpenAI request.

The facade smoke harness exercises a fresh profile without an authorized cloud account, actionable cloud errors, local controls, and retention of the original text after a failed transform. Corrupt-storage byte preservation is covered by the native tests above. The release smoke matrix should also exercise an unavailable account store through the actual facade. A real browser consent, eligible account, serving policy, and live inference still require explicit release validation.
