// L6, journey "dictionary, snippets and settings survive a restart", part 1: in the real app on a
// fresh profile, add a dictionary word and choose the dark theme through the interface.
// 02-restart.spec.ts launches the app again on the same profile and checks both survived.
import { $, expect } from "@wdio/globals";
import { call, open, saveSettings, screenshot, snapshot } from "./app.js";

describe("a fresh profile in the real app", () => {
  it("is served by the native backend", async () => {
    const state = await snapshot();
    expect(state.appName).toBe("Onda");
    expect(["windows", "linux", "macos"]).toContain(state.platform);
    expect(state.settings.onboardingCompleted).toBe(false);
  });

  it("adds a dictionary word through the interface", async () => {
    // Onboarding downloads a speech model; the journey under test starts after it.
    await saveSettings({ onboardingCompleted: true });
    await open("dictionary");
    await $("button=Add word").click();
    const phrase = await $('input[placeholder="Kubernetes"]');
    await phrase.setValue("Keelhaul");
    await $('form.entry-editor button[type="submit"]').click();
    await expect($('ul.entries[aria-label="Words"]')).toHaveText(expect.stringContaining("Keelhaul"));
    const dictionary = await call({ command: "getDictionary" });
    expect(dictionary.type === "dictionary" && dictionary.payload.map((entry) => entry.phrase)).toContain("Keelhaul");
  });

  it("chooses the dark theme in Settings", async () => {
    await open("settings/general");
    const dark = await $(".theme-option.theme-dark");
    await dark.click();
    await expect(dark).toHaveAttribute("aria-pressed", "true");
    await expect($("html")).toHaveAttribute("data-theme", "dark");
    await screenshot("settings-dark");
  });
});
