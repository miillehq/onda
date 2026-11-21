// L4: the Stats page against the labelled mock. Its activity grid labels each month above the
// week where it starts; the fixed clock (2026-11-13) starts the year in mid-November, so the
// first two labels are three weeks apart.
import { expect, test } from "@playwright/test";
import { openMock } from "./visual";

test("the shared card encodes a 1200 by 630 PNG", async ({ page }) => {
  await openMock(page, "/#stats");
  await page.getByRole("button", { name: "Share stats" }).click();
  const canvas = page.getByRole("dialog").locator("canvas");
  await expect.poll(async () => {
    const encoded = await canvas.evaluate((element) => (element as HTMLCanvasElement).toDataURL("image/png").split(",")[1]!);
    const png = Buffer.from(encoded, "base64");
    return [png.readUInt32BE(16), png.readUInt32BE(20)];
  }).toEqual([1200, 630]);
});

test("Stats keeps every month label on one row above the activity grid", async ({ page }) => {
  await openMock(page, "/#stats");
  const months = page.locator(".activity-months span");
  await expect(months.first()).toBeVisible();
  expect(await months.count()).toBeGreaterThan(10);
  const grid = await page.locator(".activity-grid").boundingBox();
  const labels = await months.evaluateAll((spans) => spans.map((span) => ({ text: span.textContent, ...span.getBoundingClientRect().toJSON() })));
  const rows = new Set(labels.map((label) => Math.round(label.top)));
  expect(rows.size, `labels on ${rows.size} rows: ${JSON.stringify(labels.map((label) => [label.text, Math.round(label.top)]))}`).toBe(1);
  for (const label of labels) expect(label.bottom, `${label.text} overlaps the grid`).toBeLessThanOrEqual(grid!.y + 1);
});
