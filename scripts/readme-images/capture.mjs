// Captures real Onda screens from the labelled development mock for the README images: the same
// interface code as the app, with sample data, a fixed clock and the mock badge hidden.
//   node scripts/readme-images/capture.mjs   (starts `npm run dev` unless it already answers)
import { chromium } from "@playwright/test";
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
export const screensDirectory = join(root, ".artifacts", "readme", "screens");
const origin = "http://127.0.0.1:1420";

export const screens = [
  { name: "home", path: "/#home" },
  { name: "home-dark", path: "/?theme=dark#home" },
  { name: "history", path: "/#history" },
  { name: "history-dark", path: "/?theme=dark#history" },
  { name: "stats", path: "/#stats" },
  { name: "stats-dark", path: "/?theme=dark#stats" },
  { name: "dictionary", path: "/#dictionary" },
  { name: "dictionary-dark", path: "/?theme=dark#dictionary" },
  { name: "shortcuts", path: "/#settings/shortcuts" },
  { name: "speech", path: "/#settings/speech" },
  { name: "speech-dark", path: "/?theme=dark#settings/speech" },
  { name: "ai", path: "/#settings/ai" },
];
export const pills = ["listening", "handsfree", "writing", "polishing", "done"];

async function answers() {
  try {
    return (await fetch(origin)).ok;
  } catch {
    return false;
  }
}

async function devServer() {
  if (await answers()) return null;
  const server = spawn(process.platform === "win32" ? "npm.cmd run dev" : "npm run dev", { cwd: root, shell: true, stdio: "ignore" });
  for (let attempt = 0; attempt < 120; attempt++) {
    if (await answers()) return server;
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  stop(server);
  throw new Error("the development server did not start on " + origin);
}

/** The shell started npm, which started Vite: stop the whole tree. */
function stop(server) {
  if (!server) return;
  if (process.platform === "win32") spawnSync("taskkill", ["/pid", String(server.pid), "/T", "/F"], { stdio: "ignore" });
  else server.kill();
}

async function settle(page) {
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  await page.waitForFunction(() => document.getAnimations().every((animation) => animation.playState !== "running" || animation.effect?.getTiming().iterations === Infinity));
  await page.waitForTimeout(250);
}

export async function capture() {
  mkdirSync(screensDirectory, { recursive: true });
  const server = await devServer();
  const browser = await chromium.launch();
  try {
    for (const screen of screens) {
      const context = await browser.newContext({ viewport: { width: 1240, height: 800 }, deviceScaleFactor: 2, colorScheme: screen.name.endsWith("-dark") ? "dark" : "light" });
      const page = await context.newPage();
      await page.clock.setFixedTime(new Date(2026, 10, 13, 10, 0, 0));
      await page.goto(origin + screen.path);
      await page.getByTestId("backend-badge").waitFor();
      await page.addStyleTag({ content: ".mock-badge { display: none !important; }" });
      await settle(page);
      // Home's hero types a sample sentence; let it finish first.
      if (screen.name.startsWith("home")) await page.waitForTimeout(4500);
      await page.screenshot({ path: join(screensDirectory, `${screen.name}.png`) });
      await context.close();
    }
    for (const pill of pills) {
      for (const scheme of ["light", "dark"]) {
        const context = await browser.newContext({ viewport: { width: 640, height: 260 }, deviceScaleFactor: 2, colorScheme: scheme });
        const page = await context.newPage();
        await page.goto(`${origin}/pill.html?pill=${pill}${scheme === "dark" ? "&theme=dark" : ""}`);
        // Lift the pill off the window's bottom edge so its whole shadow is captured.
        await page.addStyleTag({ content: ".mock-badge { display: none !important; } html, body { background: transparent !important; } .pill-stage { padding-bottom: 48px !important; }" });
        await settle(page);
        // The pill sits low on a transparent stage; keep room around it for its shadow.
        const box = await page.locator(".pill-motion").first().boundingBox();
        const margin = 40;
        const clip = { x: Math.max(0, box.x - margin), y: Math.max(0, box.y - margin), width: Math.min(640, box.width + 2 * margin), height: Math.min(260 - Math.max(0, box.y - margin), box.height + 2 * margin) };
        await page.screenshot({ path: join(screensDirectory, `pill-${pill}-${scheme}.png`), omitBackground: true, clip });
        await context.close();
      }
    }
  } finally {
    await browser.close();
    stop(server);
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) await capture();
