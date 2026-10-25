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
  offline: { title: "You're offline", hint: "Dictation still works on this PC. AI polish needs a connection." },
  unauthorized: { title: "Reconnect ChatGPT", action: { label: "Open AI polish", route: "settings/ai" } },
  quota: { title: "ChatGPT limit reached", action: { label: "Open AI polish", route: "settings/ai" } },
  cloud: { title: "ChatGPT didn't respond", action: { label: "Open AI polish", route: "settings/ai" } },
  // The native message names what ChatGPT changed, e.g. a number or a dictionary word.
  invalidEnhancement: { title: "Polish was skipped" },
  focusChanged: { title: "You switched fields", hint: "Onda didn't type because focus moved. Your words are saved in History.", action: { label: "Open History", route: "history" } },
  targetExited: { title: "The app closed", hint: "Your words are saved in History.", action: { label: "Open History", route: "history" } },
  passwordField: { title: "Password fields are skipped", hint: "For your safety, Onda never types into password fields." },
  elevatedTarget: { title: "Can't type into admin apps", hint: "Apps running as administrator block Onda. Copy your words from History instead.", action: { label: "Open History", route: "history" } },
  unsupportedControl: { title: "This field isn't supported", hint: "Your words are saved in History.", action: { label: "Open History", route: "history" } },
  clipboardBusy: { title: "Clipboard is busy", hint: "Another app was using the clipboard. Your words are saved in History.", action: { label: "Open History", route: "history" } },
  clipboardUnsupported: { title: "Clipboard can't be restored", hint: "Your clipboard holds something Onda can't safely put back, so it didn't type. Your words are saved in History.", action: { label: "Open History", route: "history" } },
  insertion: { title: "Couldn't type it", hint: "Your words are saved in History.", action: { label: "Open History", route: "history" } },
  shortcut: { title: "Shortcut unavailable", action: { label: "Open shortcuts", route: "settings/shortcuts" } },
  internal: { title: "Something went wrong", action: { label: "Get help", route: "settings/about" } },
};

export function presentError(error: AppError): ErrorPresentation {
  return PRESENTATION[error.code] ?? PRESENTATION.internal;
}
