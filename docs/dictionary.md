# Dictionary, normalization, and correction learning

The dictionary has two independent concepts: vocabulary supplies canonical spelling, and replacement rules map an explicitly configured trigger to an output phrase. Snippets expand an explicit trigger into arbitrary text. All three work locally; cloud inference is optional. Persistence distinguishes manual, imported, and explicitly accepted auto-learned entries.

## Teaching a word by saying it

Speech models take spelling hints, not pronunciations: vocabulary only nudges Whisper towards a spelling when the sound is close. For a word spelled unlike it sounds ("Miille", said "meal-le"), the Dictionary offers **Say it to teach Onda** on each vocabulary word. Each take calls `teachWord`, which records 2.5 seconds from the selected microphone and transcribes it with dictation's own model, language, execution preference and vocabulary prompt (`Runtime::transcription_options`). It returns what the model heard and what the current dictionary would write. Takes that differ from the word become proposed replacement rules (heard → word), which the user can untick before saving. Audio is discarded after each take, the text is never inserted or added to History, and only the replacements the user saves are stored. A phrase that is already a vocabulary word is never turned into a replacement.

## Deterministic correction

`onda-core/src/dictionary.rs` compiles enabled triggers into Aho–Corasick indexes. Matching uses NFKC normalization, Unicode lowercase, and collapsed non-linebreak whitespace while mapping every normalized byte back to its complete source grapheme. Emitted spelling always comes from the configured expansion.

A match must consume complete graphemes. A partial normalized `f` match cannot consume a source `ﬃ` ligature, and a partial `i` match cannot consume `İ`. Combining marks remain part of word boundaries, preventing a `café` trigger from changing `caféteria`. Latin/alphanumeric/underscore word boundaries prevent substring corruption. CJK and Hangul matching also supports scripts without whitespace and decomposed Hangul.

Matches prefer the longest trigger at the same source position and apply once from left to right. Replacements do not cascade into other replacement rules. Snippet expansion is a subsequent independent pass. URLs using `scheme://` (including HTTP, FTP, SSH, and compound schemes such as `git+ssh`) and bare `www` links, email addresses, signed/decimal numeric tokens, currency amounts, inline/fenced code, Windows paths, and qualified/underscored identifiers are protected spans; neither pass edits them. An explicitly configured rule overlapping such data remains inert rather than risking a changed URL, amount, or code token. Expansion and match limits fail explicitly.

Matching uses NFKC plus Unicode lowercase rather than full Unicode case folding. Compatibility forms and ordinary case variants work; full-fold equivalents such as `ß` and `ss` remain distinct triggers. Adopting full folding requires a normalized-key migration and explicit collision policy for existing dictionary data.

Cheap normalization collapses whitespace while preserving line breaks. It reduces only consecutive bare `um`/`uh` repetitions, case-insensitively, to one token. It preserves `very very`, punctuation-separated fillers, repetitions across line breaks, and code-containing input. It never applies a fuzzy spelling change to local text.

## Candidate retrieval

Candidate retrieval supplies bounded spelling guidance to ASR/cloud cleanup:

1. An exact lexical alias index finds vocabulary and replacement-trigger matches. A trigger such as `post grass` retrieves its configured output `Postgres` without requiring edit-distance similarity.
2. Exact matches are ordered by source appearance and longest phrase. Up to eight configured starred terms follow, without displacing exact matches.
3. Remaining slots use unique trigram postings and normalized Levenshtein similarity. Common English prose and tokens shorter than four characters do not produce fuzzy name guidance. Already recognized terms do not pay for redundant fuzzy retrieval.

The result is at most 32 unique canonical outputs. Vocabulary entries determine spelling when several rules have equivalent normalized outputs; starred status survives deduplication. Fuzzy retrieval is bounded to 64 query tokens, 1,024 query grams, 16,384 posting visits, and a 256-candidate scoring pool. Exact lexical matches remain discoverable regardless of common-gram posting limits. Fuzzy candidates are guidance, never automatic local replacements or a claim that the intended term is known.

Index construction uses hash-based output deduplication rather than repeatedly scanning the vocabulary. Its expected cost is proportional to total pattern characters plus ordered alias insertion (`O(T + d log d)`). Phrase application costs `O(n + m log(1+p) + k log k)`, including protected-span lookup, and candidate retrieval costs `O(n + m + p + c·t·l²)`, with explicit query, match, posting, token, candidate, and phrase-length limits.

## Enhancement preservation

`onda-core/src/text.rs` rejects empty/oversized/NUL output, excessive cleanup expansion or contraction, changed protected-token multisets, model refusal prefixes, added headings/bold/fences in ordinary cleanup, and loss of explicitly present dictionary terms. Terms use word boundaries rather than arbitrary substring search. URL-ending punctuation is preserved exactly because it may be part of real path/query data; an ambiguous punctuation edit falls back to local text.

The prompt treats source, context, and dictionary spellings as untrusted string data. JSON encoding prevents a term from closing application-created delimiters. Relevant spellings are capped at 32 and 8 KiB of JSON; valid large candidate sets retain a fitting prefix. Explicit word self-corrections and bullet formatting can pass; removal of a numerical value still fails the conservative protected-token gate, including an ambiguous numeric self-correction. These checks provide useful mechanical preservation gates, not a semantic proof that every model edit preserves intent. The runtime must preserve local text on any enhancement failure.

A `+`, `-` or `−` directly after a letter or digit joins a name to its number rather than signing it, so "GPT6" → "GPT-6" and "6.1" → "GPT-6.1" keep the values 6 and 6.1; "-6" after a space is still negative. Each rejection names its reason without quoting text (a number, link, email or code changed; a dictionary word changed; formatting was added; the reply was not an edit; the cleanup grew or shrank too far; nothing usable came back), so the message is safe to log and tells the user what tripped the check. The rejected text is kept as the result's `suggestion` and in History, never inserted.

## Correction learning

Learning is opt-in and proposes a correction for explicit confirmation. It does not silently modify the dictionary. `learning.rs` requires an otherwise identical bounded observation with exactly one changed alphabetic token, a 3–60-character old and new term, a capitalized proposed spelling, sufficient lexical similarity, and no change to protected tokens. Common prose, case-only edits, whole-sentence rewrites, changed URLs/emails/code, and distant name substitutions are rejected. The common-prose filter is conservative English guidance; it is not an exhaustive language lexicon.

The Windows runtime additionally owns focus/password/scope checks and observation timing. Its proposal remains ephemeral until the user accepts it. Only accepted replacements become durable dictionary entries.

## Tests and measured performance

`cargo test -p onda-core` exercises Unicode compatibility expansion, combining marks, Hangul, boundaries, longest-match behavior, no cascading, protected spans, alias retrieval beyond common posting limits, canonical deduplication/star status, common-word filtering, prompt injection quoting, numeric/URL preservation, and conservative learning. Existing SQLite restart/import/migration and state-machine tests run in the same suite.

`cargo run -p onda-core --bin onda-quality` runs the committed deterministic quality corpus and a 10,000-term retrieval workload. On this Windows development machine (Intel Core i5-14400F, 10 cores/16 logical processors), a **debug** run on 2026-10-03 reported **796.8 ms index construction** and **0.0475 ms mean exact candidate retrieval** over 1,000 iterations. The timing excludes compilation, uses synthetic dictionary entries, and is not a speech-recognition accuracy, cloud latency, or production-hardware claim. Re-run the harness for release measurements and representative device/corpus distributions.
