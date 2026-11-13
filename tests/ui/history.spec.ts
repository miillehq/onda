// L4 and L5: the History page of a returning user against the labelled mock. The mock's sample
// entries are dated from the fixed clock in openMock: four today, two yesterday.
import { expect, test, type Page } from "@playwright/test";
import { openMock, tier1, tier2, tier3 } from "./visual";

test.beforeEach(async ({ page }) => openMock(page, "/#history"));

const content = (page: Page) => page.getByRole("main", { name: "Content" });
const day = (page: Page, label: string) => content(page).getByRole("region", { name: label });

test("entries are grouped by day, newest first, with a failed one marked", async ({ page }) => {
  await expect(page.getByText("6 entries")).toBeVisible();
  await expect(day(page, "Today").getByRole("listitem")).toHaveCount(4);
  await expect(day(page, "Yesterday").getByRole("listitem")).toHaveCount(2);
  const today = day(page, "Today").getByRole("listitem");
  await expect(today.first()).toContainText("design review to Thursday");
  await expect(today.filter({ hasText: "Hi Priya" })).toContainText("Not typed");
});

test("search narrows the list and says when nothing matches", async ({ page }) => {
  const search = page.getByRole("searchbox", { name: "Search history" });
  await search.fill("STANDUP");
  await expect(content(page).getByRole("listitem")).toHaveCount(1);
  await expect(content(page).getByRole("listitem")).toContainText("Move standup to 9 tomorrow");
  await search.fill("keelson");
  await expect(page.getByText("Nothing matches “keelson”")).toBeVisible();
  await search.fill("");
  await expect(content(page).getByRole("listitem")).toHaveCount(6);
});

test("the original before polish opens and closes", async ({ page }) => {
  const entry = day(page, "Today").getByRole("listitem").first();
  await entry.getByRole("button", { name: "Show before polish" }).click();
  await expect(entry).toContainText("lets move the design review to thursday at three and um send the notes after");
  await entry.getByRole("button", { name: "Hide original" }).click();
  await expect(entry).not.toContainText("and um send");
});

test("Clear all asks first, then empties History", async ({ page }) => {
  await page.locator(".page-actions").getByRole("button", { name: "Clear all" }).click();
  const dialog = page.getByRole("dialog", { name: "Clear history?" });
  await expect(dialog).toContainText("This deletes 6 entries");
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(content(page).getByRole("listitem")).toHaveCount(6);
  await page.locator(".page-actions").getByRole("button", { name: "Clear all" }).click();
  await dialog.getByRole("button", { name: "Clear history" }).click();
  await expect(page.getByText("Nothing here yet")).toBeVisible();
  await expect(page.locator(".page-actions").getByRole("button", { name: "Clear all" })).toBeDisabled();
});

test("tier 1: History's structure", async ({ page }) => tier1(page.getByRole("main", { name: "Content" }), "history"));
test("tier 2: History's layout", async ({ page }) => tier2(page, "history"));
test("tier 3: History's pixels", async ({ page }) => tier3(page, "history"));
