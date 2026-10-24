import { signal, useComputed } from "@preact/signals";
import { Ban, Check, Copy, MicOff, Sparkles, TriangleAlert } from "lucide-preact";
import { useEffect, useState } from "preact/hooks";
import { ask, onEvent } from "../api";
import { Keys } from "../components/controls";
import { Wave } from "../components/Wave";
import type { AppError, OperationResult, OperationState, Settings } from "../generated/core";
import { presentError } from "../lib/errors";
import { formatClock } from "../lib/format";
import { levelFromRms } from "../lib/level";

const state = signal<OperationState | null>(null);
const result = signal<OperationResult | null>(null);
const level = signal(0);
const settings = signal<Settings | null>(null);
const startedAt = signal(0);

/** Exit fades start shortly before the native window hides (see src-tauri/src/pill.rs). */
const LEAVE_AFTER: Partial<Record<OperationState["phase"], number>> = { completed: 1150, error: 6650, cancelled: 120 };
const MIC_CODES = new Set<AppError["code"]>(["noSpeech", "noMicrophone", "audioDisconnected", "audioRouteChanged", "audioPermissionDenied", "audioBackend"]);

export async function startPill(): Promise<void> {
  await onEvent((event) => {
    if (event.type === "audioLevel") {
      level.value = levelFromRms(event.payload.level);
      return;
    }
    if (event.type !== "domain") return;
    const domain = event.payload;
    if (domain.type === "stateChanged") {
      const previous = state.value;
      if (domain.payload.operationId !== previous?.operationId) {
        result.value = null;
        startedAt.value = Date.now();
        void ask({ command: "getSettings" }, "settings").then((value) => (settings.value = value)).catch(() => undefined);
      }
      state.value = domain.payload;
    } else if (domain.type === "resultReady" && domain.payload.operationId === state.value?.operationId) {
      result.value = domain.payload;
    }
  });
  settings.value = await ask({ command: "getSettings" }, "settings").catch(() => null);
}

function Timer() {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 250);
    return () => window.clearInterval(timer);
  }, []);
  return <span class="pill-time tabular">{formatClock(now - startedAt.value)}</span>;
}

function ErrorCard({ error }: { error: AppError }) {
  const presentation = presentError(error);
  const mic = MIC_CODES.has(error.code);
  const section = presentation.action?.route;
  const [copied, setCopied] = useState(false);
  const saved = result.value;
  const copyable = !!saved && !saved.inserted && saved.output.trim().length > 0;
  const copy = async () => {
    try {
      await ask({ command: "copyResult" }, "unit");
      setCopied(true);
      window.setTimeout(() => void ask({ command: "dismissPill" }, "unit").catch(() => undefined), 900);
    } catch {
      setCopied(false);
    }
  };
  return (
    <div class="pill-card" role="alert">
      <span class="pill-card-icon">{mic ? <MicOff size={18} /> : <TriangleAlert size={18} />}</span>
      <div class="pill-card-text">
        <p class="pill-card-title">{presentation.title}</p>
        <p class="pill-card-body">{presentation.hint ?? error.message}</p>
        <div class="pill-card-actions">
          {copyable && (
            <button type="button" class="pill-button is-primary" onClick={() => void copy()} disabled={copied}>
              {copied ? <Check size={15} aria-hidden="true" /> : <Copy size={15} aria-hidden="true" />}
              {copied ? "Copied" : "Copy"}
            </button>
          )}
          {section && (
            <button type="button" class={`pill-button${copyable ? "" : " is-primary"}`} onClick={() => void ask({ command: "openMainWindow", args: { section } }, "unit").catch(() => undefined)}>
              {presentation.action?.label}
            </button>
          )}
          <button type="button" class="pill-button" onClick={() => void ask({ command: "dismissPill" }, "unit").catch(() => undefined)}>
            Dismiss
          </button>
        </div>
      </div>
    </div>
  );
}

const PHASE_SPEECH: Partial<Record<OperationState["phase"], string>> = {
  recording: "Listening",
  transcribing: "Writing it out",
  enhancing: "Polishing",
  completed: "Inserted",
  cancelled: "Cancelled",
};

export function Pill() {
  const current = state.value;
  const [leaving, setLeaving] = useState(false);
  const phase = current?.phase ?? "idle";
  const key = `${current?.operationId ?? "none"}:${phase}`;

  useEffect(() => {
    setLeaving(false);
    const delay = LEAVE_AFTER[phase];
    if (delay === undefined) return;
    const timer = window.setTimeout(() => setLeaving(true), delay);
    return () => window.clearTimeout(timer);
  }, [key]);

  const announcement = useComputed(() => {
    const value = state.value;
    if (!value) return "";
    return value.phase === "error" ? value.error?.message ?? "" : PHASE_SPEECH[value.phase] ?? "";
  });

  let content = null;
  if (current && phase !== "idle") {
    if (phase === "error" && current.error) {
      content = <ErrorCard error={current.error} />;
    } else if (phase === "recording") {
      const handsFree = current.handsFree;
      const command = current.mode === "spokenCommand";
      content = (
        <div class="pill-capsule pill-live">
          {handsFree && <span class="pill-rec" aria-hidden="true" />}
          {handsFree && <Timer />}
          {command && <span class="pill-label">Say your edit</span>}
          <Wave mode="live" level={level} glyph />
          {handsFree && settings.value && (
            <>
              <span class="pill-divider" />
              <Keys keys={settings.value.toggleShortcut.keys} />
              <span class="pill-hint">to finish</span>
            </>
          )}
        </div>
      );
    } else if (phase === "enhancing" && current.route === "chatgptPlan") {
      content = (
        <div class="pill-capsule">
          <Wave mode="settle" width={62} height={23} glyph />
          <span class="pill-divider" />
          <Sparkles size={16} />
          <span class="pill-label">{current.mode === "transform" ? "Rewriting" : current.mode === "spokenCommand" ? "Editing" : "Polishing"}</span>
        </div>
      );
    } else if (phase === "completed") {
      const fallback = result.value?.fallbackUsed ?? current.fallbackUsed;
      content = (
        <div class="pill-capsule">
          <span class="pill-check">
            <Check size={15} strokeWidth={3} />
          </span>
          <span class="pill-label">{fallback ? "Inserted without polish" : "Inserted"}</span>
        </div>
      );
    } else if (phase === "cancelled") {
      content = (
        <div class="pill-capsule">
          <Ban size={16} />
          <span class="pill-label">Cancelled</span>
        </div>
      );
    } else {
      content = (
        <div class="pill-capsule">
          <Wave mode="settle" glyph />
        </div>
      );
    }
  }

  return (
    <div class="pill-stage">
      {content && (
        <div class={`pill-motion${leaving ? " is-leaving" : ""}`} key={current?.operationId ?? "none"}>
          {content}
        </div>
      )}
      <div class="visually-hidden" role="status" aria-live="polite">
        {announcement.value}
      </div>
    </div>
  );
}
