import { DEFAULT_SETTINGS, type AppProfile, type Settings } from "../generated/core";

const enums = {
  executionPreference: ["auto", "cpuOnly", "gpuPreferred"],
  qualityProfile: ["fast", "balanced", "bestQuality", "custom"],
  theme: ["system", "light", "dark"],
  trayIcon: ["auto", "black", "white"],
  aiProvider: ["chatgpt", "openaiCompatible", "anthropic", "local"],
} satisfies { [K in "executionPreference" | "qualityProfile" | "theme" | "trayIcon" | "aiProvider"]: Settings[K][] };

const pasteShortcuts: ReadonlySet<AppProfile["pasteShortcut"]> = new Set(["controlV", "controlShiftV", "shiftInsert"]);
const profileFields = ["executable", "contextEnabled", "enhancementEnabled", "cleanupInstruction", "pasteShortcut"] as const satisfies readonly (keyof AppProfile)[];
const settingsFields = Object.keys(DEFAULT_SETTINGS) as (keyof Settings)[];
const MAX_UNSIGNED_32 = 0xffff_ffff;

function hasFields(value: unknown, fields: readonly string[]): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    && Object.keys(value).length === fields.length && fields.every((field) => Object.hasOwn(value, field));
}

function isUnsigned(value: unknown, maximum: number): boolean {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 && value <= maximum;
}

function isProfile(value: unknown): boolean {
  return hasFields(value, profileFields) && typeof value.executable === "string"
    && typeof value.contextEnabled === "boolean" && typeof value.enhancementEnabled === "boolean"
    && (value.cleanupInstruction === null || typeof value.cleanupInstruction === "string")
    && pasteShortcuts.has(value.pasteShortcut as AppProfile["pasteShortcut"]);
}

/** Check the JSON types that serde requires before applying the shared domain limits. */
export function isCompleteSettings(value: unknown): value is Settings {
  if (!hasFields(value, settingsFields)) return false;
  return settingsFields.every((field) => {
    const actual = value[field];
    const sample = DEFAULT_SETTINGS[field];
    if (Object.hasOwn(enums, field)) {
      const choices: readonly string[] = enums[field as keyof typeof enums];
      return typeof actual === "string" && choices.includes(actual);
    }
    if (sample === null) return actual === null || typeof actual === "string";
    if (typeof sample === "number") return isUnsigned(actual, MAX_UNSIGNED_32);
    if (Array.isArray(sample)) return Array.isArray(actual) && actual.every(isProfile);
    if (typeof sample === "object") {
      return hasFields(actual, ["keys"]) && Array.isArray(actual.keys)
        && actual.keys.every((key: unknown) => isUnsigned(key, 0xffff));
    }
    return typeof actual === typeof sample;
  });
}
