import { expect, test } from "@playwright/test";
import { readdir, readFile } from "node:fs/promises";
import { resolve } from "node:path";

test("the built page without a desktop bridge stops with a visible recovery action", async ({ page }) => {
  await page.goto("/?mock=default");
  await expect(page.getByRole("heading", { name: "Onda couldn't start" })).toBeVisible();
  await expect(page.getByText("Open Onda from the desktop app.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Try again" })).toBeVisible();
  await expect(page.getByTestId("backend-badge")).toHaveCount(0);
});

test("the distributable renderer excludes the development mock and native smoke fixture", async () => {
  const assets = resolve("artifacts", "production-ui", "assets");
  const scripts = (await readdir(assets)).filter((name) => name.endsWith(".js"));
  expect(scripts.length).toBeGreaterThan(0);
  for (const script of scripts) {
    const contents = await readFile(resolve(assets, script), "utf8");
    expect(contents, script).not.toContain("PrivateCanaryToken");
    expect(contents, script).not.toContain("Data is fictional.");
  }
});
