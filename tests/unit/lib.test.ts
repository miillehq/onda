// L1: the interface's own logic in ui/src/lib, which every view relies on and no Rust test sees.
import { describe, expect, it } from "vitest";
import type { DailyUsage, ErrorCode, Settings } from "../../ui/src/generated/core";
import { DEFAULT_SETTINGS } from "../../ui/src/generated/core";
import { activityGrid, dayKey, parseDay } from "../../ui/src/lib/activity";
import { wordDiff } from "../../ui/src/lib/diff";
import { presentError } from "../../ui/src/lib/errors";
import { appName, formatBytes, formatClock, formatDuration, greeting, greetingFor, providerName, routeLabel } from "../../ui/src/lib/format";
import { chordFrom, keyLabel, orderedKeys, sameChord, shortcutText, virtualKey, VK } from "../../ui/src/lib/keys";
import { languageName } from "../../ui/src/lib/languages";
import { levelFromRms, Smoother } from "../../ui/src/lib/level";
import { parseRoute } from "../../ui/src/lib/router";

describe("wordDiff", () => {
  it("keeps unchanged words and marks only the replaced one", () => {
    expect(wordDiff("send the report today", "send the summary today")).toEqual([
      { kind: "equal", text: "send the " },
      { kind: "removed", text: "report" },
      { kind: "added", text: "summary" },
      { kind: "equal", text: " today" },
    ]);
  });

  it("rebuilds both texts from its segments", () => {
    const before = "Ünïcode   spaces\nand lines stay";
    const after = "Ünïcode spaces\nand new lines stay";
    const segments = wordDiff(before, after);
    expect(segments.filter((s) => s.kind !== "added").map((s) => s.text).join("")).toBe(before);
    expect(segments.filter((s) => s.kind !== "removed").map((s) => s.text).join("")).toBe(after);
  });

  it("shows one whole replacement when the texts are too long to compare word by word", () => {
    const before = "a ".repeat(1200);
    const after = "b ".repeat(1200);
    expect(wordDiff(before, after)).toEqual([
      { kind: "removed", text: before },
      { kind: "added", text: after },
    ]);
  });
});

describe("shortcut keys", () => {
  it("labels modifiers, letters, digits, function and named keys", () => {
    expect([VK.leftControl, VK.rightShift, VK.leftAlt].map(keyLabel)).toEqual(["Ctrl", "Shift", "Alt"]);
    expect([0x41, 0x37, 0x70, 0x87, 0x20, 0x62, 0xff].map(keyLabel)).toEqual(["A", "7", "F1", "F24", "Space", "Num 2", "Key 255"]);
  });

  it("orders a chord Ctrl, Shift, Alt, Win, then the main key", () => {
    expect(orderedKeys([0x20, VK.leftWin, VK.rightControl])).toEqual([VK.leftControl, VK.leftWin, 0x20]);
    expect(shortcutText([0x50, VK.leftAlt, VK.leftControl])).toBe("Ctrl + Alt + P");
  });

  it("treats left and right modifiers as the same chord", () => {
    expect(sameChord([VK.leftControl, VK.leftWin, 0x20], [VK.rightControl, VK.rightWin, 0x20])).toBe(true);
    expect(sameChord([VK.leftControl, 0x20], [VK.leftControl, VK.leftAlt, 0x20])).toBe(false);
  });

  it("records a chord as left-hand modifiers and one main key, refusing Esc and extra keys", () => {
    expect(chordFrom([VK.rightControl, VK.rightWin, 0x20])).toEqual({ keys: [VK.leftControl, VK.leftWin, 0x20], problem: null });
    expect(chordFrom([VK.leftControl]).problem).toBe("Add one key that isn't Ctrl, Shift, Alt or Win.");
    expect(chordFrom([VK.leftControl, 0x41, 0x42]).problem).toBe("Use only one key besides Ctrl, Shift, Alt and Win.");
    expect(chordFrom([VK.leftControl, VK.escape]).problem).toBe("Esc is reserved for cancelling.");
    expect(chordFrom([VK.leftControl, VK.leftShift, VK.leftAlt, VK.leftWin, 0x41]).problem).toBe("Use at most four keys.");
  });

  it("maps keyboard event codes to Windows virtual keys", () => {
    const code = (value: string) => virtualKey({ code: value } as KeyboardEvent);
    expect(["KeyQ", "Digit5", "Numpad3", "F13", "MetaLeft", "Escape", "Slash", "Unidentified"].map(code)).toEqual([0x51, 0x35, 0x63, 0x7c, VK.leftWin, VK.escape, 0xbf, null]);
  });
});

describe("activity grid", () => {
  const usage = (day: string, words: number): DailyUsage => ({ day, dictations: words > 0 ? 1 : 0, words, audioMs: 1000 });

  it("reads native day keys as local calendar days", () => {
    const date = parseDay("2026-03-29");
    expect([date.getFullYear(), date.getMonth(), date.getDate()]).toEqual([2026, 2, 29]);
    expect(dayKey(date)).toBe("2026-03-29");
  });

  it("lays out Monday-first weeks ending with the week of today", () => {
    const grid = activityGrid([], "2026-11-13", 4);
    expect(grid.weeks).toHaveLength(4);
    expect(grid.weeks.every((week) => week.length === 7)).toBe(true);
    expect(grid.weeks[0]![0]!.key).toBe("2026-10-19");
    expect(grid.weeks[3]!.map((day) => day.key)).toEqual(["2026-11-09", "2026-11-10", "2026-11-11", "2026-11-12", "2026-11-13", "2026-11-14", "2026-11-15"]);
    expect(grid.weeks[3]!.map((day) => day.future)).toEqual([false, false, false, false, false, true, true]);
  });

  it("scales intensity in quarters of the busiest day and counts only past active days", () => {
    const grid = activityGrid([usage("2026-11-09", 100), usage("2026-11-10", 50), usage("2026-11-11", 1), usage("2026-11-12", 0)], "2026-11-13", 1);
    expect(grid.weeks[0]!.slice(0, 5).map((day) => day.level)).toEqual([4, 2, 1, 0, 0]);
    expect(grid.activeDays).toBe(3);
  });
});

describe("formatting", () => {
  it("sizes bytes the way the model pages show them", () => {
    expect([512, 1536, 1024 ** 2 * 1.5, 77_704_715, 1024 ** 3 * 2.25].map(formatBytes)).toEqual(["1 KB", "2 KB", "1.5 MB", "74 MB", "2.3 GB"]);
  });

  it("formats durations and clocks", () => {
    expect([0, 29_000, 61_000, 3_600_000, 5_400_000].map(formatDuration)).toEqual(["0 s", "29 s", "1 min", "1 h", "1 h 30 m"]);
    expect([0, 65_400, -5].map(formatClock)).toEqual(["0:00", "1:05", "0:00"]);
  });

  it("greets by the time of day and the first word of the name", () => {
    const at = (hour: number) => new Date(2026, 10, 13, hour);
    expect([at(3), at(9), at(13), at(20)].map((date) => greeting(date))).toEqual(["Working late", "Good morning", "Good afternoon", "Good evening"]);
    expect(greetingFor("  Ada   Lovelace", at(9))).toBe("Good morning, Ada");
    expect(greetingFor("   ", at(9))).toBe("Good morning");
  });

  it("names where AI polish sends text, never inventing a host", () => {
    const settings = (change: Partial<Settings>): Settings => ({ ...DEFAULT_SETTINGS, ...change });
    expect(providerName(settings({}))).toBe("ChatGPT");
    expect(providerName(settings({ aiProvider: "openaiCompatible", openaiBaseUrl: "https://api.example.com/v1" }))).toBe("api.example.com");
    expect(providerName(settings({ aiProvider: "openaiCompatible", openaiBaseUrl: "not a url" }))).toBe("your AI server");
    expect(providerName(settings({ aiProvider: "local" }))).toBe("the model on this computer");
    expect(routeLabel("anthropic")).toBe("Polished with Claude");
    expect(routeLabel("local")).toBeNull();
  });

  it("names apps from their executables", () => {
    expect(["WINWORD.EXE", "slack.exe", "my-tool.exe"].map(appName)).toEqual(["Word", "Slack", "My-tool"]);
    expect([null, "en", "xx"].map(languageName)).toEqual(["Auto-detect", "English", "XX"]);
  });
});

describe("microphone level", () => {
  it("maps RMS onto a speech-friendly 0 to 1 scale", () => {
    expect([0, Number.NaN, 10 ** (-55 / 20), 10 ** (-35 / 20), 1].map(levelFromRms)).toEqual([0, 0, 0, 0.5, 1]);
  });

  it("rises fast, falls slowly and settles at zero", () => {
    const smoother = new Smoother();
    expect(smoother.next(1)).toBe(0.5);
    expect(smoother.next(0)).toBeCloseTo(0.44, 5);
    for (let step = 0; step < 200; step += 1) smoother.next(0);
    expect(smoother.value).toBe(0);
  });
});

describe("error presentation", () => {
  it("sends every error's action to a route the app can open", () => {
    const codes: ErrorCode[] = ["storage", "noMicrophone", "noSpeech", "audioLimit", "noModel", "unauthorized", "focusChanged", "shortcut", "internal"];
    for (const code of codes) {
      const action = presentError({ code, message: "", retryable: false }).action;
      expect(action, code).toBeDefined();
      expect(parseRoute(action!.route), `${code} -> ${action!.route}`).toBe(action!.route);
    }
  });

  it("falls back to the internal presentation for an unknown code", () => {
    expect(presentError({ code: "notACode" as ErrorCode, message: "", retryable: false }).title).toBe("Something went wrong");
  });
});
