import type { ApiRequest } from "../ui/src/generated/api.js";
import type { Settings } from "../ui/src/generated/core.js";

/** Deliberately untyped callers still cross the same boundary as the desktop renderer. */
export function malformedSettingsRequests(settings: Settings): { name: string; request: ApiRequest }[] {
  const withoutLanguage: Partial<Settings> = { ...settings };
  delete withoutLanguage.language;
  const cases: [string, unknown][] = [
    ["an unrecognised preference", { ...settings, historyDayz: 7 }],
    ["a missing nullable preference", withoutLanguage],
    ["an incomplete replacement", { theme: "light" }],
    ["an extra key inside a shortcut", { ...settings, pushToTalk: { ...settings.pushToTalk, key: 42 } }],
    ["a fractional history limit", { ...settings, historyDays: 1.5 }],
    ["a string used as a switch", { ...settings, playSounds: "false" }],
    ["an unknown theme", { ...settings, theme: "sepia" }],
    ["null", null], ["a number", 2], ["an array", []], ["a serialized date", "2026-01-01T00:00:00Z"],
  ];
  return cases.map(([name, value]) => ({
    name,
    request: { command: "updateSettings", args: { settings: value as Settings } },
  }));
}
