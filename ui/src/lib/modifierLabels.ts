export type ModifierKind = "ctrl" | "shift" | "alt" | "win";

/** Modifier names as the keyboard prints them. WebView2 reports Windows in its user agent and
 *  WKWebView reports Macintosh; a Mac reads the Windows key as Command and Alt as Option, and
 *  Linux calls the Windows key Super. */
export function modifierLabels(userAgent: string): Record<ModifierKind, string> {
  if (userAgent.includes("Windows")) return { ctrl: "Ctrl", shift: "Shift", alt: "Alt", win: "Win" };
  if (userAgent.includes("Macintosh")) return { ctrl: "Control", shift: "Shift", alt: "Option", win: "Command" };
  return { ctrl: "Ctrl", shift: "Shift", alt: "Alt", win: "Super" };
}
