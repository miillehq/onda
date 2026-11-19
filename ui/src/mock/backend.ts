/**
 * DEVELOPMENT MOCK. Loaded only by `vite dev` outside the desktop shell, for visual QA.
 * Never bundled into production (see api.ts). Data below is clearly fictional sample data.
 *
 * URL parameters:
 *   ?mock=default|empty|onboarding|error|skipped   initial scenario
 *   ?theme=light|dark                      force a theme
 *   ?pill=listening|handsfree|writing|polishing|done|error|nottyped|skipped|cycle   pill window state
 *   ?pill=code&code=focusChanged[&mode=transform][&text=1][&suggestion=1]   any error card
 *   ?platform=linux|macos                  Linux or Mac variants (clipboard handoff, shortcuts, wording)
 *   ?provider=chatgpt|openaiCompatible|anthropic|local   AI polish provider
 */
import type { Backend } from "../api";
import type { ApiRequest, ApiResponse, RuntimeEvent } from "../generated/api";
import type { AiProvider, AppError, OperationResult, OperationState, } from "../generated/core";
import { normalizeName, validateSettings } from "./validate";
import { requireRequestShape } from "./request-shape";
import {
  params,
  scenario,
  pillState,
  platform,
  settings,
  catalog,
  transforms,
  rich,
  SKIPPED_TEXT,
  SKIPPED_SUGGESTION,
  SKIPPED_ERROR,
  dictionary,
  initialSnippets,
  profiles,
  sample,
  initialHistory,
  snapshot,
  today,
  key,
  usageDays,
  summarize,
  initialProfile,
  providerModels,
  resources,
  samplePicture,
} from "./fixtures";

let snippets = initialSnippets;
let history = initialHistory;
let profile = initialProfile;

/* A Mac answers "allow typing" before the user flips Onda's switch in System Settings. The mock
 * flips it when the window gets focus back, as if the user had done it and returned. */
let macSwitchedOnInSettings = false;
if (platform === "macos")
  window.addEventListener("focus", () => {
    if (macSwitchedOnInSettings) snapshot.typingAccess = "allowed";
  });

let teachTakes = 0;
const listeners = new Set<(event: RuntimeEvent) => void>();
const emit = (event: RuntimeEvent) => listeners.forEach((listener) => listener(event));
const domain = (payload: Extract<RuntimeEvent, { type: "domain" }>["payload"]) => emit({ type: "domain", payload });
const wait = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms));

function setState(patch: Partial<OperationState>): void {
  snapshot.state = { ...snapshot.state, ...patch };
  domain({ type: "stateChanged", payload: snapshot.state });
}

async function speak(ms: number): Promise<void> {
  const end = performance.now() + ms;
  while (performance.now() < end) {
    const t = performance.now() / 1000;
    emit({ type: "audioLevel", payload: { level: 0.004 + 0.09 * Math.abs(Math.sin(t * 3.1) * Math.sin(t * 1.7)) } });
    await wait(50);
  }
  emit({ type: "audioLevel", payload: { level: 0 } });
}

async function dictate(handsFree = false): Promise<void> {
  const id = `op-${Date.now()}`;
  snapshot.result = null;
  setState({ operationId: id, phase: "recording", mode: "dictation", route: "local", error: null, fallbackUsed: false, handsFree });
  await speak(2600);
  setState({ phase: "finalizingAudio" });
  setState({ phase: "transcribing" });
  await wait(700);
  if (settings.enhancementEnabled && profiles.length) {
    setState({ phase: "enhancing", route: "chatgptPlan" });
    for (const word of "Sure, I can send the summary by Friday.".split(" ")) {
      domain({ type: "cloudDelta", payload: { operationId: id, delta: `${word} ` } });
      await wait(90);
    }
  }
  const output = "Sure, I can send the summary by Friday.";
  // Like the native runtime: without permission to type, the words go to the clipboard instead.
  const inserted = snapshot.typingAccess === "allowed";
  snapshot.result = { commandText: null, operationId: id, original: "sure i can send the summary by friday", output, inserted, fallbackUsed: false, diff: [{ kind: "removed", text: "sure i can send the summary by friday" }, { kind: "added", text: output }], suggestion: null };
  domain({ type: "resultReady", payload: snapshot.result });
  const entry = sample(id, 0, "slack.exe", output, { original: "sure i can send the summary by friday", route: snapshot.state.route, inserted });
  history = [entry, ...history];
  emit({ type: "historyUpdated", payload: { entry } });
  setState({ phase: "inserting" });
  await wait(250);
  setState({ phase: "completed" });
}

/** Messages as the native runtime words them, longest variant where there are several. */
const SAMPLE_ERROR_MESSAGES: Partial<Record<AppError["code"], string>> = {
  focusChanged: "Onda only types where you started dictating. Your words are on the clipboard, so press Ctrl+V to paste them.",
  targetExited: "Nothing was typed. Your words are on the clipboard, so press Ctrl+V to paste them.",
  elevatedTarget: "Windows doesn't let Onda type into apps run as administrator. Your words are on the clipboard, so press Ctrl+V to paste them.",
  unsupportedControl: "Onda can't type into this kind of field. Your words are on the clipboard, so press Ctrl+V to paste them.",
  clipboardBusy: "Another app was holding the clipboard. Your words are saved in History.",
  insertion: "The app didn't accept the paste. Your words are on the clipboard, so press Ctrl+V to paste them.",
  invalidEnhancement: "ChatGPT rewrote far more than a cleanup should, so Onda kept your original words.",
  shortPress: "Keep holding the keys while you speak, then let go to type.",
  noSpeech: "No speech was recognized. Check the selected microphone with Test level while speaking, then hold the dictation shortcut, speak and release.",
  unauthorized: "No ChatGPT account is connected. Connect one in AI polish to use this.",
  noModel: "Fast · English (whisper-tiny-en) is selected but not downloaded yet. Download it in Settings › Speech and language, or pick a model you already have.",
  audioPermissionDenied: "Windows denied microphone access. Enable microphone access for desktop apps in Windows privacy settings and retry.",
  audioBackend: "The microphone driver rejected its recording format. Select another input in Settings and retry.",
  selectionRequired: "Select text in your target app before using a transform or spoken editing shortcut.",
};

function freezePill(state: string): void {
  const base: OperationState = { operationId: "pill", phase: "recording", mode: "dictation", route: "local", error: null, fallbackUsed: false, handsFree: false };
  const states: Record<string, OperationState> = {
    listening: base,
    handsfree: { ...base, handsFree: true },
    writing: { ...base, phase: "transcribing" },
    polishing: { ...base, phase: "enhancing", route: "chatgptPlan" },
    done: { ...base, phase: "completed" },
    error: { ...base, phase: "error", error: { code: "noSpeech", message: "No speech was detected. Check microphone level and try again.", retryable: true } },
    nottyped: { ...base, phase: "error", error: { code: "focusChanged", message: "The focused app, control, or selection changed. Your text is available to copy.", retryable: true } },
    skipped: { ...base, phase: "error", mode: "transform", route: "chatgptPlan", error: SKIPPED_ERROR },
  };
  // One card per error code, for checking that every combination of message and buttons fits.
  const code = state === "code" ? (params.get("code") as AppError["code"] | null) : null;
  const mode = (params.get("mode") ?? "dictation") as OperationState["mode"];
  const target = code ? { ...base, phase: "error" as const, mode, error: { code, message: SAMPLE_ERROR_MESSAGES[code] ?? "Sample failure message.", retryable: true } } : states[state] ?? base;
  domain({ type: "stateChanged", payload: target });
  if (state === "nottyped") {
    const output = "Hi Priya, thanks for the quick turnaround.";
    domain({ type: "resultReady", payload: { commandText: null, operationId: "pill", original: output, output, inserted: false, fallbackUsed: false, diff: [{ kind: "equal", text: output }], suggestion: null } });
  }
  if (code && params.get("text")) {
    const output = "Hi Priya, thanks for the quick turnaround on the deck.";
    const original = mode === "dictation" ? output : "hi priya thanks for the quick turnaround on the deck";
    domain({ type: "resultReady", payload: { commandText: null, operationId: "pill", original, output, inserted: false, fallbackUsed: false, diff: [{ kind: "equal", text: output }], suggestion: params.get("suggestion") ? `${output} Talk soon!` : null } });
  }
  if (state === "skipped") {
    domain({ type: "resultReady", payload: { commandText: null, operationId: "pill", original: SKIPPED_TEXT, output: SKIPPED_TEXT, inserted: false, fallbackUsed: false, diff: [{ kind: "equal", text: SKIPPED_TEXT }], suggestion: SKIPPED_SUGGESTION } });
  }
  if (target.phase === "recording") {
    const tick = () => {
      const t = performance.now() / 1000;
      emit({ type: "audioLevel", payload: { level: 0.01 + 0.08 * Math.abs(Math.sin(t * 3.1) * Math.sin(t * 1.7)) } });
    };
    window.setInterval(tick, 50);
  }
}

async function cycle(): Promise<void> {
  for (;;) {
    await dictate(false);
    await wait(1600);
    setState({ operationId: `op-${Date.now()}`, phase: "recording", error: null, handsFree: false });
    await wait(300);
    setState({ phase: "error", error: { code: "noSpeech", message: "No speech was detected. Check microphone level and try again.", retryable: true } });
    await wait(4000);
  }
}

type Reply = ApiResponse | Promise<ApiResponse>;
type Handlers = { [C in ApiRequest["command"]]: (request: Extract<ApiRequest, { command: C }>) => Reply };
const unit = (): ApiResponse => ({ type: "unit" });

function changeProviderKey(request: Extract<ApiRequest, { command: "saveProviderKey" | "removeProviderKey" }>): Reply {
  const saved = request.command === "saveProviderKey";
  const field = ({ openaiCompatible: "openaiCompatible", anthropic: "anthropic", local: "local" } as const)[request.args.provider as Exclude<AiProvider, "chatgpt">];
  if (field) snapshot.providerKeys = { ...snapshot.providerKeys, [field]: saved };
  return { type: "unit" };
}

const handlers: Handlers = {
  getSnapshot: () => ({ type: "snapshot", payload: structuredClone(snapshot) }),
  getSettings: () => ({ type: "settings", payload: structuredClone(settings) }),
  updateSettings: (request) => {
    const refusal = validateSettings(request.args.settings);
    if (refusal) throw refusal;
      if (!transforms.some((item) => item.id === request.args.settings.defaultTransformId)) {
        throw { code: "invalidInput", message: "Choose an existing default rewrite.", retryable: false } satisfies AppError;
      }
    Object.assign(settings, request.args.settings);
    snapshot.settings = settings;
    snapshot.selectedModelId = settings.qualityProfile === "custom" && settings.modelId ? settings.modelId : ({ fast: ["whisper-tiny-en", "whisper-tiny"], balanced: ["whisper-base-en-q5", "whisper-base-q5"], bestQuality: ["whisper-small-en-q5", "whisper-small-q5"], custom: ["whisper-tiny-en", "whisper-tiny"] } as const)[settings.qualityProfile][settings.language === "en" ? 0 : 1];
    return { type: "settings", payload: structuredClone(settings) };
  },
  getDictionary: dictionary,
  saveDictionaryEntry: dictionary,
  deleteDictionaryEntry: dictionary,
  importDictionary: dictionary,
  exportDictionary: dictionary,
  getSnippets: () => ({ type: "snippets", payload: snippets }),
  saveSnippet: (request) => {
    snippets = [...snippets.filter((snippet) => snippet.id !== request.args.snippet.id), request.args.snippet];
    return { type: "snippets", payload: snippets };
  },
  deleteSnippet: (request) => {
    snippets = snippets.filter((snippet) => snippet.id !== request.args.id);
    return { type: "unit" };
  },
  getTransforms: () => ({ type: "transforms", payload: transforms }),
  saveTransform: (request) => {
    const index = transforms.findIndex((transform) => transform.id === request.args.transform.id);
    if (index >= 0) transforms[index] = request.args.transform;
    else transforms.push(request.args.transform);
    return { type: "transforms", payload: transforms };
  },
  listAudioDevices: () => ({ type: "audioDevices", payload: [{ id: "mic-1", name: "Microphone Array (2- Intel® Smart Sound Technology for Digital Microphones)", isDefault: true }, { id: "mic-2", name: "Headset Microphone (Sample)", isDefault: false }] }),
  testMicrophone: (request) => speak(request.args.durationMs).then(() => ({ type: "microphone", payload: { durationMs: request.args.durationMs, peak: 0.42, rms: 0.05, clippedSamples: 0, discontinuities: 0, mostlySilent: false, sourceSampleRate: 48000, sourceChannels: 1 } })),
  startDictation: () => {
    void dictate(false);
    return { type: "unit" };
  },
  getModelCatalog: () => ({ type: "modelCatalog", payload: catalog }),
  getInstalledModels: () => ({ type: "installedModels", payload: snapshot.installedModels }),
  startModelDownload: (request) => {
    const model = catalog.find((item) => item.id === request.args.modelId);
    void (async () => {
      const total = model?.downloadSizeBytes ?? 1;
      for (let step = 1; step <= 20; step += 1) {
        await wait(150);
        emit({ type: "downloadProgress", payload: { modelId: request.args.modelId, stage: "downloading", completedBytes: (total * step) / 20, totalBytes: total } });
      }
      emit({ type: "downloadProgress", payload: { modelId: request.args.modelId, stage: "verifying", completedBytes: total, totalBytes: total } });
      await wait(600);
      snapshot.installedModels = [...snapshot.installedModels, { modelId: request.args.modelId, version: "sample", sizeBytes: total }];
      emit({ type: "downloadProgress", payload: { modelId: request.args.modelId, stage: "installed", completedBytes: total, totalBytes: total } });
    })();
    return { type: "unit" };
  },
  listChatgptProfiles: () => ({ type: "profiles", payload: profiles }),
  listAvailableModels: () => ({ type: "cloudModels", payload: [{ slug: "thorough-sample", name: "Thorough (sample)", recommended: false }, { slug: "light-sample", name: "Light (sample)", recommended: true }] }),
  previewTransform: (request) => {
    const output = request.args.source.trim().replace(/^./, (c) => c.toUpperCase());
    const id = `preview-${Date.now()}`;
    // Like the native runtime, a rewrite tried in the app is kept in History.
    if (output !== request.args.source) {
      const entry = sample(id, 0, "", output, { mode: "transform", route: "chatgptPlan", original: request.args.source, inserted: false, app: null });
      history = [entry, ...history];
      emit({ type: "historyUpdated", payload: { entry } });
    }
    return { type: "result", payload: { commandText: null, operationId: id, original: request.args.source, output, inserted: false, fallbackUsed: false, diff: [{ kind: "equal", text: request.args.source }], suggestion: null } };
  },
  retryTransformation: () => ({ type: "result", payload: snapshot.result as OperationResult }),
  exportDiagnostics: () => ({ type: "text", payload: JSON.stringify({ app: "Onda", version: "0.2.0", note: "Sample diagnostics from the development mock." }, null, 2) }),
  checkUpdate: () => ({ type: "update", payload: { available: false, version: null } }),
  getUsage: () => ({ type: "usage", payload: { today: key(today()), days: usageDays, summary: summarize(usageDays) } }),
  getProfile: () => ({ type: "profile", payload: profile }),
  setProfileName: (request) => {
    const result = normalizeName(request.args.name);
    if ("error" in result) throw result.error;
    profile = { ...profile, name: result.name };
    return { type: "profile", payload: profile };
  },
  chooseProfilePicture: () => {
    profile = { ...profile, picture: samplePicture() };
    return { type: "profile", payload: profile };
  },
  removeProfilePicture: () => {
    profile = { ...profile, picture: null };
    return { type: "profile", payload: profile };
  },
  saveStatsImage: (request) => {
    // The mock offers the image as a browser download so its rendering can be inspected.
    const link = document.createElement("a");
    link.href = `data:image/png;base64,${request.args.pngBase64}`;
    link.download = "Onda stats.png";
    link.click();
    return { type: "savedFile", payload: { path: "~/Pictures/Onda stats.png" } };
  },
  listProviderModels: (request) => {
    if (request.args.provider === "chatgpt") return { type: "cloudModels", payload: [] };
    if (scenario === "providerdown")
      throw { code: "offline", message: request.args.provider === "local" ? "Nothing answered at 127.0.0.1:11434. Start your local model server (Ollama, LM Studio or llama.cpp) and try again." : "Couldn't reach api.example.com.", retryable: true } satisfies AppError;
    return wait(300).then(() => ({ type: "cloudModels", payload: providerModels[request.args.provider as Exclude<AiProvider, "chatgpt">] }));
  },
  saveProviderKey: changeProviderKey,
  removeProviderKey: changeProviderKey,
  getResourceUsage: () => wait(500).then(() => ({ type: "resources", payload: resources() })),
  allowTyping: () => {
    return wait(600).then(() => {
      if (platform === "macos") {
        macSwitchedOnInSettings = true;
        throw { code: "cancelled", message: "Turn on Onda in System Settings › Privacy & Security › Accessibility. Until then, dictation is copied for you to paste.", retryable: true } satisfies AppError;
      }
      snapshot.typingAccess = "allowed";
      return { type: "unit" };
    });
  },
  stopTyping: () => {
    // A Mac opens System Settings; Onda stays on until the user switches it off there.
    if (platform !== "macos") snapshot.typingAccess = "notAllowed";
    return { type: "unit" };
  },
  restartShortcuts: () => {
    snapshot.shortcutStatus = "ready";
    return { type: "unit" };
  },
  signOutChatgptProfile: () => ({ type: "signOut", payload: { remoteRevocationConfirmed: true } }),
  clearResult: () => {
    snapshot.result = null;
    return { type: "unit" };
  },
  dismissError: (request) => {
    if (snapshot.state.phase === "error" && snapshot.state.operationId === request.args.operationId) setState({ phase: "idle", error: null });
    return { type: "unit" };
  },
  teachWord: (request) => {
    // Fictional takes of the brand name "Miille": two mishearings, then a correct one.
    const heard = ["Miley", "Mealy", "Miille"][teachTakes++ % 3] ?? "Miley";
    return speak(request.args.durationMs).then(() => wait(500)).then(() => ({ type: "teachTake", payload: { heard, written: heard } }));
  },
  getHistory: () => ({ type: "history", payload: history }),
  deleteHistoryEntry: (request) => {
    history = history.filter((entry) => entry.id !== request.args.id);
    if (snapshot.result?.operationId === request.args.id) snapshot.result = null;
    return { type: "unit" };
  },
  clearHistory: () => {
    history = [];
    snapshot.result = null;
    return { type: "unit" };
  },
  acceptLearning: unit,
  cancelChatgptSignIn: unit,
  cancelModelDownload: unit,
  cancelOperation: unit,
  completeDebugSmoke: unit,
  copyHistoryEntry: unit,
  copyHistorySuggestion: unit,
  copyResult: unit,
  copySuggestion: unit,
  deleteTransform: (request) => {
      const index = transforms.findIndex((item) => item.id === request.args.id);
      if (transforms[index]?.builtIn) {
        throw { code: "invalidInput", message: "Built-in rewrites can be edited but cannot be removed.", retryable: false } satisfies AppError;
      }
      if (index >= 0) transforms.splice(index, 1);
      if (settings.defaultTransformId === request.args.id) settings.defaultTransformId = "polish";
      return { type: "unit" };
    },
  dismissPill: unit,
  enableChatgptPlanUsage: unit,
  installUpdate: unit,
  openMainWindow: unit,
  removeChatgptProfile: unit,
  replayDebugFixture: unit,
  retryResult: unit,
  selectChatgptProfile: unit,
  setChatgptModel: unit,
  startChatgptSignIn: unit,
  stopDictation: unit,
  transformSelection: unit,
  undoLastTransform: unit,
  uninstallModel: unit,
};

function respond(request: ApiRequest): Reply {
  if (!Object.hasOwn(handlers, request.command)) {
    throw { code: "invalidInput", message: "Unknown desktop command.", retryable: false } satisfies AppError;
  }
  // The mapped type checks every handler against its own discriminant. Indexing with
  // the union loses that correlation in TypeScript.
  const handler = handlers[request.command] as (value: ApiRequest) => Reply;
  return handler(request);
}

export function createMockBackend(): Backend {
  console.info("[Onda] Using the development mock backend. Data is fictional.");
  let started = false;
  return {
    call: async (request) => {
      requireRequestShape(request);
      await wait(40);
      return respond(request);
    },
    onEvent: async (callback) => {
      listeners.add(callback);
      if (!started) {
        started = true;
        const initialPill = pillState;
        if (initialPill === "cycle") void wait(400).then(cycle);
        else if (initialPill) void wait(200).then(() => freezePill(initialPill));
        else if (rich) window.setTimeout(() => domain({ type: "learningProposed", payload: { id: "lp1", original: "Cooper Netties", corrected: "Kubernetes" } }), 600);
      }
      return () => listeners.delete(callback);
    },
  };
}
