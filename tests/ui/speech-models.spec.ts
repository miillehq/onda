import { expect, test } from "@playwright/test";
import { openMock } from "./visual";

test("the selected Fast card downloads its missing model and becomes ready", async ({ page }) => {
  await openMock(page, "/?mock=empty#settings/speech");
  const fast = page.locator(".model-card").filter({ has: page.getByRole("radio", { name: /^Fast\./ }) });
  await fast.getByRole("button", { name: "Download and use Fast" }).click();
  await expect(fast.getByText("In use", { exact: true })).toBeVisible();
  await expect(fast.getByRole("radio")).toHaveAttribute("aria-checked", "true");
});
