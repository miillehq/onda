import { batch, computed, signal } from "@preact/signals";
import { ask, onEvent, toAppError } from "../api";
import type {
  AccountInfo,
  AudioDevice,
  CloudModel,
  DownloadReport,
  MicrophoneReport,
  ModelInfo,
  RuntimeEvent,
  Snapshot,
  UsageReport,
} from "../generated/api";
import type {
  AppError,
  AppEvent,
  DictionaryEntry,
  HistoryEntry,
  LearningProposal,
  Settings,
  Snippet,
  Transform,
} from "../generated/core";
import { presentError } from "./errors";
import { levelFromRms } from "./level";
import { navigate, parseRoute } from "./router";

/* ------------------------------------------------------------------ state */

export const snapshot = signal<Snapshot | null>(null);
export const startupError = signal<AppError | null>(null);
export const dictionary = signal<DictionaryEntry[]>([]);
export const snippets = signal<Snippet[]>([]);
export const transforms = signal<Transform[]>([]);
export const usage = signal<UsageReport | null>(null);
/** Newest first; null until loaded. */
export const history = signal<HistoryEntry[] | null>(null);
export const catalog = signal<ModelInfo[]>([]);
export const downloads = signal<Record<string, DownloadReport>>({});
export const devices = signal<AudioDevice[] | null>(null);
export const oauthStatus = signal<string | null>(null);
export const learning = signal<LearningProposal | null>(null);
/** Streaming ChatGPT text; provisional until `resultReady` validates it. */
export const provisional = signal<{ operationId: string; text: string } | null>(null);
/** 0–1 display level of the open microphone. */
export const micLevel = signal(0);
export const cloudModels = signal<CloudModel[] | null>(null);
/** A quality profile to apply once its model finishes downloading. */
export const pendingProfile = signal<{ modelId: string; patch: Partial<Settings> } | null>(null);

export const settings = computed(() => snapshot.value?.settings ?? null);
export const operation = computed(() => snapshot.value?.state ?? null);
export const lastResult = computed(() => snapshot.value?.result ?? null);
export const installedIds = computed(() => new Set(snapshot.value?.installedModels.map((model) => model.modelId) ?? []));
export const selectedModel = computed(() => catalog.value.find((model) => model.id === snapshot.value?.selectedModelId) ?? null);
export const modelReady = computed(() => {
  const id = snapshot.value?.selectedModelId;
  return id ? installedIds.value.has(id) : false;
});
/** The microphone Onda will use: the chosen one, else the Windows default. */
export const micName = computed(() => {
  const list = devices.value ?? [];
  const id = snapshot.value?.settings.microphoneId ?? null;
  const device = id ? list.find((item) => item.id === id) : list.find((item) => item.isDefault);
  if (device) return device.name;
  return id ? "Disconnected microphone" : "Windows default microphone";
});
export const activeProfile = computed<AccountInfo | null>(() => snapshot.value?.profiles.find((profile) => profile.active) ?? null);
export const aiReady = computed(() => {
  const profile = activeProfile.value;
  return !!profile && profile.signedIn && profile.planUsageEnabled;
});
export const isBusy = computed(() => {
  const phase = operation.value?.phase;
  return phase === "recording" || phase === "finalizingAudio" || phase === "transcribing" || phase === "enhancing" || phase === "inserting";
});

/* ------------------------------------------------------------------ toasts */

export interface Toast {
  id: number;
  tone: "info" | "ok" | "error";
  title: string;
  body?: string;
  action?: { label: string; run: () => void };
}
export const toasts = signal<Toast[]>([]);
let toastId = 0;

export function toast(value: Omit<Toast, "id">, ms = 5200): void {
  const id = (toastId += 1);
  toasts.value = [...toasts.value.slice(-2), { ...value, id }];
  window.setTimeout(() => dismissToast(id), ms);
}

export function dismissToast(id: number): void {
  toasts.value = toasts.value.filter((item) => item.id !== id);
}

export function toastError(error: unknown, title?: string): AppError {
  const value = toAppError(error);
  if (value.code !== "cancelled") toast({ tone: "error", title: title ?? presentError(value).title, body: value.message }, 7000);
  return value;
}

/* ------------------------------------------------------------------ loading */

export async function refresh(): Promise<void> {
  const fresh = await ask({ command: "getSnapshot" }, "snapshot");
  // A settings change still on its way to native code wins over the older stored copy.
  snapshot.value = desiredSettings ? { ...fresh, settings: desiredSettings } : fresh;
}

export async function loadCatalog(): Promise<void> {
  catalog.value = await ask({ command: "getModelCatalog" }, "modelCatalog");
}
export async function loadDictionary(): Promise<void> {
  dictionary.value = await ask({ command: "getDictionary" }, "dictionary");
}
export async function loadSnippets(): Promise<void> {
  snippets.value = await ask({ command: "getSnippets" }, "snippets");
}
export async function loadTransforms(): Promise<void> {
  transforms.value = await ask({ command: "getTransforms" }, "transforms");
}
export async function loadUsage(): Promise<void> {
  usage.value = await ask({ command: "getUsage" }, "usage");
}
export async function loadHistory(): Promise<void> {
  history.value = await ask({ command: "getHistory" }, "history");
}
export async function loadDevices(): Promise<void> {
  devices.value = await ask({ command: "listAudioDevices" }, "audioDevices");
}

let usageTimer: number | undefined;
function scheduleUsage(): void {
  window.clearTimeout(usageTimer);
  usageTimer = window.setTimeout(() => void loadUsage().catch(() => undefined), 400);
}

export async function init(): Promise<void> {
  try {
    await onEvent(handleEvent);
    await refresh();
  } catch (error) {
    startupError.value = toAppError(error);
    throw error;
  }
  await Promise.allSettled([loadCatalog(), loadDictionary(), loadSnippets(), loadTransforms(), loadUsage(), loadHistory()]);
}

/* ------------------------------------------------------------------ settings */

/** True while a settings change is on its way; derived native values (the model in use) lag. */
export const settingsPending = signal(false);
/** The newest settings the UI shows that native code may not have yet. */
let desiredSettings: Settings | null = null;
/** The last settings native code accepted, to roll back to. */
let confirmedSettings: Settings | null = null;
let settingsFlush: Promise<AppError | null> | null = null;

/**
 * Optimistic: the UI updates now and rolls back if native validation refuses. Rapid changes
 * are coalesced and sent one at a time, so replies can never arrive out of order and flicker
 * the page between old and new values.
 */
export async function saveSettings(patch: Partial<Settings>, options: { quiet?: boolean } = {}): Promise<AppError | null> {
  const current = snapshot.value;
  if (!current) return null;
  if (!desiredSettings) confirmedSettings = current.settings;
  const next: Settings = { ...(desiredSettings ?? current.settings), ...patch };
  desiredSettings = next;
  snapshot.value = { ...current, settings: next };
  settingsPending.value = true;
  settingsFlush ??= flushSettings().finally(() => (settingsFlush = null));
  const error = await settingsFlush;
  // One toast per failed burst: only the newest change reports it.
  if (error && !options.quiet && next === lastFailedSettings) toast({ tone: "error", title: "Setting not saved", body: error.message }, 7000);
  return error;
}

let lastFailedSettings: Settings | null = null;

async function flushSettings(): Promise<AppError | null> {
  try {
    for (;;) {
      const target = desiredSettings;
      if (!target) return null;
      await ask({ command: "updateSettings", args: { settings: target } }, "settings");
      confirmedSettings = target;
      if (desiredSettings !== target) continue;
      const fresh = await ask({ command: "getSnapshot" }, "snapshot");
      if (desiredSettings !== target) continue;
      desiredSettings = null;
      snapshot.value = fresh;
      return null;
    }
  } catch (error) {
    lastFailedSettings = desiredSettings;
    desiredSettings = null;
    const latest = snapshot.value;
    if (latest && confirmedSettings) snapshot.value = { ...latest, settings: confirmedSettings };
    return toAppError(error);
  } finally {
    settingsPending.value = false;
  }
}

/* ------------------------------------------------------------------ models */

export async function downloadModel(modelId: string, then?: Partial<Settings>): Promise<void> {
  if (then) pendingProfile.value = { modelId, patch: then };
  downloads.value = { ...downloads.value, [modelId]: { modelId, stage: "downloading", completedBytes: 0, totalBytes: catalog.value.find((model) => model.id === modelId)?.downloadSizeBytes ?? 0 } };
  try {
    await ask({ command: "startModelDownload", args: { modelId } }, "unit");
  } catch (error) {
    clearDownload(modelId);
    if (pendingProfile.value?.modelId === modelId) pendingProfile.value = null;
    toastError(error, "Download didn't start");
  }
}

export async function cancelDownload(modelId: string): Promise<void> {
  clearDownload(modelId);
  if (pendingProfile.value?.modelId === modelId) pendingProfile.value = null;
  await ask({ command: "cancelModelDownload", args: { modelId } }, "unit").catch(() => undefined);
}

function clearDownload(modelId: string): void {
  const { [modelId]: _removed, ...rest } = downloads.value;
  downloads.value = rest;
}

export async function removeModel(modelId: string): Promise<void> {
  try {
    await ask({ command: "uninstallModel", args: { modelId } }, "unit");
    await refresh();
    toast({ tone: "ok", title: "Model removed" });
  } catch (error) {
    toastError(error, "Model not removed");
  }
}

/* ------------------------------------------------------------------ results */

async function run(label: string, request: Parameters<typeof ask>[0]): Promise<boolean> {
  try {
    await ask(request, "unit");
    return true;
  } catch (error) {
    toastError(error, label);
    return false;
  }
}

export async function copyResult(): Promise<void> {
  if (await run("Couldn't copy", { command: "copyResult" })) toast({ tone: "ok", title: "Copied" }, 2400);
}
/** Copy ChatGPT's version that Onda's checks kept out of the document. */
export async function copySuggestion(): Promise<void> {
  if (await run("Couldn't copy", { command: "copySuggestion" })) toast({ tone: "ok", title: "Copied ChatGPT's version" }, 2400);
}
/** Acknowledge a failure so Onda reads as ready again; its text stays in History. */
export const dismissError = (operationId: string) => run("Couldn't dismiss", { command: "dismissError", args: { operationId } });
export const pasteAgain = () => run("Couldn't paste", { command: "retryResult" });
export const undoRewrite = () => run("Couldn't undo", { command: "undoLastTransform" });
/** Forget the visible result, and its History entry too. */
export async function discardResult(): Promise<void> {
  const id = lastResult.value?.operationId;
  if (id && history.value?.some((entry) => entry.id === id)) await deleteHistoryEntry(id);
  if (await run("Couldn't delete", { command: "clearResult" })) {
    batch(() => {
      learning.value = null;
      provisional.value = null;
    });
    await refresh();
  }
}
export async function retryPolish(): Promise<void> {
  try {
    await ask({ command: "retryTransformation" }, "result");
    await refresh();
  } catch (error) {
    toastError(error, "Retry didn't work");
  }
}

/* ------------------------------------------------------------------ history */

export async function copyHistoryEntry(id: string): Promise<void> {
  if (await run("Couldn't copy", { command: "copyHistoryEntry", args: { id } })) toast({ tone: "ok", title: "Copied" }, 2400);
}
export async function copyHistorySuggestion(id: string): Promise<void> {
  if (await run("Couldn't copy", { command: "copyHistorySuggestion", args: { id } })) toast({ tone: "ok", title: "Copied ChatGPT's version" }, 2400);
}

export async function deleteHistoryEntry(id: string): Promise<void> {
  const before = history.value;
  history.value = before?.filter((entry) => entry.id !== id) ?? null;
  if (await run("Couldn't delete", { command: "deleteHistoryEntry", args: { id } })) {
    if (lastResult.value?.operationId === id) await refresh();
  } else {
    history.value = before;
  }
}

export async function clearHistory(): Promise<boolean> {
  if (!(await run("Couldn't clear history", { command: "clearHistory" }))) return false;
  batch(() => {
    history.value = [];
    learning.value = null;
    provisional.value = null;
  });
  await refresh();
  return true;
}

function upsertHistory(entry: HistoryEntry): void {
  const list = history.value;
  if (!list) return;
  history.value = [entry, ...list.filter((item) => item.id !== entry.id)].sort((a, b) => b.createdAt - a.createdAt);
}

export async function acceptLearning(proposal: LearningProposal): Promise<void> {
  try {
    await ask({ command: "acceptLearning", args: { proposalId: proposal.id } }, "unit");
    learning.value = null;
    await loadDictionary();
    toast({ tone: "ok", title: `Onda will write “${proposal.corrected}” from now on` });
  } catch (error) {
    learning.value = null;
    toastError(error, "Word not added");
  }
}

/* ------------------------------------------------------------------ microphone */

export async function testMicrophone(durationMs = 3000): Promise<MicrophoneReport | null> {
  try {
    return await ask({ command: "testMicrophone", args: { durationMs } }, "microphone");
  } catch (error) {
    toastError(error, "Microphone test didn't run");
    return null;
  } finally {
    micLevel.value = 0;
  }
}

/* ------------------------------------------------------------------ events */

function handleEvent(event: RuntimeEvent): void {
  switch (event.type) {
    case "domain":
      handleDomain(event.payload);
      break;
    case "downloadProgress": {
      const report = event.payload;
      downloads.value = { ...downloads.value, [report.modelId]: report };
      if (report.stage === "installed") {
        void refresh().then(() => {
          clearDownload(report.modelId);
          const pending = pendingProfile.value;
          if (pending?.modelId === report.modelId) {
            pendingProfile.value = null;
            void saveSettings(pending.patch);
          }
        });
      }
      break;
    }
    case "oauthChanged":
      oauthStatus.value = event.payload.status;
      if (event.payload.status !== "authorizing") {
        cloudModels.value = null;
        void refresh().catch(() => undefined);
      }
      break;
    case "shortcutStatus": {
      const current = snapshot.value;
      if (current) snapshot.value = { ...current, shortcutStatus: event.payload.status };
      break;
    }
    case "audioLevel":
      micLevel.value = levelFromRms(event.payload.level);
      break;
    case "navigate": {
      const target = parseRoute(event.payload.section);
      if (target) navigate(target);
      break;
    }
    case "historyUpdated":
      upsertHistory(event.payload.entry);
      break;
    case "historyReset":
      void loadHistory().catch(() => undefined);
      break;
  }
}

function handleDomain(event: AppEvent): void {
  const current = snapshot.value;
  switch (event.type) {
    case "stateChanged": {
      if (!current) return;
      const state = event.payload;
      const newOperation = state.operationId !== current.state.operationId;
      batch(() => {
        if (newOperation) {
          learning.value = null;
          provisional.value = null;
        }
        snapshot.value = { ...current, state, result: newOperation && state.phase === "recording" ? null : current.result };
      });
      if (state.phase === "completed") scheduleUsage();
      break;
    }
    case "resultReady":
      if (!current || event.payload.operationId !== current.state.operationId) return;
      batch(() => {
        snapshot.value = { ...current, result: event.payload };
        provisional.value = null;
      });
      break;
    case "learningProposed":
      learning.value = event.payload;
      break;
    case "cloudDelta": {
      if (!current || event.payload.operationId !== current.state.operationId) return;
      const previous = provisional.value?.operationId === event.payload.operationId ? provisional.value.text : "";
      provisional.value = { operationId: event.payload.operationId, text: previous + event.payload.delta };
      break;
    }
    case "error": {
      const error = event.payload;
      if (error.code === "cancelled") return;
      if (error.code === "download" || error.code === "insufficientDisk") {
        downloads.value = Object.fromEntries(Object.entries(downloads.value).filter(([, report]) => report.stage === "installed"));
        pendingProfile.value = null;
      }
      // Operation failures arrive just before their state; those are shown in place, not as toasts.
      window.setTimeout(() => {
        const stateError = snapshot.value?.state.error;
        if (stateError && stateError.message === error.message) return;
        const tone = error.code === "audioLimit" || error.code === "invalidEnhancement" ? "info" : "error";
        toast({ tone, title: presentError(error).title, body: error.message }, 7000);
      }, 80);
      break;
    }
    case "metric":
      break;
  }
}
