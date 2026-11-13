// The three visual tiers (TESTING.md), shared by every browser spec.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { existsSync, readFileSync } from "node:fs";
import { basename, join } from "node:path";
import { assertGeometry, captureGeometry } from "./geometry";

/** Open a page of the labelled mock and prove it is the mock before anything else. */
export async function openMock(page: Page, path: string): Promise<void> {
  // A fixed clock keeps greetings, dates and the activity grid identical in every baseline.
  await page.clock.setFixedTime(new Date(2026, 10, 13, 10, 0, 0));
  await page.goto(path);
  await expect(page.getByTestId("backend-badge")).toHaveText("Development mock");
}

function snapshotFile(name: string): string {
  const info = test.info();
  return join(info.project.testDir, "__snapshots__", basename(info.file), name);
}

/** Tier 1: the accessibility tree, one baseline for every engine and OS. */
export async function tier1(scope: Locator, name: string): Promise<void> {
  await expect(scope).toMatchAriaSnapshot({ name: `${name}.aria.yml` });
}

/** Tier 2: rounded boxes of the elements marked `data-geo`, one baseline, 8 px tolerance. */
export async function tier2(page: Page, name: string): Promise<void> {
  const file = snapshotFile(`${name}.geometry.json`);
  // The app renders after its first backend answer and its fonts load; measure only then.
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  if (existsSync(file)) {
    const expected = Object.keys(JSON.parse(readFileSync(file, "utf8")) as Record<string, unknown>).sort();
    await expect
      .poll(async () => Object.keys(await captureGeometry(page)).sort(), { message: "every element of the geometry baseline renders" })
      .toEqual(expected);
  } else {
    await expect(page.locator("[data-geo]").first()).toBeVisible();
  }
  assertGeometry(file, await captureGeometry(page));
}

/**
 * Tier 3: pixels, one baseline per engine and OS, captured only by the snapshots workflow on
 * the runners. Until that workflow has run for a screen, its pixel check is skipped and says so.
 */
export async function tier3(page: Page, name: string): Promise<void> {
  const file = `${name}-${test.info().project.name}-${process.platform}.png`;
  const updating = test.info().config.updateSnapshots === "all";
  test.skip(!updating && !existsSync(snapshotFile(file)), `no pixel baseline for ${file} yet; the snapshots workflow captures it on the runners`);
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  await expect(page).toHaveScreenshot(file, { fullPage: true });
}
