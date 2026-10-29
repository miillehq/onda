import { signal, useComputed } from "@preact/signals";
import { Ban, Check, Copy, Keyboard, MicOff, Sparkles, TriangleAlert, X } from "lucide-preact";
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
/** The pill loads no snapshot; WebView2 reports Windows in its user agent, WebKitGTK Linux. */
const isWindows = { value: navigator.userAgent.includes("Windows") };
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

function ErrorCard({ error, operation }: { error: AppError; operation: OperationState }) {
  const presentation = presentError(error);
  const mic = MIC_CODES.has(error.code);
  const section = presentation.action?.route;
  const [copied, setCopied] = useState(false);
  const saved = result.value;
  // The AI version is the thing to copy only when its polish was what failed; after any
  // other failure the words to copy are the ones Onda meant to type.
  const suggestion = error.code === "invalidEnhancement" ? saved?.suggestion ?? null : null;
  // A failed rewrite leaves the selection in place, so copying it again would do nothing.
  const keptSelection = operation.mode !== "dictation" && saved?.output === saved?.original;
  const copyable = !!saved && !saved.inserted && saved.output.trim().length > 0 && !keptSelection;
  const dismiss = () => {
    const id = operation.operationId;
    void ask(id ? { command: "dismissError", args: { operationId: id } } : { command: "dismissPill" }, "unit").catch(() => undefined);
  };
  const copy = async () => {
    try {
      await ask(suggestion ? { command: "copySuggestion" } : { command: "copyResult" }, "unit");
      setCopied(true);
      window.setTimeout(dismiss, 900);
    } catch {
      setCopied(false);
    }
  };
  const canCopy = copyable || !!suggestion;
  // Dismiss lives in the corner, so the row holds at most a copy and one fix-it action and
  // never runs past the card's 340 px.
  return (
    <div class="pill-card" role="alert">
      <span class="pill-card-icon">{mic ? <MicOff size={18} /> : error.code === "shortPress" ? <Keyboard size={18} /> : <TriangleAlert size={18} />}</span>
      <div class="pill-card-text">
        <p class="pill-card-title">{presentation.title}</p>
        <p class="pill-card-body">{presentation.hint ?? error.message}</p>
        {(canCopy || section) && (
          <div class="pill-card-actions">
            {canCopy && (
              <button type="button" class="pill-button is-primary" onClick={() => void copy()} disabled={copied}>
                {copied ? <Check size={15} aria-hidden="true" /> : <Copy size={15} aria-hidden="true" />}
                {copied ? "Copied" : suggestion ? "Copy AI version" : "Copy"}
              </button>
            )}
            {section && (
              <button type="button" class={`pill-button${canCopy ? "" : " is-primary"}`} onClick={() => void ask({ command: "openMainWindow", args: { section } }, "unit").catch(() => undefined)}>
                {presentation.action?.label}
              </button>
            )}
          </div>
        )}
      </div>
      <button type="button" class="pill-card-close" aria-label="Dismiss" title="Dismiss" onClick={dismiss}>
        <X size={16} aria-hidden="true" />
      </button>
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
      content = <ErrorCard error={current.error} operation={current} />;
    } else if (phase === "recording") {
      const handsFree = current.handsFree;
      const command = current.mode === "spokenCommand";
      content = (
        <div class="pill-capsule pill-live">
          {handsFree && <span class="pill-rec" aria-hidden="true" />}
          {handsFree && <Timer />}
          {command && <span class="pill-label">Say your edit</span>}
          <Wave mode="live" level={level} glyph />
          {handsFree && settings.value && isWindows.value && (
            <>
              <span class="pill-divider" />
              <Keys keys={settings.value.toggleShortcut.keys} />
              <span class="pill-hint">to finish</span>
            </>
          )}
          {handsFree && !isWindows.value && (
            <>
              <span class="pill-divider" />
              <span class="pill-hint">Shortcut again to finish</span>
            </>
          )}
        </div>
      );
    } else if (phase === "enhancing" && current.route !== "local") {
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
      // Where Onda can't type into apps (Linux), a finished dictation waits on the clipboard.
      const copied = result.value?.operationId === current.operationId && result.value.inserted === false;
      content = (
        <div class="pill-capsule">
          <span class="pill-check">
            <Check size={15} strokeWidth={3} />
          </span>
          <span class="pill-label">{copied ? (fallback ? "Copied without polish · Ctrl+V" : "Copied · press Ctrl+V") : fallback ? "Inserted without polish" : "Inserted"}</span>
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
