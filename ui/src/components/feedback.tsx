import { useComputed, type ReadonlySignal } from "@preact/signals";
import { CircleAlert, CircleCheck, Info, X } from "lucide-preact";
import type { DiffSegment } from "../generated/core";
import { dismissToast, toasts } from "../lib/store";

export function Toasts() {
  return (
    <div class="toasts" aria-live="polite" aria-relevant="additions">
      {toasts.value.map((item) => (
        <div class={`toast toast-${item.tone}`} key={item.id} role={item.tone === "error" ? "alert" : "status"}>
          <span class="toast-icon">{item.tone === "ok" ? <CircleCheck size={18} /> : item.tone === "error" ? <CircleAlert size={18} /> : <Info size={18} />}</span>
          <div class="toast-text">
            <div class="toast-title">{item.title}</div>
            {item.body && <div class="toast-body">{item.body}</div>}
          </div>
          {item.action && (
            <button
              type="button"
              class="toast-action"
              onClick={() => {
                item.action?.run();
                dismissToast(item.id);
              }}
            >
              {item.action.label}
            </button>
          )}
          <button type="button" class="icon-button toast-close" aria-label="Dismiss" onClick={() => dismissToast(item.id)}>
            <X size={16} />
          </button>
        </div>
      ))}
    </div>
  );
}

/** Vertical bars that rise with the input level; a quiet, readable meter for mic checks. */
export function LevelBars({ level, bars = 24, active }: { level: ReadonlySignal<number>; bars?: number; active: boolean }) {
  const lit = useComputed(() => Math.round(level.value * bars));
  return (
    <div class={`level-bars${active ? " is-active" : ""}`} aria-hidden="true">
      {Array.from({ length: bars }, (_, index) => {
        const center = 1 - Math.abs(index - (bars - 1) / 2) / (bars / 2);
        const height = 22 + center * 78;
        return <i key={index} class={index < lit.value ? "on" : undefined} style={{ height: `${height}%` }} />;
      })}
    </div>
  );
}

export function Diff({ segments }: { segments: DiffSegment[] }) {
  return (
    <p class="diff serif selectable">
      {segments.map((segment, index) =>
        segment.kind === "equal" ? (
          <span key={index}>{segment.text}</span>
        ) : segment.kind === "added" ? (
          <ins key={index}>{segment.text}</ins>
        ) : (
          <del key={index}>{segment.text}</del>
        ),
      )}
    </p>
  );
}
