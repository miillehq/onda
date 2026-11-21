/** Fictional fixtures for the labelled development backend. */
import type { AccountInfo, CloudModel, ModelInfo, ResourceUsage, Snapshot } from "../generated/api";
import type {
  AiProvider,
  DailyUsage,
  HistoryEntry,
  OperationResult,
  OperationState,
  Profile,
  Settings,
  Snippet,
  Transform,
  UsageSummary,
} from "../generated/core";
import { DEFAULT_SETTINGS, MACOS_DEFAULT_SETTINGS } from "../generated/core";
import { createDictionary } from "./dictionary";

const params = new URLSearchParams(window.location.search);
const scenario = params.get("mock") ?? "default";
const pillState = params.get("pill");
const platform = params.get("platform") === "linux" ? "linux" : params.get("platform") === "macos" ? "macos" : "windows";
/** What a fresh profile starts with on this platform. */
const defaults = platform === "macos" ? MACOS_DEFAULT_SETTINGS : DEFAULT_SETTINGS;
const provider = (params.get("provider") ?? "chatgpt") as AiProvider;

const themeParam = params.get("theme") === "dark" ? "dark" : params.get("theme") === "light" ? "light" : null;
const settings: Settings = scenario === "onboarding" ? { ...structuredClone(defaults), theme: themeParam ?? defaults.theme } : {
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
  pushToTalk: structuredClone(defaults.pushToTalk),
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
const dictionary = createDictionary(rich
  ? [
      { id: "d1", kind: "vocabulary", phrase: "Kubernetes", replacement: null, enabled: true, starred: true, source: "manual" },
      { id: "d2", kind: "vocabulary", phrase: "miille", replacement: null, enabled: true, starred: false, source: "manual" },
      { id: "d3", kind: "vocabulary", phrase: "Onda", replacement: null, enabled: true, starred: false, source: "manual" },
      { id: "d4", kind: "vocabulary", phrase: "PostgreSQL", replacement: null, enabled: true, starred: false, source: "imported" },
      { id: "d5", kind: "vocabulary", phrase: "Tauri", replacement: null, enabled: false, starred: false, source: "manual" },
      { id: "d6", kind: "replacement", phrase: "cooper netties", replacement: "Kubernetes", enabled: true, starred: false, source: "autoLearned" },
      { id: "d7", kind: "replacement", phrase: "my email", replacement: "name@example.com", enabled: true, starred: false, source: "manual" },
    ]
  : []);
const snippets: Snippet[] = rich
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
const history: HistoryEntry[] = rich
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
  typingAccess: platform === "windows" ? "allowed" : "notAllowed",
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

const profile: Profile = { name: rich ? "Alex" : null, picture: null };

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

export {
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
  snippets as initialSnippets,
  profiles,
  sample,
  history as initialHistory,
  snapshot,
  today,
  key,
  usageDays,
  summarize,
  profile as initialProfile,
  providerModels,
  resources,
  samplePicture,
};
