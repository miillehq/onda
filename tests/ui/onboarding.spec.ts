// L4 and L5: a fresh install against the labelled mock (`?mock=onboarding`, Rust's defaults).
import { expect, test } from "@playwright/test";
import { openMock, tier1, tier2, tier3 } from "./visual";

test.beforeEach(async ({ page }) => openMock(page, "/?mock=onboarding"));

test("a fresh install opens on the welcome step and moves on to the microphone check", async ({ page }) => {
  const progress = page.getByRole("progressbar", { name: "Setup progress" });
  await expect(page.getByRole("heading", { name: "Welcome to Onda" })).toBeVisible();
  await expect(progress).toHaveAttribute("aria-valuenow", "1");
  await page.getByRole("button", { name: "Get started" }).click();
  await expect(progress).toHaveAttribute("aria-valuenow", "2");
  await page.getByRole("button", { name: "Back" }).click();
  await expect(progress).toHaveAttribute("aria-valuenow", "1");
});

test("tier 1: the welcome step's structure", async ({ page }) => tier1(page.locator("main.setup-main"), "welcome"));
test("tier 2: the welcome step's layout", async ({ page }) => tier2(page, "welcome"));
test("tier 3: the welcome step's pixels", async ({ page }) => tier3(page, "welcome"));
