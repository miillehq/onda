// L4 and L5: Settings › General against the labelled mock.
import { expect, test } from "@playwright/test";
import { openMock, tier1, tier2, tier3 } from "./visual";

test.beforeEach(async ({ page }) => openMock(page, "/#settings/general"));

test("a profile name can contain forty flag graphemes", async ({ page }) => {
  const field = page.getByRole("textbox", { name: "Your name" });
  const name = "🇬🇧".repeat(40);
  await expect(field).toBeEnabled();
  await field.fill("");
  await page.keyboard.insertText(name);
  await field.press("Tab");
  await expect(field).toHaveValue(name);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1 })).toContainText(name);
});

test("choosing Dark applies it at once and Light switches back", async ({ page }) => {
  const dark = page.locator(".theme-option.theme-dark");
  await dark.click();
  await expect(dark).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.locator(".theme-option.theme-light").click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
});

test("tier 1: General settings' structure", async ({ page }) => tier1(page.getByRole("main", { name: "Content" }), "general"));
test("tier 2: General settings' layout", async ({ page }) => tier2(page, "general"));
test("tier 3: General settings' pixels", async ({ page }) => tier3(page, "general", page.getByRole("main", { name: "Content" }).getByRole("heading", { level: 1 })));
