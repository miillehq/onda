import { computed, signal } from "@preact/signals";
import type { AccountInfo, AudioDevice, CloudModel, DownloadReport, ModelInfo, Snapshot, UsageReport } from "../generated/api";
import type {
  AppError,
  DictionaryEntry,
  HistoryEntry,
  LearningProposal,
  Profile,
  Settings,
  Snippet,
  Transform,
} from "../generated/core";
import { cancelChord } from "./keys";

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
/** Streaming AI text; provisional until `resultReady` validates it. */
export const provisional = signal<{ operationId: string; text: string } | null>(null);
/** 0–1 display level of the open microphone. */
export const micLevel = signal(0);
export const cloudModels = signal<CloudModel[] | null>(null);
/** A quality profile to apply once its model finishes downloading. */
export const pendingProfile = signal<{ modelId: string; patch: Partial<Settings> } | null>(null);
/** The local profile (name and picture); loaded with the rest at start. */
export const profile = signal<Profile | null>(null);

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
  return id ? "Disconnected microphone" : isWindows.value ? "Windows default microphone" : "System default microphone";
});
/** "windows", "linux", …: where Onda can type into other apps and how shortcuts work. */
export const platform = computed(() => snapshot.value?.platform ?? "windows");
export const isWindows = computed(() => platform.value === "windows");
export const isMac = computed(() => platform.value === "macos");
/** "this PC" on Windows, "this computer" elsewhere; `yourDevice` likewise. */
export const thisDevice = computed(() => (isWindows.value ? "this PC" : "this computer"));
export const yourDevice = computed(() => (isWindows.value ? "your PC" : "your computer"));
/** Linux desktops without a shortcuts portal start dictation through `onda --dictate` instead. */
export const commandShortcutOnly = computed(() => !isWindows.value && snapshot.value?.shortcutStatus === "desktop");
/** The chord that cancels on this platform (`cancelChord`). */
export function cancelKeys(pushToTalk: readonly number[]): number[] {
  return cancelChord(platform.value, pushToTalk);
}
export const activeProfile = computed<AccountInfo | null>(() => snapshot.value?.profiles.find((profile) => profile.active) ?? null);
/** A ChatGPT account is connected and allowed to use its plan. */
export const chatgptReady = computed(() => {
  const profile = activeProfile.value;
  return !!profile && profile.signedIn && profile.planUsageEnabled;
});
/** The chosen AI provider has what it needs: an account, or an address, model and key. */
export const aiReady = computed(() => {
  const value = settings.value;
  const keys = snapshot.value?.providerKeys;
  switch (value?.aiProvider) {
    case "chatgpt":
      return chatgptReady.value;
    case "openaiCompatible":
      return !!value.openaiBaseUrl && !!value.openaiModel;
    case "anthropic":
      return !!value.anthropicModel && !!keys?.anthropic;
    case "local":
      return !!value.localBaseUrl && !!value.localModel;
    default:
      return false;
  }
});
export const isBusy = computed(() => {
  const phase = operation.value?.phase;
  return phase === "recording" || phase === "finalizingAudio" || phase === "transcribing" || phase === "enhancing" || phase === "inserting";
});
