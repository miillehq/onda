// L4: what a Mac user reads where Onda behaves differently on macOS, against the labelled mock's
// Mac variant (?platform=macos). Key names follow the browser's own platform, so a chord is
// "Ctrl + Shift + Space" in a Windows browser and "Control + Shift + Space" in a Mac one.
import { expect, test } from "@playwright/test";
import { openMock } from "./visual";

const content = (page: import("@playwright/test").Page) => page.getByRole("main", { name: "Content" });

test("Shortcuts on a Mac: Control+Shift+Space dictates, Esc alone cancels, Accessibility lets Onda paste", async ({ page }) => {
  await openMock(page, "/?platform=macos#settings/shortcuts");
  const shortcuts = content(page);
  await expect(shortcuts).toContainText("Onda listens for these anywhere on your Mac.");
  await expect(shortcuts.getByRole("img", { name: /^(Ctrl|Control) \+ Shift \+ Space$/ })).toBeVisible();
  await expect(shortcuts.getByRole("img", { name: "Escape" })).toBeVisible();
  await expect(shortcuts).not.toContainText("desktop");
  await expect(shortcuts.getByRole("heading", { name: "Typing into apps" })).toBeVisible();
  await expect(shortcuts).toContainText("Off. What you say is copied, ready for ⌘V. Turn on Onda under Privacy & Security › Accessibility and it pastes for you.");
  await expect(shortcuts).toContainText("Onda never pastes into password fields, or while an app protects what you type.");
});

test("Allowing typing on a Mac sends the user to System Settings, where only they can turn it on or off", async ({ page }) => {
  await openMock(page, "/?platform=macos#settings/shortcuts");
  const shortcuts = content(page);
  await shortcuts.getByRole("button", { name: "Allow" }).click();
  // macOS answers before the switch is on; that is not an error to show, and typing is still off.
  await expect(shortcuts.getByRole("button", { name: "Allow" })).toBeEnabled();
  await expect(shortcuts).toContainText("Off. What you say is copied, ready for ⌘V.");
  await expect(page.getByRole("alert")).toHaveCount(0);
  // The user turns Onda on in System Settings and comes back: the page looks again.
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expect(shortcuts).toContainText("On. Onda pastes what you say where your cursor is, and puts back what you had copied.");
  await shortcuts.getByRole("button", { name: "Open System Settings" }).click();
  await expect(shortcuts).toContainText("To stop, turn Onda off under Privacy & Security › Accessibility.");
  await expect(shortcuts.getByRole("button", { name: "Allow" })).toHaveCount(0);
});

test("Home on a Mac points to Shortcuts until Onda may paste", async ({ page }) => {
  await openMock(page, "/?platform=macos#home");
  await expect(page.locator(".hero-text")).toHaveText("Works in any app. Allow typing in Shortcuts so Onda can paste for you.");
});

test("Privacy on a Mac keeps the history key in the keychain", async ({ page }) => {
  await openMock(page, "/?platform=macos#settings/privacy");
  await expect(content(page)).toContainText("Encrypted with a key in your Mac's keychain");
  await expect(content(page)).not.toContainText("desktop keyring");
});

test("Updates and help on a Mac offers no resource report, which macOS cannot measure yet", async ({ page }) => {
  await openMock(page, "/?platform=macos#settings/about");
  await expect(content(page).getByRole("heading", { name: "Shortcuts at a glance" })).toBeVisible();
  await expect(content(page).getByRole("heading", { name: "What Onda is using" })).toHaveCount(0);
});
