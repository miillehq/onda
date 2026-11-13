// Shared contract examples: requests whose answers the labelled development mock and the real
// Rust backend must agree on. tests/unit runs them against the mock (L2); tests/e2e runs the same
// list in the real app (L6), on a fresh profile. Expectations are partial: the response must
// contain them, so mock-only sample data (a picture, demo history) does not matter.
import type { ApiRequest, ApiResponse } from "../ui/src/generated/api";
import type { AppError, Settings } from "../ui/src/generated/core";
import { DEFAULT_SETTINGS, LIMITS, MACOS_DEFAULT_SETTINGS } from "../ui/src/generated/core";

type DeepPartial<T> = T extends object ? { [K in keyof T]?: DeepPartial<T[K]> } : T;
export type Outcome = { response: DeepPartial<ApiResponse> } | { error: Pick<AppError, "code" | "message"> };
export type Example = { name: string; request: ApiRequest; expect: Outcome };

const refused = (message: string): Outcome => ({ error: { code: "invalidInput", message } });
const settingsWith = (change: Partial<Settings>): ApiRequest => ({
  command: "updateSettings",
  args: { settings: { ...structuredClone(DEFAULT_SETTINGS), ...change } },
});
const named = (name: string | null): ApiRequest => ({ command: "setProfileName", args: { name } });
// One grapheme, two code points: the case where counting code points instead of graphemes drifts.
const flags = (count: number) => "\u{1F1F3}\u{1F1FF}".repeat(count);

/** Must run before anything changes the profile. A Mac starts with its own dictation chord. */
export const freshProfile = (macos: boolean): Example[] => [
  {
    name: "a fresh profile starts from Rust's default settings",
    request: { command: "getSettings" },
    expect: { response: { type: "settings", payload: macos ? MACOS_DEFAULT_SETTINGS : DEFAULT_SETTINGS } },
  },
];

/** Settings that `Settings::validate` refuses; nothing is saved. */
export const settingsRefusals: Example[] = [
  { name: "history longer than the limit", request: settingsWith({ historyDays: LIMITS.maxHistoryDays + 1 }), expect: refused("History can be kept for at most 30 days.") },
  { name: "a zero recording limit", request: settingsWith({ maxRecordingSeconds: 0 }), expect: refused("Recording limit must be between 1 and 600 seconds.") },
  { name: "a recording limit over the maximum", request: settingsWith({ maxRecordingSeconds: LIMITS.maxRecordingSeconds + 1 }), expect: refused("Recording limit must be between 1 and 600 seconds.") },
  {
    name: "Escape in a shortcut",
    request: settingsWith({ pushToTalk: { keys: [0xa2, 0x1b] } }),
    expect: refused("Shortcuts need 1–4 valid Windows virtual keys; Escape is reserved for cancellation."),
  },
  {
    name: "two actions on the same chord",
    request: settingsWith({ toggleShortcut: structuredClone(DEFAULT_SETTINGS.pushToTalk) }),
    expect: refused("Shortcuts must be distinct and cannot repeat a key."),
  },
  { name: "a key repeated within a shortcut", request: settingsWith({ pushToTalk: { keys: [0xa2, 0xa2, 0x20] } }), expect: refused("Shortcuts must be distinct and cannot repeat a key.") },
  { name: "a language that is not a code", request: settingsWith({ language: "en us" }), expect: refused("Language must be a supported language code.") },
  { name: "a model name with a space", request: settingsWith({ openaiModel: "gpt 5" }), expect: refused("AI provider addresses and model names cannot be empty or contain spaces.") },
  {
    name: "an app profile without an executable",
    request: settingsWith({ appProfiles: [{ executable: "", contextEnabled: false, enhancementEnabled: true, cleanupInstruction: null, pasteShortcut: "controlV" }] }),
    expect: refused("Application profiles are invalid or too large."),
  },
];

/** Profile names: normalised like Rust's `normalize_name`. The last example clears the name again. */
export const profileNames: Example[] = [
  { name: "a name has its whitespace collapsed", request: named("  Ada \t Lovelace "), expect: { response: { type: "profile", payload: { name: "Ada Lovelace" } } } },
  { name: "the name limit counts graphemes, not code points", request: named(flags(LIMITS.maxNameCharacters)), expect: { response: { type: "profile", payload: { name: flags(LIMITS.maxNameCharacters) } } } },
  { name: "a name over the limit", request: named(flags(LIMITS.maxNameCharacters + 1)), expect: refused("Use a name of at most 40 characters.") },
  { name: "a control character in a name", request: named("Ada\u0007"), expect: refused("Names can't contain control characters.") },
  { name: "an empty name clears it", request: named("   "), expect: { response: { type: "profile", payload: { name: null } } } },
];

const word = (id: string, phrase: string, replacement: string | null = null): ApiRequest => ({
  command: "saveDictionaryEntry",
  args: { entry: { id, kind: replacement === null ? "vocabulary" : "replacement", phrase, replacement, enabled: true, starred: false, source: "manual" } },
});

/** Dictionary entries: Rust's validation and one entry per trigger. The last example removes the word again. */
export const dictionaryEntries: Example[] = [
  { name: "a word is saved to the dictionary", request: word("contract-word", "Keelson"), expect: { response: { type: "dictionary", payload: [{ id: "contract-word", phrase: "Keelson" }] } } },
  { name: "the same word in another case and spacing is refused", request: word("contract-twin", "  KEELSON "), expect: refused("This trigger already exists. Edit the existing entry.") },
  { name: "a phrase without a word is refused", request: word("contract-symbols", "!!!"), expect: refused("Dictionary phrase must contain a word and at most 160 characters.") },
  {
    name: "a vocabulary word cannot carry a replacement",
    request: { command: "saveDictionaryEntry", args: { entry: { id: "contract-mixed", kind: "vocabulary", phrase: "Keel", replacement: "keel", enabled: true, starred: false, source: "manual" } } },
    expect: refused("Vocabulary has no replacement; a correction needs an output phrase."),
  },
  { name: "the saved word is removed", request: { command: "deleteDictionaryEntry", args: { id: "contract-word" } }, expect: { response: { type: "unit" } } },
];

/** Every example, in order, for a backend on macOS (`macos`) or on Windows and Linux. */
export const examplesFor = (macos: boolean): Example[] => [...freshProfile(macos), ...settingsRefusals, ...profileNames, ...dictionaryEntries];
