import { expect, test } from "@playwright/test";
import { openMock } from "./visual";

test("editing a rewrite keeps its instruction and removing the default chooses Polish", async ({ page }) => {
  await openMock(page, "/#rewrites");
  await page.getByRole("button", { name: "New rewrite" }).click();
  const dialog = page.getByRole("dialog");
  const instruction = "\n  Keep the name.\n";
  await dialog.getByLabel("Name", { exact: true }).fill("Personal");
  await dialog.getByLabel("Instruction", { exact: true }).fill(instruction);
  await dialog.getByRole("button", { name: "Create rewrite" }).click();
  await page.getByRole("button", { name: "Edit Personal" }).click();
  await dialog.getByLabel("Name", { exact: true }).fill("Personal revised");
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await page.getByRole("button", { name: "Edit Personal revised" }).click();
  await expect(dialog.getByLabel("Instruction", { exact: true })).toHaveValue(instruction);
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  const selected = page.getByRole("combobox", { name: "Default rewrite" });
  await selected.selectOption({ label: "Personal revised" });
  await expect(page.getByRole("listitem").filter({ hasText: "Personal revised" }).getByText("Default", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Remove Personal revised" }).click();
  await expect(selected).toHaveValue("polish");
});

test("editing the snippet expansion preserves its original trigger", async ({ page }) => {
  await openMock(page, "/#snippets");
  await page.getByRole("button", { name: "New snippet" }).first().click();
  const dialog = page.getByRole("dialog");
  const trigger = "  outer  ";
  await dialog.getByLabel("When you say").fill(trigger);
  await dialog.getByLabel("Type this").fill("First expansion");
  await dialog.getByRole("button", { name: "Create snippet" }).click();
  const row = page.getByRole("listitem").filter({ hasText: "outer" });
  await row.getByRole("button", { name: "Edit" }).click();
  await dialog.getByLabel("Type this").fill("Changed expansion");
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await row.getByRole("button", { name: "Edit" }).click();
  await expect(dialog.getByLabel("When you say")).toHaveValue(trigger);
});

test("a rewrite can use its own ChatGPT model and return to the account default", async ({ page }) => {
  await openMock(page, "/#rewrites");
  await page.getByRole("button", { name: "New rewrite" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("Name", { exact: true }).fill("Test rewrite");
  await dialog.getByLabel("Instruction", { exact: true }).fill("Keep the meaning.");
  const model = dialog.getByRole("combobox", { name: "ChatGPT model" });
  await model.selectOption("thorough-sample");
  await dialog.getByRole("button", { name: "Create rewrite" }).click();
  const edit = page.getByRole("button", { name: "Edit Test rewrite" });
  await edit.click();
  await expect(model).toHaveValue("thorough-sample");
  await model.selectOption("");
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await edit.click();
  await expect(model).toHaveValue("");
});
