import type { DataRoute, Settings } from "../generated/core";

const integer = new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 });

export function formatInteger(value: number): string {
  return integer.format(Math.round(value));
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  if (bytes >= 10 * 1024 ** 2) return `${Math.round(bytes / 1024 ** 2)} MB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

export function formatDuration(ms: number): string {
  const totalMinutes = Math.round(ms / 60000);
  if (totalMinutes < 1) return `${Math.max(0, Math.round(ms / 1000))} s`;
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours === 0) return `${minutes} min`;
  return minutes === 0 ? `${hours} h` : `${hours} h ${minutes} m`;
}

export function formatClock(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

export function plural(count: number, one: string, many = `${one}s`): string {
  return `${formatInteger(count)} ${count === 1 ? one : many}`;
}

export function greeting(now = new Date()): string {
  const hour = now.getHours();
  if (hour < 5) return "Working late";
  if (hour < 12) return "Good morning";
  if (hour < 18) return "Good afternoon";
  return "Good evening";
}

/** Who receives text for AI polish under these settings, for sentences such as "goes to …". */
export function providerName(value: Settings): string {
  switch (value.aiProvider) {
    case "openaiCompatible": {
      try {
        return value.openaiBaseUrl ? new URL(value.openaiBaseUrl).host : "your AI server";
      } catch {
        return "your AI server";
      }
    }
    case "anthropic":
      return "Anthropic";
    case "local":
      return "the model on this computer";
    default:
      return "ChatGPT";
  }
}

/** Where an AI step sent the text, or null when nothing left Onda's own pipeline. */
export function routeLabel(route: DataRoute): string | null {
  switch (route) {
    case "chatgptPlan":
      return "Polished with ChatGPT";
    case "openaiCompatible":
      return "Polished by your AI server";
    case "anthropic":
      return "Polished with Claude";
    case "localModel":
      return "Polished on this computer";
    default:
      return null;
  }
}

/** The paste chord to press when the words wait on the clipboard: Command-V on a Mac. */
export function pasteKeys(mac: boolean): string {
  return mac ? "\u2318V" : "Ctrl+V";
}

/** "Good afternoon, Daniel" with the first word of the profile name, or just the greeting. */
export function greetingFor(name: string | null | undefined, now = new Date()): string {
  const first = name?.trim().split(/\s+/)[0];
  return first ? `${greeting(now)}, ${first}` : greeting(now);
}

/** "slack.exe" → "Slack"; keeps unusual names readable without guessing branding. */
export function appName(executable: string): string {
  const base = executable.replace(/\.exe$/i, "");
  const known: Record<string, string> = {
    outlook: "Outlook", winword: "Word", excel: "Excel", powerpnt: "PowerPoint", onenote: "OneNote", teams: "Teams",
    "ms-teams": "Teams", slack: "Slack", discord: "Discord", chrome: "Chrome", msedge: "Edge", firefox: "Firefox",
    code: "VS Code", cursor: "Cursor", notepad: "Notepad", notion: "Notion", obsidian: "Obsidian", whatsapp: "WhatsApp",
    telegram: "Telegram", zoom: "Zoom", windowsterminal: "Terminal", figma: "Figma", thunderbird: "Thunderbird",
  };
  return known[base.toLowerCase()] ?? (base.charAt(0).toUpperCase() + base.slice(1));
}
