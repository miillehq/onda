// L4 and L5: how a failed dictation is shown, on Home and in the pill, against the labelled mock.
import { expect, test } from "@playwright/test";
import { openMock, tier1, tier3 } from "./visual";

test("a failed dictation on Home names the problem and leads to the fix", async ({ page }) => {
  await openMock(page, "/?mock=error");
  await expect(page.getByText("Didn't hear anything")).toBeVisible();
  await page.getByRole("button", { name: "Test microphone" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Microphone" })).toBeVisible();
});

test.describe("the pill's error card for a lost focus", () => {
  test.beforeEach(async ({ page }) => openMock(page, "/pill.html?pill=code&code=focusChanged&text=1"));

  test("names the problem", async ({ page }) => {
    await expect(page.getByRole("alert")).toContainText("Focus moved");
  });

  test("tier 1: its structure", async ({ page }) => tier1(page.getByRole("alert"), "pill-focus-changed"));
  test("tier 3: its pixels", async ({ page }) => tier3(page, "pill-focus-changed", page.getByRole("alert")));
});
