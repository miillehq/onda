import type { AppError, ErrorCode } from "../generated/core";
import type { Route } from "./router";

export interface ErrorPresentation {
  title: string;
  /** Extra guidance shown under the native message, when the fix is not obvious. */
  hint?: string;
  action?: { label: string; route: Route };
}

const PRESENTATION: Record<ErrorCode, ErrorPresentation> = {
  busy: { title: "Onda is busy" },
  cancelled: { title: "Cancelled" },
  invalidInput: { title: "That didn't work" },
  storage: { title: "Couldn't save your data", action: { label: "Get help", route: "settings/about" } },
  noMicrophone: { title: "No microphone found", action: { label: "Check microphone", route: "settings/microphone" } },
  noSpeech: { title: "Didn't hear anything", action: { label: "Test microphone", route: "settings/microphone" } },
  shortPress: { title: "Hold to talk" },
  selectionRequired: { title: "Select some text first", hint: "Highlight the text you want to change, then use the shortcut again." },
  audioDisconnected: { title: "Microphone disconnected", action: { label: "Check microphone", route: "settings/microphone" } },
  audioRouteChanged: { title: "Microphone changed", action: { label: "Check microphone", route: "settings/microphone" } },
  audioPermissionDenied: {
    title: "Microphone access is off",
    hint: "In Windows Settings, open Privacy & security › Microphone and allow desktop apps to use it.",
  },
  audioBackend: { title: "Microphone problem", action: { label: "Check microphone", route: "settings/microphone" } },
  audioLimit: { title: "Recording limit reached", action: { label: "Change limit", route: "settings/microphone" } },
  noModel: { title: "Speech model needed", action: { label: "Get a model", route: "settings/speech" } },
  corruptModel: { title: "Speech model needs repair", action: { label: "Open speech settings", route: "settings/speech" } },
  download: { title: "Download didn't finish", action: { label: "Try again", route: "settings/speech" } },
  insufficientDisk: { title: "Not enough disk space", action: { label: "Open speech settings", route: "settings/speech" } },
  inference: { title: "Couldn't write that out", action: { label: "Open speech settings", route: "settings/speech" } },
  inferenceTimeout: { title: "That took too long", hint: "Try the Fast model for quicker results.", action: { label: "Open speech settings", route: "settings/speech" } },
  offline: { title: "Couldn't connect", hint: "Dictation still works. AI polish needs a connection, or its local server running." },
  unauthorized: { title: "Check your AI provider", action: { label: "Open AI polish", route: "settings/ai" } },
  quota: { title: "AI usage limit reached", action: { label: "Open AI polish", route: "settings/ai" } },
  cloud: { title: "AI polish didn't work", action: { label: "Open AI polish", route: "settings/ai" } },
  // The native message names what the AI model changed, e.g. a number or a dictionary word.
  invalidEnhancement: { title: "Polish was skipped" },
  // Typing failures carry a native message saying whether the words went to the clipboard.
  focusChanged: { title: "Focus moved", action: { label: "Open History", route: "history" } },
  targetExited: { title: "The app closed", action: { label: "Open History", route: "history" } },
  passwordField: { title: "Password fields are skipped", hint: "For your safety, Onda never types into password fields." },
  elevatedTarget: { title: "Can't type into admin apps", action: { label: "Open History", route: "history" } },
  unsupportedControl: { title: "This field isn't supported", action: { label: "Open History", route: "history" } },
  clipboardBusy: { title: "Clipboard is busy", action: { label: "Open History", route: "history" } },
  clipboardUnsupported: { title: "Clipboard can't be restored", hint: "Your clipboard holds something Onda can't safely put back, so it didn't type. Your words are saved in History.", action: { label: "Open History", route: "history" } },
  insertion: { title: "Couldn't type it", action: { label: "Open History", route: "history" } },
  shortcut: { title: "Shortcut unavailable", action: { label: "Open shortcuts", route: "settings/shortcuts" } },
  internal: { title: "Something went wrong", action: { label: "Get help", route: "settings/about" } },
};

export function presentError(error: AppError): ErrorPresentation {
  return PRESENTATION[error.code] ?? PRESENTATION.internal;
}
