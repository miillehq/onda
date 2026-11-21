// L4 and L5: the dictionary page of a returning user against the labelled mock.
import { expect, test } from "@playwright/test";
import { readFile } from "node:fs/promises";
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

test("Enter in a replacement saves it, and Shift+Enter adds a line", async ({ page }) => {
  await page.getByRole("radio", { name: /Replacements/ }).click();
  await page.locator(".page-actions").getByRole("button", { name: "Add replacement" }).click();
  await page.getByLabel("When you say").fill("review address");
  const write = page.getByLabel("Write", { exact: true });
  await write.fill("First line");
  await write.press("Shift+Enter");
  await write.pressSequentially("Second line");
  await write.press("Enter");
  await expect(page.locator("form.entry-editor")).toHaveCount(0);
  const saved = page.getByRole("list", { name: "Replacements" }).getByRole("listitem").filter({ hasText: "review address" });
  await expect(saved.locator(".entry-phrase")).toHaveJSProperty("textContent", "First line\nSecond line");
});

test("a typed word is saved without the spaces around it", async ({ page }) => {
  await page.locator(".page-actions").getByRole("button", { name: "Add word" }).click();
  await page.getByPlaceholder("Kubernetes").fill("  Keelhaul  ");
  await page.locator("form.entry-editor").getByRole("button", { name: "Add word" }).click();
  await expect(words(page).getByRole("listitem").filter({ hasText: "Keelhaul" }).locator(".entry-phrase")).toHaveJSProperty("textContent", "Keelhaul");
});

test("a damaged UTF-8 backup explains the error and keeps the dictionary", async ({ page }) => {
  const buffer = Buffer.concat([
    Buffer.from('{"version":1,"entries":[{"id":"bad","kind":"vocabulary","phrase":"Broken'),
    Buffer.from([0xff]),
    Buffer.from('word","replacement":null,"enabled":true,"starred":false,"source":"manual"}]}'),
  ]);
  await page.locator('input[type="file"]').setInputFiles({ name: "damaged.json", mimeType: "application/json", buffer });
  await expect(page.getByText(/valid UTF-8/)).toBeVisible();
  await expect(words(page)).not.toContainText("Broken");
});

test("editing a trigger preserves its imported output and new typed whitespace", async ({ page }) => {
  const original = "\nKeep this line.\n";
  const entry = { id: "archive-entry", phrase: "Original trigger", replacement: original, kind: "replacement", enabled: true, starred: false, source: "manual" };
  await page.locator('input[type="file"]').setInputFiles({
    name: "backup.json", mimeType: "application/json", buffer: Buffer.from(JSON.stringify({ version: 1, entries: [entry] })),
  });
  await page.getByRole("radio", { name: /Replacements/ }).click();
  await page.getByRole("listitem").filter({ hasText: "Original trigger" }).getByRole("button", { name: "Edit", exact: true }).click();
  await page.getByLabel("When you say").fill("Changed trigger");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  const saved = page.getByRole("listitem").filter({ hasText: "Changed trigger" });
  await expect(saved).toBeVisible();
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export", exact: true }).click();
  const path = await (await download).path();
  const archive = JSON.parse(await readFile(path!, "utf8")) as { entries: { phrase: string; replacement: string | null }[] };
  expect(archive.entries.find((item) => item.phrase === "Changed trigger")?.replacement).toBe(original);
  await saved.getByRole("button", { name: "Edit", exact: true }).click();
  await page.getByLabel("Write", { exact: true }).fill("  Typed output  ");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(saved.locator(".entry-phrase")).toHaveJSProperty("textContent", "  Typed output  ");
});

test("tier 1: the dictionary's structure", async ({ page }) => tier1(page.getByRole("main", { name: "Content" }), "dictionary"));
test("tier 2: the dictionary's layout", async ({ page }) => tier2(page, "dictionary"));
test("tier 3: the dictionary's pixels", async ({ page }) => tier3(page, "dictionary", words(page)));
