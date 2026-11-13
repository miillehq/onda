// Real-shell (L6) suite: the debug app built by `npm run build:e2e`, driven through the
// embedded WebDriver server that the e2e build registers. No external driver on any OS.
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { browser } from "@wdio/globals";
import path, { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
// The Cargo workspace builds into target/ at the root unless CARGO_TARGET_DIR moves it.
const targetRoot = process.env.CARGO_TARGET_DIR ? path.resolve(process.env.CARGO_TARGET_DIR) : join(root, "target");
const appBinaryPath = join(targetRoot, "debug", process.platform === "win32" ? "onda.exe" : "onda");
if (!existsSync(appBinaryPath)) {
  throw new Error(`Debug app not found at ${appBinaryPath}. Run: npm run build:e2e`);
}

// Every launch uses one isolated debug profile: its own data folder, no global shortcuts, no
// single-instance lock and no registry changes, so the suite can run beside an installed Onda.
// Specs run one after another on the same profile, which is how a later spec sees an earlier
// one's data after a real restart. onPrepare empties it once per run.
const artifacts = join(root, "artifacts", "e2e");
const profile = join(artifacts, "profile");

export const config = {
  runner: "local",
  // Order matters: each spec is a fresh launch of the app on the shared profile.
  specs: ["./tests/e2e/00-contract.spec.ts", "./tests/e2e/01-change.spec.ts", "./tests/e2e/02-restart.spec.ts"],
  maxInstances: 1,
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": { application: appBinaryPath },
      "wdio:tauriServiceOptions": { appBinaryPath, appArgs: [] },
    },
  ],
  services: [
    [
      "@wdio/tauri-service",
      {
        driverProvider: "embedded" as const,
        windowLabel: "main",
        env: { ONDA_DEV_PROFILE: profile },
        captureBackendLogs: true,
        captureFrontendLogs: true,
        startTimeout: 90_000,
      },
    ],
  ],
  onPrepare() {
    rmSync(profile, { recursive: true, force: true });
    mkdirSync(profile, { recursive: true });
  },
  // Onda has two webviews, the main window and the hidden dictation pill, and the WebDriver
  // session may start on either. Window handles are Tauri window labels.
  async before() {
    await browser.switchToWindow("main");
  },
  // A failed test leaves its screen and DOM next to the logs, so a CI failure can be read
  // from the uploaded artifact without rerunning it.
  async afterTest(test: { parent: string; title: string }, _context: unknown, result: { passed: boolean }) {
    if (result.passed) return;
    const name = `${test.parent} ${test.title}`.replace(/[^a-z0-9]+/gi, "-").toLowerCase();
    mkdirSync(artifacts, { recursive: true });
    await browser.saveScreenshot(join(artifacts, `failed-${name}.png`));
    writeFileSync(join(artifacts, `failed-${name}.html`), await browser.getPageSource());
  },
  logLevel: (process.env.DEBUG ? "debug" : "warn") as "debug" | "warn",
  outputDir: join(artifacts, "logs"),
  bail: 0,
  waitforTimeout: 15_000,
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 3,
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: { ui: "bdd", timeout: 90_000 },
  tsConfigPath: join(root, "tests", "e2e", "tsconfig.json"),
};
