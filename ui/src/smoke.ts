/** Exercises the actual hidden webview (WebView2, or WebKitGTK on Linux) -> Tauri -> native facade
 *  in an isolated debug profile. */
import { call } from "./api";
import type { ApiRequest } from "./generated/api";
import { refresh } from "./lib/store";

const UI_SETTLE_TIMEOUT_MS = 3000;
const UI_POLL_INTERVAL_MS = 25;

/** Waits for asynchronous native events and UI requests without assuming renderer speed. */
async function waitForUi(predicate: () => boolean): Promise<boolean> {
  const deadline = performance.now() + UI_SETTLE_TIMEOUT_MS;
  while (!predicate()) {
    if (performance.now() >= deadline) return false;
    await new Promise(resolve => setTimeout(resolve, UI_POLL_INTERVAL_MS));
  }
  return true;
}

export async function runSmoke(): Promise<void> {
  const checks: string[] = [];
  const check = (condition: boolean, label: string): void => { if (!condition) throw new Error(label); checks.push(label); };
  const rejected = async (request: ApiRequest, code: string): Promise<boolean> => { try { await call(request); return false; } catch (error) { return typeof error === "object" && error !== null && "code" in error && error.code === code; } };
  try {
    const snapshot = await call({ command: "getSnapshot" });
    check(snapshot.type === "snapshot" && snapshot.payload.contractVersion === 1 && !snapshot.payload.settings.enhancementEnabled && !snapshot.payload.settings.contextEnabled && !snapshot.payload.settings.learningEnabled, "private default snapshot");
    const cloudFailureCode = snapshot.type === "snapshot" && snapshot.payload.cloudError ? "cloud" : "unauthorized";
    // Skip first-run setup so the main window renders; everything else keeps private defaults.
    const baseline = snapshot.type === "snapshot" ? { ...snapshot.payload.settings, onboardingCompleted: true } : null;
    if (baseline) await call({ command: "updateSettings", args: { settings: baseline } });
    await refresh();
    check(await waitForUi(() => !!document.querySelector('[data-testid="status"]')), "main window renders after setup");
    try {
      await call({ command: "previewTransform", args: { source: " \n ", transformId: "polish" } });
      check(false, "empty preview must fail before cloud work");
    } catch (error) {
      check(typeof error === "object" && error !== null && "code" in error && error.code === "invalidInput" && "message" in error && String(error.message).includes("Enter or select") && !String(error.message).includes("128"), "empty preview has an actionable missing input error");
    }
    const word = { id: "smoke-word", kind: "vocabulary" as const, phrase: "PrivateCanaryToken", replacement: null, enabled: true, starred: false, source: "manual" as const };
    check((await call({ command: "saveDictionaryEntry", args: { entry: word } })).type === "dictionary", "dictionary save through IPC");
    check(await rejected({ command: "saveDictionaryEntry", args: { entry: { ...word, id: "smoke-duplicate" } } }, "invalidInput"), "duplicate phrase refused");
    const snippet = { id: "smoke-snippet", trigger: "test trigger", expansion: "🌊 Unicode output", enabled: true };
    check((await call({ command: "saveSnippet", args: { snippet } })).type === "snippets", "Unicode snippet save through IPC");
    check(await rejected({ command: "saveSnippet", args: { snippet: { ...snippet, id: "bad-trigger", trigger: "bad\0trigger" } } }, "invalidInput"), "invalid snippet refused before commit");
    check(await rejected({ command: "listAvailableModels" }, cloudFailureCode), "local use works with unavailable cloud");
    check(await rejected({ command: "previewTransform", args: { source: "PrivateCanaryTranscript", transformId: "polish" } }, cloudFailureCode), "offline transform reports actionable error");
    const retained = await call({ command: "getSnapshot" });
    check(retained.type === "snapshot" && retained.payload.result?.original === "PrivateCanaryTranscript" && retained.payload.state.phase === "error", "failed transform retains original and terminal state");
    check(await rejected({command:"retryTransformation"},cloudFailureCode),"transformation retry preserves unavailable cloud input");
    const previewHistory = await call({ command: "getHistory" });
    check(previewHistory.type === "history" && previewHistory.payload.length === 0, "typed previews stay out of history");
    if (baseline) {
      check(baseline.historyDays === 3, "history defaults to three days");
      check(await rejected({ command: "updateSettings", args: { settings: { ...baseline, historyDays: 31 } } }, "invalidInput"), "history retention is capped at thirty days");
      check((await call({ command: "updateSettings", args: { settings: { ...baseline, historyDays: 0 } } })).type === "settings", "session-only history accepted");
      await call({ command: "updateSettings", args: { settings: baseline } });
    }
    const diagnostic = await call({ command: "exportDiagnostics" });
    check(diagnostic.type === "text" && !diagnostic.payload.includes("PrivateCanary"), "diagnostics exclude sensitive content");
    check((await call({ command: "getTransforms" })).type === "transforms", "built-in transform catalog available");
    check((await call({ command: "getModelCatalog" })).type === "modelCatalog", "trusted local model catalog available");
    try {
      await call({command:"startDictation",args:{commandMode:false}});
      check(false,"missing selected model must fail before recording");
    } catch(error) {
      check(typeof error === "object" && error !== null && "code" in error && error.code === "noModel", "missing selected model has a specific error");
    }
    const missingModel = await call({command:"getSnapshot"});
    check(missingModel.type === "snapshot" && missingModel.payload.selectedModelId === "whisper-tiny" && !!missingModel.payload.state.error?.message.includes("whisper-tiny"),"snapshot identifies the missing selected model");
    await refresh();
    const modelError = () => document.querySelector('[data-testid="operation-error"]')?.textContent ?? "";
    const statusModel = () => document.querySelector<HTMLElement>('[data-testid="status"]')?.dataset.modelId ?? "";
    check(await waitForUi(() => modelError().includes("whisper-tiny")),"dictation state displays the specific model error");
    if(baseline){
      const settings={...baseline,qualityProfile:"custom" as const,modelId:"whisper-small-q5"};
      await call({command:"updateSettings",args:{settings}});
      const selected=await call({command:"getSnapshot"});
      check(selected.type==="snapshot"&&selected.payload.selectedModelId==="whisper-small-q5"&&selected.payload.state.phase==="idle", "explicit small selection resolves native model and clears stale model error");
      await refresh();
      check(await waitForUi(() => statusModel() === "whisper-small-q5"), "model panel displays native custom selection");
      await call({command:"updateSettings",args:{settings:baseline}});
      await refresh();
      check(await waitForUi(() => statusModel() === "whisper-tiny"), "applying a profile refreshes selected model display");
    }
    check(await rejected({ command: "teachWord", args: { durationMs: 100 } }, "invalidInput"), "teaching take length is bounded");
    check(await rejected({ command: "teachWord", args: { durationMs: 2500 } }, "noModel"), "teaching a word needs a speech model before recording");
    check((await call({ command: "listAudioDevices" })).type === "audioDevices", "audio device enumeration available");
    // Profile and statistics: stored locally, normalized, and never invented.
    const named = await call({ command: "setProfileName", args: { name: "  Smoke   Tester " } });
    check(named.type === "profile" && named.payload.name === "Smoke Tester", "profile name normalized and stored");
    check(await rejected({ command: "setProfileName", args: { name: "x".repeat(41) } }, "invalidInput"), "overlong profile name refused");
    const usageReport = await call({ command: "getUsage" });
    check(usageReport.type === "usage" && usageReport.payload.summary.dictations === 0 && usageReport.payload.summary.since === null && usageReport.payload.summary.currentStreak === 0, "fresh profile has no invented statistics");
    check(await rejected({ command: "saveStatsImage", args: { pngBase64: btoa("<svg/>") } }, "invalidInput"), "only PNG statistics images are saved");
    // AI providers: addresses are checked before saving and failures name what to fix.
    const current = await call({ command: "getSettings" });
    if (current.type === "settings") {
      check(await rejected({ command: "updateSettings", args: { settings: { ...current.payload, aiProvider: "local", localBaseUrl: "http://192.168.1.2:11434/v1" } } }, "invalidInput"), "local provider refuses addresses off this computer");
      check(await rejected({ command: "updateSettings", args: { settings: { ...current.payload, openaiBaseUrl: "http://api.example.com/v1" } } }, "invalidInput"), "remote provider needs HTTPS");
      check(await rejected({ command: "updateSettings", args: { settings: { ...current.payload, language: "yue" } } }, "invalidInput"), "languages the models lack are refused");
      check((await call({ command: "updateSettings", args: { settings: { ...current.payload, aiProvider: "local", localBaseUrl: "http://127.0.0.1:9/v1", localModel: "smoke" } } })).type === "settings", "local provider settings saved");
      check(await rejected({ command: "listProviderModels", args: { provider: "local" } }, "offline"), "unreachable local server reported as offline");
      check(await rejected({ command: "saveProviderKey", args: { provider: "anthropic", key: "not a key" } }, "invalidInput"), "malformed API key refused before storage");
      await call({ command: "updateSettings", args: { settings: current.payload } });
    }
    // The whole process tree, web view included, as Task Manager does not show it.
    const resources = await call({ command: "getResourceUsage" });
    check(resources.type === "resources" && resources.payload.processes.some((process) => process.role === "app") && resources.payload.totalMemoryBytes > 0, "resource report measures the Onda process");
    check(resources.type === "resources" && resources.payload.processes.some((process) => process.role === "webview"), "resource report includes the web view processes");
    if(window.__ONDA_REPLAY__){const replay=await call({command:"replayDebugFixture"});check(replay.type==="result"&&replay.payload.inserted,"real CPU fixture reached controlled Windows field");const completed=await call({command:"getSnapshot"});check(completed.type==="snapshot"&&completed.payload.state.phase==="completed"&&completed.payload.state.route==="local","full dictation pipeline completed locally");
      const replayed=replay.type==="result"?replay.payload:null;const kept=await call({command:"getHistory"});const entry=kept.type==="history"?kept.payload.find(item=>item.id===replayed?.operationId):undefined;check(!!entry&&entry.inserted&&entry.mode==="dictation"&&entry.output===replayed?.output&&entry.errorCode===null,"fixture dictation recorded in history");
      check((await call({command:"deleteHistoryEntry",args:{id:entry?.id??""}})).type==="unit","history entry deleted");const cleared=await call({command:"getHistory"});check(cleared.type==="history"&&!cleared.payload.some(item=>item.id===entry?.id),"deleted entry is gone");}
    await call({ command: "clearResult" });
    await call({ command: "completeDebugSmoke", args: { passed: true, checks } });
  } catch(error) {
    if(typeof error === 'object' && error !== null && 'code' in error) checks.push('failure code '+String(error.code));
    await call({ command: "completeDebugSmoke", args: { passed: false, checks } });
  }
}
