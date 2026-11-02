/**
 * DEVELOPMENT MOCK. Loaded only by `vite dev` outside the desktop shell, for visual QA.
 * Never bundled into production (see api.ts). Data below is clearly fictional sample data.
 *
 * URL parameters:
 *   ?mock=default|empty|onboarding|error|skipped   initial scenario
 *   ?theme=light|dark                      force a theme
 *   ?pill=listening|handsfree|writing|polishing|done|error|nottyped|skipped|cycle   pill window state
 *   ?pill=code&code=focusChanged[&mode=transform][&text=1][&suggestion=1]   any error card
 *   ?platform=linux                        Linux variants (clipboard handoff, desktop shortcut)
 *   ?provider=chatgpt|openaiCompatible|anthropic|local   AI polish provider
 */
import type { Backend } from "../api";
import type { AccountInfo, ApiRequest, ApiResponse, CloudModel, ModelInfo, ResourceUsage, RuntimeEvent, Snapshot } from "../generated/api";
import type { AiProvider, AppError, DailyUsage, DictionaryEntry, HistoryEntry, OperationResult, OperationState, Profile, Settings, Snippet, Transform, UsageSummary } from "../generated/core";

const params = new URLSearchParams(window.location.search);
const scenario = params.get("mock") ?? "default";
const pillState = params.get("pill");
const platform = params.get("platform") === "linux" ? "linux" : "windows";
const provider = (params.get("provider") ?? "chatgpt") as AiProvider;

const settings: Settings = {
  executionPreference: "auto",
  qualityProfile: "fast",
  modelId: null,
  modelDirectory: null,
  language: "en",
  microphoneId: null,
  maxRecordingSeconds: 180,
  enhancementEnabled: scenario === "default",
  contextEnabled: false,
  learningEnabled: true,
  pushToTalk: { keys: [0xa2, 0x5b, 0x20] },
  transformShortcut: { keys: [0xa2, 0xa4, 0x50] },
  commandShortcut: { keys: [0xa2, 0xa4, 0x43] },
  toggleShortcut: { keys: [0xa2, 0xa4, 0x44] },
  toggleEnabled: true,
  defaultTransformId: "polish",
  appProfiles: scenario === "default" ? [{ executable: "slack.exe", contextEnabled: false, enhancementEnabled: true, cleanupInstruction: "Casual and brief. No sign-off.", pasteShortcut: "controlV" }] : [],
  theme: params.get("theme") === "dark" ? "dark" : params.get("theme") === "light" ? "light" : "system",
  launchAtLogin: true,
  showPill: true,
  playSounds: true,
  onboardingCompleted: scenario !== "onboarding",
  historyDays: 3,
  trayIcon: "auto",
  aiProvider: provider,
  openaiBaseUrl: provider === "openaiCompatible" ? "https://api.example.com/v1" : null,
  openaiModel: provider === "openaiCompatible" ? "example-small" : null,
  anthropicModel: provider === "anthropic" ? "claude-opus-5-5" : null,
  localBaseUrl: provider === "local" ? "http://127.0.0.1:11434/v1" : null,
  localModel: provider === "local" ? "qwen3.5:0.8b" : null,
};

const catalog: ModelInfo[] = [
  ["whisper-tiny-en", "Fast · English", 77_704_715, 400, true],
  ["whisper-tiny", "Fast · Multilingual", 77_691_713, 400, false],
  ["whisper-base-en-q5", "Balanced · English", 59_721_011, 600, true],
  ["whisper-base-q5", "Balanced · Multilingual", 59_707_625, 600, false],
  ["whisper-small-en-q5", "Quality · English", 190_098_681, 1200, true],
  ["whisper-small-q5", "Quality · Multilingual", 190_085_487, 1200, false],
].map(([id, name, size, memory, englishOnly]) => ({ id: id as string, name: name as string, version: "sample", downloadSizeBytes: size as number, memoryBytes: (memory as number) * 1024 * 1024, license: "MIT", englishOnly: englishOnly as boolean, sha256: "" }));

const transforms: Transform[] = [
  ["polish", "Polish", "Polish this text while preserving its meaning and voice."],
  ["grammar", "Fix grammar", "Fix spelling, grammar and punctuation only."],
  ["concise", "Make concise", "Make this text concise while retaining every material point."],
  ["clearer", "Make clearer", "Improve clarity without inventing information."],
  ["formal", "Make formal", "Rewrite in a professional formal tone with the same meaning."],
  ["casual", "Make casual", "Rewrite in a natural casual tone with the same meaning."],
  ["bullets", "Turn into bullets", "Format the existing points as bullet items without adding information."],
  ["translate", "Translate", "Translate this text into English, preserving meaning, URLs, email addresses and numeric values."],
  ["prompt", "Prompt engineer", "Improve this as an instruction prompt. Clarify requirements and organize existing information."],
].map(([id, name, instruction]) => ({ id: id as string, name: name as string, instruction: instruction as string, shortcut: null, modelPreference: null, preserveProtectedTokens: true, builtIn: true }));

const rich = scenario === "default" || scenario === "error" || scenario === "skipped";
/** A rewrite whose ChatGPT version changed a number, so Onda kept the selection. */
const SKIPPED_TEXT = "Send the invoice for 1,200 by Friday and copy finance.";
const SKIPPED_SUGGESTION = "Please send the invoice for 1,500 by Friday and copy the finance team.";
const SKIPPED_ERROR = { code: "invalidEnhancement", message: "The AI model changed a number, link, email or code, so Onda kept your original words.", retryable: true } as const;
let dictionary: DictionaryEntry[] = rich
  ? [
      { id: "d1", kind: "vocabulary", phrase: "Kubernetes", replacement: null, enabled: true, starred: true, source: "manual" },
      { id: "d2", kind: "vocabulary", phrase: "miille", replacement: null, enabled: true, starred: false, source: "manual" },
      { id: "d3", kind: "vocabulary", phrase: "Onda", replacement: null, enabled: true, starred: false, source: "manual" },
      { id: "d4", kind: "vocabulary", phrase: "PostgreSQL", replacement: null, enabled: true, starred: false, source: "imported" },
      { id: "d5", kind: "vocabulary", phrase: "Tauri", replacement: null, enabled: false, starred: false, source: "manual" },
      { id: "d6", kind: "replacement", phrase: "cooper netties", replacement: "Kubernetes", enabled: true, starred: false, source: "autoLearned" },
      { id: "d7", kind: "replacement", phrase: "my email", replacement: "name@example.com", enabled: true, starred: false, source: "manual" },
    ]
  : [];
let snippets: Snippet[] = rich
  ? [
      { id: "s1", trigger: "my address", expansion: "221B Example Street\nLondon NW1 6XE", enabled: true },
      { id: "s2", trigger: "calendar link", expansion: "https://example.com/book-a-call", enabled: true },
      { id: "s3", trigger: "sign off", expansion: "Thanks,\nAlex", enabled: false },
    ]
  : [];

const profiles: AccountInfo[] = scenario === "default" ? [{ id: "p1", label: "Personal", email: "you@example.com", signedIn: true, planUsageEnabled: true, active: true, preferredModel: null }] : [];

const result: OperationResult | null =
  scenario === "skipped"
    ? { commandText: null, operationId: "op-sample", original: SKIPPED_TEXT, output: SKIPPED_TEXT, inserted: false, fallbackUsed: false, diff: [{ kind: "equal", text: SKIPPED_TEXT }], suggestion: SKIPPED_SUGGESTION }
    : rich
  ? {
      commandText: null,
      operationId: "op-sample",
      original: "lets move the design review to thursday at three and um send the notes after",
      output: "Let's move the design review to Thursday at three and send the notes after.",
      inserted: true,
      fallbackUsed: false,
      diff: [
        { kind: "removed", text: "lets move the design review to thursday at three and um send the notes after" },
        { kind: "added", text: "Let's move the design review to Thursday at three and send the notes after." },
      ],
      suggestion: null,
    }
  : null;

const minutesAgo = (minutes: number) => Date.now() - minutes * 60_000;
function sample(id: string, minutes: number, app: string, output: string, extra: Partial<HistoryEntry> = {}): HistoryEntry {
  return { id, createdAt: minutesAgo(minutes), mode: "dictation", route: "local", original: output, output, commandText: null, inserted: true, fallbackUsed: false, errorCode: null, app, suggestion: null, ...extra };
}
let history: HistoryEntry[] = rich
  ? [
      sample("op-sample", 2, "slack.exe", "Let's move the design review to Thursday at three and send the notes after.", { route: "chatgptPlan", original: "lets move the design review to thursday at three and um send the notes after" }),
      sample("h2", 18, "outlook.exe", "Hi Priya, thanks for the quick turnaround. The numbers look right to me, so let's send the deck to the client tomorrow morning.", { inserted: false, errorCode: "focusChanged" }),
      sample("h3", 47, "code.exe", "Move standup to 9 tomorrow and invite the two new hires.", { route: "chatgptPlan", fallbackUsed: true, original: "move standup to 9 tomorrow and invite the two new hires", suggestion: "Move standup to 9:00 tomorrow and invite the two new hires." }),
      sample("h4", 95, "winword.exe", "The pilot ran for six weeks across three teams. Adoption was strongest where managers used it first.", { mode: "spokenCommand", commandText: "make it more formal", original: "pilot went for six weeks, three teams, people used it more when their managers did", route: "chatgptPlan" }),
      sample("h5", 60 * 26, "notion.exe", "Ideas for the offsite: a short retro, a demo hour, and dinner somewhere walkable."),
      sample("h6", 60 * 27, "chrome.exe", "Order confirmation for the standing desk, delivery window Tuesday."),
    ]
  : [];

function idle(): OperationState {
  if (scenario === "skipped") return { operationId: "op-sample", phase: "error", mode: "transform", route: "chatgptPlan", error: SKIPPED_ERROR, fallbackUsed: false, handsFree: false };
  return { operationId: rich ? "op-sample" : null, phase: scenario === "error" ? "error" : rich ? "completed" : "idle", mode: "dictation", route: rich ? "chatgptPlan" : "local", error: scenario === "error" ? { code: "noSpeech", message: "No speech was detected. Check microphone level and try again.", retryable: true } : null, fallbackUsed: false, handsFree: false };
}

const snapshot: Snapshot = {
  appName: "Onda",
  version: "0.2.0",
  contractVersion: 1,
  state: idle(),
  settings,
  selectedModelId: "whisper-tiny-en",
  result,
  hardware: { logicalCpus: 16, physicalCpus: 8, totalMemoryBytes: 32 * 1024 ** 3, availableMemoryBytes: 18 * 1024 ** 3, inferenceThreads: 8, simd: ["avx2"], gpus: [{ name: "Sample GPU", vendorId: 0, dedicatedMemoryBytes: 8 * 1024 ** 3, sharedMemoryBytes: 0 }], vulkanLoaderPresent: true, cudaDriverPresent: false, directmlLibraryPresent: true, vulkanWorkerInstalled: false },
  installedModels: scenario === "empty" || scenario === "onboarding" ? [] : [{ modelId: "whisper-tiny-en", version: "sample", sizeBytes: 77_704_715 }],
  profiles,
  cloudError: null,
  shortcutStatus: "ready",
  updaterConfigured: false,
  platform,
  providerKeys: { openaiCompatible: provider === "openaiCompatible", anthropic: provider === "anthropic", local: false },
  typingAccess: platform === "linux" ? "notAllowed" : "allowed",
};

function today(): Date {
  return new Date();
}
function key(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}
/** About four months of fictional, deterministic activity, the latest week matching Home. */
const usageDays: DailyUsage[] = rich
  ? Array.from({ length: 130 }, (_, offset) => {
      const recent = [412, 655, 503, 820, 588, 0, 0][offset];
      const wave = Math.sin(offset * 1.7) * Math.cos(offset * 0.43);
      const words = recent ?? (wave > 0.15 ? Math.round(120 + wave * 900) : 0);
      const date = today();
      date.setDate(date.getDate() - offset);
      return { day: key(date), dictations: Math.round(words / 22), words, audioMs: Math.round((words / 142) * 60_000) };
    }).filter((day) => day.words > 0)
  : [];

/** The same rules as onda-core's stats::summarize, for the mock only. */
function summarize(days: DailyUsage[]): UsageSummary {
  const dayNumber = (value: string) => Math.round(Date.parse(`${value}T00:00:00Z`) / 86_400_000);
  const active = [...new Set(days.filter((day) => day.dictations > 0).map((day) => dayNumber(day.day)))].sort((a, b) => a - b);
  const todayNumber = dayNumber(key(today()));
  const has = (value: number) => active.includes(value);
  let current = 0;
  let cursor = has(todayNumber) ? todayNumber : has(todayNumber - 1) ? todayNumber - 1 : null;
  while (cursor !== null && has(cursor)) {
    current += 1;
    cursor -= 1;
  }
  let longest = 0;
  let run = 0;
  active.forEach((value, index) => {
    run = index > 0 && active[index - 1] === value - 1 ? run + 1 : 1;
    longest = Math.max(longest, run);
  });
  const words = days.reduce((sum, day) => sum + day.words, 0);
  const audioMs = days.reduce((sum, day) => sum + day.audioMs, 0);
  const since = days.length ? days.map((day) => day.day).sort()[0] ?? null : null;
  return { dictations: days.reduce((sum, day) => sum + day.dictations, 0), words, audioMs, since, wordsPerMinute: audioMs >= 60_000 && words > 0 ? words / (audioMs / 60_000) : null, currentStreak: current, longestStreak: longest, activeDays: active.length };
}

let profile: Profile = { name: rich ? "Alex" : null, picture: null };

const providerModels: Record<Exclude<AiProvider, "chatgpt">, CloudModel[]> = {
  openaiCompatible: [
    { slug: "example-small", name: "example-small", recommended: false },
    { slug: "example-large", name: "example-large", recommended: false },
  ],
  anthropic: [
    { slug: "claude-opus-5-5", name: "Claude Opus 5.5", recommended: false },
    { slug: "claude-sonnet-5-5", name: "Claude Sonnet 5.5", recommended: false },
    { slug: "claude-haiku-5-5", name: "Claude Haiku 5.5", recommended: false },
  ],
  local: [
    { slug: "qwen3.5:0.8b", name: "qwen3.5:0.8b", recommended: false },
    { slug: "qwen3.5:2b", name: "qwen3.5:2b", recommended: false },
  ],
};

/** A fictional process tree shaped like Onda's on each platform. */
function resources(): ResourceUsage {
  const mb = 1024 * 1024;
  const processes =
    platform === "linux"
      ? [
          { pid: 4100, name: "onda", role: "app", memoryBytes: 61 * mb, cpuPercent: 0.2 },
          { pid: 4122, name: "WebKitNetworkPr", role: "webview", memoryBytes: 14 * mb, cpuPercent: 0 },
          { pid: 4131, name: "WebKitWebProces", role: "webview", memoryBytes: 96 * mb, cpuPercent: 0.4 },
          { pid: 4132, name: "WebKitWebProces", role: "webview", memoryBytes: 41 * mb, cpuPercent: 0 },
        ]
      : [
          { pid: 9120, name: "onda.exe", role: "app", memoryBytes: 8.8 * mb, cpuPercent: 0 },
          { pid: 9144, name: "onda-input.exe", role: "helper", memoryBytes: 1.0 * mb, cpuPercent: 0 },
          { pid: 9160, name: "msedgewebview2.exe", role: "webview", memoryBytes: 19.7 * mb, cpuPercent: 0 },
          { pid: 9168, name: "msedgewebview2.exe", role: "webview", memoryBytes: 55.0 * mb, cpuPercent: 0.1 },
          { pid: 9172, name: "msedgewebview2.exe", role: "webview", memoryBytes: 4.2 * mb, cpuPercent: 0 },
          { pid: 9176, name: "msedgewebview2.exe", role: "webview", memoryBytes: 1.6 * mb, cpuPercent: 0 },
          { pid: 9190, name: "msedgewebview2.exe", role: "webview", memoryBytes: 57.2 * mb, cpuPercent: 0.3 },
          { pid: 9204, name: "msedgewebview2.exe", role: "webview", memoryBytes: 25.3 * mb, cpuPercent: 0 },
        ];
  return {
    processes,
    totalMemoryBytes: processes.reduce((sum, process) => sum + process.memoryBytes, 0),
    totalCpuPercent: processes.reduce((sum, process) => sum + process.cpuPercent, 0),
    memoryMeasure: platform === "linux" ? "privateResident" : "privateWorkingSet",
    sampleMs: 500,
  };
}

/** A small fictional avatar: a soft gradient square, drawn so no real picture is involved. */
function samplePicture(): string {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 256;
  const context = canvas.getContext("2d");
  if (context) {
    const gradient = context.createLinearGradient(0, 0, 256, 256);
    gradient.addColorStop(0, "#8a8a92");
    gradient.addColorStop(1, "#2c2c30");
    context.fillStyle = gradient;
    context.fillRect(0, 0, 256, 256);
  }
  return canvas.toDataURL("image/png").replace(/^data:image\/png;base64,/, "");
}

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
  snapshot.result = { commandText: null, operationId: id, original: "sure i can send the summary by friday", output, inserted: true, fallbackUsed: false, diff: [{ kind: "removed", text: "sure i can send the summary by friday" }, { kind: "added", text: output }], suggestion: null };
  domain({ type: "resultReady", payload: snapshot.result });
  const entry = sample(id, 0, "slack.exe", output, { original: "sure i can send the summary by friday", route: snapshot.state.route });
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

function respond(request: ApiRequest): ApiResponse | Promise<ApiResponse> {
  switch (request.command) {
    case "getSnapshot":
      return { type: "snapshot", payload: structuredClone(snapshot) };
    case "getSettings":
      return { type: "settings", payload: structuredClone(settings) };
    case "updateSettings":
      Object.assign(settings, request.args.settings);
      snapshot.settings = settings;
      snapshot.selectedModelId = settings.qualityProfile === "custom" && settings.modelId ? settings.modelId : ({ fast: ["whisper-tiny-en", "whisper-tiny"], balanced: ["whisper-base-en-q5", "whisper-base-q5"], bestQuality: ["whisper-small-en-q5", "whisper-small-q5"], custom: ["whisper-tiny-en", "whisper-tiny"] } as const)[settings.qualityProfile][settings.language === "en" ? 0 : 1];
      return { type: "settings", payload: structuredClone(settings) };
    case "getDictionary":
      return { type: "dictionary", payload: dictionary };
    case "saveDictionaryEntry":
      dictionary = [...dictionary.filter((entry) => entry.id !== request.args.entry.id), request.args.entry];
      return { type: "dictionary", payload: dictionary };
    case "deleteDictionaryEntry":
      dictionary = dictionary.filter((entry) => entry.id !== request.args.id);
      return { type: "unit" };
    case "importDictionary":
      return { type: "count", payload: 0 };
    case "exportDictionary":
      return { type: "text", payload: JSON.stringify({ version: 1, entries: dictionary }, null, 2) };
    case "getSnippets":
      return { type: "snippets", payload: snippets };
    case "saveSnippet":
      snippets = [...snippets.filter((snippet) => snippet.id !== request.args.snippet.id), request.args.snippet];
      return { type: "snippets", payload: snippets };
    case "deleteSnippet":
      snippets = snippets.filter((snippet) => snippet.id !== request.args.id);
      return { type: "unit" };
    case "getTransforms":
      return { type: "transforms", payload: transforms };
    case "saveTransform": {
      const index = transforms.findIndex((transform) => transform.id === request.args.transform.id);
      if (index >= 0) transforms[index] = request.args.transform;
      else transforms.push(request.args.transform);
      return { type: "transforms", payload: transforms };
    }
    case "listAudioDevices":
      return { type: "audioDevices", payload: [{ id: "mic-1", name: "Microphone Array (2- Intel® Smart Sound Technology for Digital Microphones)", isDefault: true }, { id: "mic-2", name: "Headset Microphone (Sample)", isDefault: false }] };
    case "testMicrophone":
      return speak(request.args.durationMs).then(() => ({ type: "microphone", payload: { durationMs: request.args.durationMs, peak: 0.42, rms: 0.05, clippedSamples: 0, discontinuities: 0, mostlySilent: false, sourceSampleRate: 48000, sourceChannels: 1 } }));
    case "startDictation":
      void dictate(false);
      return { type: "unit" };
    case "getModelCatalog":
      return { type: "modelCatalog", payload: catalog };
    case "getInstalledModels":
      return { type: "installedModels", payload: snapshot.installedModels };
    case "startModelDownload": {
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
    }
    case "listChatgptProfiles":
      return { type: "profiles", payload: profiles };
    case "listAvailableModels":
      return { type: "cloudModels", payload: [{ slug: "thorough-sample", name: "Thorough (sample)", recommended: false }, { slug: "light-sample", name: "Light (sample)", recommended: true }] };
    case "previewTransform": {
      const output = request.args.source.trim().replace(/^./, (c) => c.toUpperCase());
      const id = `preview-${Date.now()}`;
      // Like the native runtime, a rewrite tried in the app is kept in History.
      if (output !== request.args.source) {
        const entry = sample(id, 0, "", output, { mode: "transform", route: "chatgptPlan", original: request.args.source, inserted: false, app: null });
        history = [entry, ...history];
        emit({ type: "historyUpdated", payload: { entry } });
      }
      return { type: "result", payload: { commandText: null, operationId: id, original: request.args.source, output, inserted: false, fallbackUsed: false, diff: [{ kind: "equal", text: request.args.source }], suggestion: null } };
    }
    case "retryTransformation":
      return { type: "result", payload: snapshot.result as OperationResult };
    case "exportDiagnostics":
      return { type: "text", payload: JSON.stringify({ app: "Onda", version: "0.2.0", note: "Sample diagnostics from the development mock." }, null, 2) };
    case "checkUpdate":
      return { type: "update", payload: { available: false, version: null } };
    case "getUsage":
      return { type: "usage", payload: { today: key(today()), days: usageDays, summary: summarize(usageDays) } };
    case "getProfile":
      return { type: "profile", payload: profile };
    case "setProfileName": {
      const name = request.args.name?.trim().replace(/\s+/g, " ") ?? "";
      if ([...name].length > 40) throw { code: "invalidInput", message: "Use a name of at most 40 characters.", retryable: false } satisfies AppError;
      profile = { ...profile, name: name || null };
      return { type: "profile", payload: profile };
    }
    case "chooseProfilePicture":
      profile = { ...profile, picture: samplePicture() };
      return { type: "profile", payload: profile };
    case "removeProfilePicture":
      profile = { ...profile, picture: null };
      return { type: "profile", payload: profile };
    case "saveStatsImage": {
      // The mock offers the image as a browser download so its rendering can be inspected.
      const link = document.createElement("a");
      link.href = `data:image/png;base64,${request.args.pngBase64}`;
      link.download = "Onda stats.png";
      link.click();
      return { type: "savedFile", payload: { path: "~/Pictures/Onda stats.png" } };
    }
    case "listProviderModels": {
      if (request.args.provider === "chatgpt") return { type: "cloudModels", payload: [] };
      if (scenario === "providerdown")
        throw { code: "offline", message: request.args.provider === "local" ? "Nothing answered at 127.0.0.1:11434. Start your local model server (Ollama, LM Studio or llama.cpp) and try again." : "Couldn't reach api.example.com.", retryable: true } satisfies AppError;
      return wait(300).then(() => ({ type: "cloudModels", payload: providerModels[request.args.provider as Exclude<AiProvider, "chatgpt">] }));
    }
    case "saveProviderKey":
    case "removeProviderKey": {
      const saved = request.command === "saveProviderKey";
      const field = ({ openaiCompatible: "openaiCompatible", anthropic: "anthropic", local: "local" } as const)[request.args.provider as Exclude<AiProvider, "chatgpt">];
      if (field) snapshot.providerKeys = { ...snapshot.providerKeys, [field]: saved };
      return { type: "unit" };
    }
    case "getResourceUsage":
      return wait(500).then(() => ({ type: "resources", payload: resources() }));
    case "allowTyping":
      return wait(600).then(() => {
        snapshot.typingAccess = "allowed";
        return { type: "unit" };
      });
    case "stopTyping":
      snapshot.typingAccess = "notAllowed";
      return { type: "unit" };
    case "restartShortcuts":
      snapshot.shortcutStatus = "ready";
      return { type: "unit" };
    case "signOutChatgptProfile":
      return { type: "signOut", payload: { remoteRevocationConfirmed: true } };
    case "clearResult":
      snapshot.result = null;
      return { type: "unit" };
    case "dismissError":
      if (snapshot.state.phase === "error" && snapshot.state.operationId === request.args.operationId) setState({ phase: "idle", error: null });
      return { type: "unit" };
    case "teachWord": {
      // Fictional takes of the brand name "Miille": two mishearings, then a correct one.
      const heard = ["Miley", "Mealy", "Miille"][teachTakes++ % 3] ?? "Miley";
      return speak(request.args.durationMs).then(() => wait(500)).then(() => ({ type: "teachTake", payload: { heard, written: heard } }));
    }
    case "getHistory":
      return { type: "history", payload: history };
    case "deleteHistoryEntry":
      history = history.filter((entry) => entry.id !== request.args.id);
      if (snapshot.result?.operationId === request.args.id) snapshot.result = null;
      return { type: "unit" };
    case "clearHistory":
      history = [];
      snapshot.result = null;
      return { type: "unit" };
    default:
      return { type: "unit" };
  }
}

export function createMockBackend(): Backend {
  console.info("[Onda] Using the development mock backend. Data is fictional.");
  let started = false;
  return {
    call: async (request) => {
      await wait(40);
      return respond(request);
    },
    onEvent: async (callback) => {
      listeners.add(callback);
      if (!started) {
        started = true;
        if (pillState === "cycle") void wait(400).then(cycle);
        else if (pillState) void wait(200).then(() => freezePill(pillState));
        else if (rich) window.setTimeout(() => domain({ type: "learningProposed", payload: { id: "lp1", original: "Cooper Netties", corrected: "Kubernetes" } }), 600);
      }
      return () => listeners.delete(callback);
    },
  };
}
