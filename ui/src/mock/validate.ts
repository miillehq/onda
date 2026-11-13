/**
 * The development mock's copy of Rust's input rules: `Settings::validate`
 * (crates/onda-core/src/contracts.rs) and `normalize_name` (crates/onda-core/src/profile.rs).
 * Messages must match Rust's exactly; the shared examples in contracts/examples.ts run against
 * both this mock (tests/unit) and the real app (tests/e2e), so a drift fails one of them.
 */
import type { AppError, Settings, Shortcut } from "../generated/core";
import { LIMITS } from "../generated/core";

function invalid(message: string): AppError {
  return { code: "invalidInput", message, retryable: false };
}

/** Rust's `char::is_control`: Unicode general category Cc. */
const CONTROL = /\p{Cc}/u;

export function validateSettings(settings: Settings): AppError | null {
  if (settings.historyDays > LIMITS.maxHistoryDays) return invalid("History can be kept for at most 30 days.");
  if (settings.maxRecordingSeconds < 1 || settings.maxRecordingSeconds > LIMITS.maxRecordingSeconds) {
    return invalid("Recording limit must be between 1 and 600 seconds.");
  }
  const chords = new Set<string>();
  const shortcuts: Shortcut[] = [settings.pushToTalk, settings.transformShortcut, settings.commandShortcut, settings.toggleShortcut];
  for (const shortcut of shortcuts) {
    const keys = shortcut.keys;
    if (keys.length === 0 || keys.length > 4 || keys.some((key) => key === 0 || key > 254 || key === 0x1b)) {
      return invalid("Shortcuts need 1–4 valid Windows virtual keys; Escape is reserved for cancellation.");
    }
    const sorted = [...new Set(keys)].sort((a, b) => a - b);
    const chord = sorted.join(",");
    if (sorted.length !== keys.length || chords.has(chord)) return invalid("Shortcuts must be distinct and cannot repeat a key.");
    chords.add(chord);
  }
  if (settings.language !== null && (settings.language.length > 12 || !/^[A-Za-z-]*$/.test(settings.language))) {
    return invalid("Language must be a supported language code.");
  }
  const directory = settings.modelDirectory;
  if (directory !== null && (directory.length === 0 || utf8Length(directory) > 32767 || directory.includes("\0"))) {
    return invalid("Model directory is invalid.");
  }
  for (const value of [settings.openaiBaseUrl, settings.openaiModel, settings.anthropicModel, settings.localBaseUrl, settings.localModel]) {
    if (value === null) continue;
    if (value.length === 0 || utf8Length(value) > LIMITS.maxProviderSettingBytes || /[\p{Cc}\s]/u.test(value)) {
      return invalid("AI provider addresses and model names cannot be empty or contain spaces.");
    }
  }
  if (
    settings.appProfiles.length > 100 ||
    settings.appProfiles.some(
      (profile) =>
        profile.executable.length === 0 ||
        utf8Length(profile.executable) > 260 ||
        (profile.cleanupInstruction !== null && utf8Length(profile.cleanupInstruction) > 4096),
    )
  ) {
    return invalid("Application profiles are invalid or too large.");
  }
  return null;
}

/** Rust's `normalize_name`: collapse whitespace, refuse control characters, count graphemes. */
export function normalizeName(raw: string | null): { name: string | null } | { error: AppError } {
  const name = (raw ?? "").split(/\s+/u).filter(Boolean).join(" ");
  if (name.length === 0) return { name: null };
  if (CONTROL.test(name)) return { error: invalid("Names can't contain control characters.") };
  const graphemes = [...new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(name)].length;
  if (graphemes > LIMITS.maxNameCharacters) return { error: invalid(`Use a name of at most ${LIMITS.maxNameCharacters} characters.`) };
  return { name };
}

function utf8Length(value: string): number {
  return new TextEncoder().encode(value).length;
}
