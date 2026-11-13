// Helpers for the real-shell suite. Every request goes through the same `application_call`
// command and generated `ApiRequest`/`ApiResponse` types that the UI and the mock use.
import { browser, $ } from "@wdio/globals";
import "@wdio/native-types";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import type { ApiRequest, ApiResponse, Snapshot } from "../../ui/src/generated/api.js";
import type { Settings } from "../../ui/src/generated/core.js";

export async function call(request: ApiRequest): Promise<ApiResponse> {
  return (await browser.tauri.execute(
    ({ core }, payload) => core.invoke("application_call", { request: payload }),
    request,
  )) as ApiResponse;
}

export async function snapshot(): Promise<Snapshot> {
  const response = await call({ command: "getSnapshot" });
  if (response.type !== "snapshot") throw new Error(`Expected a snapshot, got ${response.type}`);
  return response.payload;
}

export async function saveSettings(change: Partial<Settings>): Promise<Settings> {
  const current = (await snapshot()).settings;
  const response = await call({ command: "updateSettings", args: { settings: { ...current, ...change } } });
  if (response.type !== "settings") throw new Error(`Expected settings, got ${response.type}`);
  return response.payload;
}

/**
 * Load a route the way the app does at startup: a new document that reads the backend's current
 * state and the route from the hash. A unique query string forces the reload (a hash change
 * alone keeps the document), and a marker on the old document proves the new one replaced it.
 */
export async function open(route: string): Promise<void> {
  const target = new URL(await browser.getUrl());
  target.search = `?load=${Date.now()}`;
  target.hash = `#${route}`;
  await browser.execute(() => {
    (window as unknown as { __ondaStale?: boolean }).__ondaStale = true;
  });
  await browser.url(target.toString());
  await browser.waitUntil(
    () =>
      browser.execute(
        () =>
          !(window as unknown as { __ondaStale?: boolean }).__ondaStale &&
          (document.querySelector("#app")?.childElementCount ?? 0) > 0,
      ),
    { timeoutMsg: `The app did not load #${route}` },
  );
}

/** One screenshot per platform, so runs on different runners never overwrite each other. */
export async function screenshot(name: string): Promise<void> {
  const { platform } = await snapshot();
  const directory = join(process.cwd(), "artifacts", "e2e");
  mkdirSync(directory, { recursive: true });
  await browser.saveScreenshot(join(directory, `${name}-${platform}-${process.arch}.png`));
}
