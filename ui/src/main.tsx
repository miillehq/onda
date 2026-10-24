import "@fontsource-variable/inter";
import "@fontsource-variable/newsreader";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/controls.css";
import "./styles/capsule.css";
import "./styles/shell.css";
import "./styles/pages.css";
import { render } from "preact";
import { call } from "./api";
import { App } from "./app/App";
import { navigate, parseRoute } from "./lib/router";
import * as store from "./lib/store";
import { init, operation, toastError } from "./lib/store";

const root = document.getElementById("app");
if (!root) throw new Error("Application root is missing.");

// The browser's own context menu ("Back", "Inspect") has no place in a desktop app.
if (!import.meta.env.DEV) {
  document.addEventListener("contextmenu", (event) => {
    const target = event.target as HTMLElement | null;
    if (!target?.closest("input, textarea, .selectable")) event.preventDefault();
  });
}

if (import.meta.env.DEV) {
  // Development aid: inspect live state from automated checks.
  (window as unknown as { __onda: typeof store }).__onda = store;
  // Development aid: the shortcut helper ignores injected input by design, so automated
  // checks toggle dictation with Ctrl+Shift+F9 through the same IPC instead.
  document.addEventListener("keydown", (event) => {
    if (!(event.ctrlKey && event.shiftKey && event.key === "F9")) return;
    event.preventDefault();
    const recording = operation.value?.phase === "recording";
    void call(recording ? { command: "stopDictation" } : { command: "startDictation", args: { commandMode: false } }).catch((error) => toastError(error));
  });
}

// Deep links such as #settings/speech open a section directly.
const linked = parseRoute(window.location.hash.slice(1));
if (linked) navigate(linked);

render(<App />, root);

void init()
  .then(async () => {
    if (window.__EAGLE_SMOKE__) await (await import("./smoke")).runSmoke();
  })
  .catch(async (error: unknown) => {
    if (window.__EAGLE_SMOKE__) {
      const code = typeof error === "object" && error !== null && "code" in error ? String(error.code) : "unknown";
      await call({ command: "completeDebugSmoke", args: { passed: false, checks: ["frontend startup " + code] } });
    }
  });
