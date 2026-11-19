import { ask } from "../api";
import { dictionary, snippets, transforms, usage, history, catalog, devices, profile } from "./appState";

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
export async function loadProfile(): Promise<void> {
  profile.value = await ask({ command: "getProfile" }, "profile");
}
export async function loadDevices(): Promise<void> {
  devices.value = await ask({ command: "listAudioDevices" }, "audioDevices");
}
