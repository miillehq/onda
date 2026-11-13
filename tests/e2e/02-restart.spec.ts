// L6, journey "dictionary, snippets and settings survive a restart", part 2: a new launch of the
// real app on the profile 01-change.spec.ts left behind.
import { $, expect } from "@wdio/globals";
import { open, screenshot, snapshot } from "./app.js";

describe("the same profile after a restart", () => {
  it("keeps onboarding finished and the dark theme", async () => {
    const { settings } = await snapshot();
    expect(settings.onboardingCompleted).toBe(true);
    expect(settings.theme).toBe("dark");
    await expect($("html")).toHaveAttribute("data-theme", "dark");
  });

  it("keeps the dictionary word", async () => {
    await open("dictionary");
    await expect($('ul.entries[aria-label="Words"]')).toHaveText(expect.stringContaining("Keelhaul"));
    await screenshot("dictionary-after-restart");
  });
});
