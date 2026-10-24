import { computed, signal } from "@preact/signals";

export const SETTINGS_SECTIONS = [
  "general",
  "shortcuts",
  "microphone",
  "speech",
  "ai",
  "apps",
  "privacy",
  "about",
] as const;
export type SettingsSection = (typeof SETTINGS_SECTIONS)[number];
export type MainPage = "home" | "history" | "dictionary" | "snippets" | "rewrites";
export type Route = MainPage | `settings/${SettingsSection}` | "onboarding";

const MAIN_PAGES: MainPage[] = ["home", "history", "dictionary", "snippets", "rewrites"];

export function parseRoute(value: string): Route | null {
  if ((MAIN_PAGES as string[]).includes(value) || value === "onboarding") return value as Route;
  if (value === "settings") return "settings/general";
  const match = /^settings\/([a-z]+)$/.exec(value);
  if (match && (SETTINGS_SECTIONS as readonly string[]).includes(match[1] ?? "")) return value as Route;
  return null;
}

export const route = signal<Route>("home");
/** Where "Back" from settings returns to. */
const lastMainPage = signal<MainPage>("home");

export const inSettings = computed(() => route.value.startsWith("settings/"));

export function navigate(to: Route): void {
  if (!to.startsWith("settings/") && to !== "onboarding") lastMainPage.value = to as MainPage;
  route.value = to;
}

export function leaveSettings(): void {
  route.value = lastMainPage.value;
}
