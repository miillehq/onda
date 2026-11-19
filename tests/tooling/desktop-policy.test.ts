import { readFileSync } from "node:fs";
import { expect, it } from "vitest";

it("keeps the development WebSocket and style allowances out of the distributed policy", () => {
  const config = JSON.parse(readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  const { csp, devCsp } = config.app.security as { csp: string; devCsp: string };
  expect(csp).toContain("connect-src ipc: http://ipc.localhost");
  expect(csp).not.toMatch(/wss?:|(?:127\.0\.0\.1|localhost):\d|unsafe-eval|unsafe-inline/);
  expect(devCsp).toContain("ws://127.0.0.1:1420");
});
