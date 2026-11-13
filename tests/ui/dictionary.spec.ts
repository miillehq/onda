// L4 and L5: the dictionary page of a returning user against the labelled mock.
import { expect, test } from "@playwright/test";
import { openMock, tier1, tier2, tier3 } from "./visual";

test.beforeEach(async ({ page }) => openMock(page, "/#dictionary"));

const words = (page: import("@playwright/test").Page) => page.getByRole("list", { name: "Words" });

test("adding a word through the editor lists it", async ({ page }) => {
  await page.locator(".page-actions").getByRole("button", { name: "Add word" }).click();
  await page.getByPlaceholder("Kubernetes").fill("Keelhaul");
  await page.locator("form.entry-editor").getByRole("button", { name: "Add word" }).click();
  await expect(words(page)).toContainText("Keelhaul");
  await expect(page.locator("form.entry-editor")).toHaveCount(0);
});

test("Escape closes the editor without saving", async ({ page }) => {
  await page.locator(".page-actions").getByRole("button", { name: "Add word" }).click();
  await page.getByPlaceholder("Kubernetes").fill("Discarded");
  await page.keyboard.press("Escape");
  await expect(page.locator("form.entry-editor")).toHaveCount(0);
  await expect(words(page)).not.toContainText("Discarded");
});

test("tier 1: the dictionary's structure", async ({ page }) => tier1(page.getByRole("main", { name: "Content" }), "dictionary"));
test("tier 2: the dictionary's layout", async ({ page }) => tier2(page, "dictionary"));
test("tier 3: the dictionary's pixels", async ({ page }) => tier3(page, "dictionary", words(page)));
