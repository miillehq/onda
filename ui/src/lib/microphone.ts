// What the microphone settings say, in the words of the user's own system.
import type { MicrophoneReport } from "../generated/api";

export type MicVerdict = { tone: "ok" | "warn" | "error"; title: string; body: string };

/** How a microphone test sounded, and what to change if it was too quiet or too loud. */
export function micVerdict(report: MicrophoneReport, platform: string): MicVerdict {
  const settings = soundSettings(platform);
  if (report.mostlySilent || report.peak < 0.02) return { tone: "error", title: "We couldn't hear you", body: "Check the mic isn't muted and that it's the one you're speaking into." };
  if (report.clippedSamples > 0 || report.peak >= 0.99) return { tone: "warn", title: "A bit loud", body: `Your voice is clipping. Move back from the mic or lower its input volume in ${settings}.` };
  if (report.rms < 0.01) return { tone: "warn", title: "A bit quiet", body: `Move closer to the mic or raise its input volume in ${settings}.` };
  return { tone: "ok", title: "Sounds good", body: "Onda can hear you clearly." };
}

/** The system's name for the default microphone and where its input volume is set. */
export function micWording(platform: string): { system: string; defaultLabel: string; volumeHint: string } {
  switch (platform) {
    case "windows":
      return { system: "Windows", defaultLabel: "Windows default", volumeHint: "Adjust it in Windows: Settings › System › Sound › Input." };
    case "macos":
      return { system: "macOS", defaultLabel: "System default", volumeHint: "Adjust it in System Settings › Sound › Input." };
    default:
      return { system: "the system", defaultLabel: "System default", volumeHint: "Adjust it in your desktop's sound settings, under Input." };
  }
}

function soundSettings(platform: string): string {
  switch (platform) {
    case "windows":
      return "Windows sound settings";
    case "macos":
      return "System Settings › Sound";
    default:
      return "your sound settings";
  }
}
