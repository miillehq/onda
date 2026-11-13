// L4 and L5: the interface in real browsers against the Vite dev server, where the labelled
// development mock answers. chromium stands in for WebView2 (Windows) and webkit for WebKitGTK
// (Linux) and WKWebView (macOS); CI runs both engines on the three OS runners.
import { defineConfig, devices } from "@playwright/test";

// Onda's main window opens at 1120 x 780 (src-tauri/tauri.conf.json).
const viewport = { width: 1120, height: 780 };

export default defineConfig({
  testDir: "tests/ui",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  workers: process.env.CI ? 2 : undefined,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  // One template for every snapshot kind. Pixel snapshots carry the engine and OS in their own
  // name; ARIA and geometry snapshots are shared by every engine and OS by design.
  snapshotPathTemplate: "{testDir}/__snapshots__/{testFileName}/{arg}{ext}",
  expect: {
    toHaveScreenshot: { maxDiffPixelRatio: 0.002, animations: "disabled", caret: "hide" },
  },
  use: {
    baseURL: "http://127.0.0.1:1420",
    viewport,
    deviceScaleFactor: 1,
    colorScheme: "light",
    // Dates and times in the baselines ("9:58 AM") must not follow the runner's locale.
    locale: "en-US",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"], viewport, deviceScaleFactor: 1 } },
    { name: "webkit", use: { ...devices["Desktop Safari"], viewport, deviceScaleFactor: 1 } },
  ],
  webServer: {
    command: "npm run dev",
    url: "http://127.0.0.1:1420",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
