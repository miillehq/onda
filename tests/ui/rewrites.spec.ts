import { expect, test } from "@playwright/test";
import { openMock } from "./visual";

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
