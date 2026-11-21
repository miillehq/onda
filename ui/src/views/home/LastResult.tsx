import { ClipboardPaste, Copy, Cpu, History as HistoryIcon, RotateCcw, Sparkles, Trash2, Undo2 } from "lucide-preact";
import { useEffect, useState } from "preact/hooks";
import { Button, Chip } from "../../components/controls";
import { Diff, Suggestion } from "../../components/feedback";
import { EmptyState } from "../../components/layout";
import { pasteKeys, routeLabel } from "../../lib/format";
import { navigate } from "../../lib/router";
import { retentionPhrase } from "../History";
import {
  copyResult,
  copySuggestion,
  discardResult,
  isBusy,
  isWindows,
  lastResult,
  operation,
  pasteAgain,
  platform,
  provisional,
  retryPolish,
  settings,
  thisDevice,
  undoRewrite,
} from "../../lib/store";

const CLOUD_CODES = new Set(["unauthorized", "quota", "cloud", "offline", "invalidEnhancement"]);

export function ResultCard() {
  const result = lastResult.value;
  const state = operation.value;
  const streaming = provisional.value;
  const [showChanges, setShowChanges] = useState(false);
  const [countdown, setCountdown] = useState<number | null>(null);
  useEffect(() => setShowChanges(false), [result?.operationId]);
  useEffect(() => {
    if (countdown === null) return;
    if (countdown <= 0) {
      setCountdown(null);
      return;
    }
    const timer = window.setTimeout(() => setCountdown(countdown - 1), 1000);
    return () => window.clearTimeout(timer);
  }, [countdown]);

  const days = settings.value?.historyDays ?? 0;
  if (!result && !streaming) {
    return (
      <section class="card result is-empty">
        <EmptyState art={<HistoryIcon size={20} />} title="Your last dictation shows up here">
          Earlier ones stay in History {retentionPhrase(days)}, encrypted on {thisDevice.value}, so you can always copy them again.
        </EmptyState>
      </section>
    );
  }
  const rewrite = state?.mode === "transform" || state?.mode === "spokenCommand";
  const changed = !!result && result.output !== result.original;
  const cloudFailure = state?.phase === "error" && !!state.error && CLOUD_CODES.has(state.error.code) && !!result && !result.inserted;
  const canUndo = !!result && result.inserted && rewrite && state?.phase !== "error";
  const skipped = !!result && (result.fallbackUsed || !!result.suggestion);
  // A skipped rewrite left the selection as it was, so pasting it again changes nothing.
  const keptSelection = rewrite && !!result && !result.inserted && !changed;
  return (
    <section class="card result" aria-labelledby="result-title">
      <header class="result-head">
        <h3 id="result-title">{rewrite ? "Last rewrite" : "Last dictation"}</h3>
        <div class="chip-row">
          {skipped ? (
            <Chip tone="warn">Polish skipped</Chip>
          ) : state && routeLabel(state.route) ? (
            <Chip icon={<Sparkles size={14} />}>{routeLabel(state.route)}</Chip>
          ) : (
            <Chip icon={<Cpu size={14} />}>On this {isWindows.value ? "PC" : "computer"}</Chip>
          )}
          {result && !result.inserted && state?.phase === "error" && state.error?.code !== "invalidEnhancement" && <Chip tone="err">Not inserted</Chip>}
          {result && !result.inserted && state?.phase === "completed" && <Chip tone="ok">On the clipboard · {pasteKeys(platform.value === "macos")}</Chip>}
        </div>
      </header>
      {result?.commandText && <p class="result-command">You asked: “{result.commandText}”</p>}
      {streaming && !result?.inserted ? (
        <p class="transcript serif is-provisional" aria-live="off">
          {streaming.text}
          <span class="caret" />
        </p>
      ) : showChanges && result ? (
        <Diff segments={result.diff} />
      ) : (
        <p class="transcript serif selectable">{result?.output}</p>
      )}
      {result?.suggestion && <Suggestion kept={result.output} suggestion={result.suggestion} onCopy={() => void copySuggestion()} />}
      {countdown !== null && (
        <p class="result-countdown" role="status">
          Switch to your app now. Pasting in {countdown}…
        </p>
      )}
      <footer class="result-foot">
        <div class="button-row">
          <Button icon={<Copy size={16} />} onClick={() => void copyResult()} disabled={!result}>
            Copy
          </Button>
          <Button
            icon={<ClipboardPaste size={16} />}
            disabled={!result || isBusy.value || keptSelection}
            onClick={async () => {
              if (await pasteAgain()) setCountdown(3);
            }}
          >
            Paste again
          </Button>
          {canUndo && (
            <Button icon={<Undo2 size={16} />} onClick={async () => (await undoRewrite()) && setCountdown(3)}>
              Undo
            </Button>
          )}
          {cloudFailure && (
            <Button icon={<RotateCcw size={16} />} onClick={() => void retryPolish()}>
              Try polish again
            </Button>
          )}
          {changed && (
            <Button variant="ghost" onClick={() => setShowChanges(!showChanges)} aria-pressed={showChanges}>
              {showChanges ? "Hide changes" : "Show changes"}
            </Button>
          )}
        </div>
        <div class="result-meta">
          <button type="button" class="link-button muted-link" onClick={() => navigate("history")}>
            <HistoryIcon size={14} aria-hidden="true" />
            In History {retentionPhrase(days)}
          </button>
          <Button variant="ghost" size="sm" icon={<Trash2 size={15} />} onClick={() => void discardResult()} aria-label="Delete last dictation">
            Delete
          </Button>
        </div>
      </footer>
    </section>
  );
}

/** The dictations before the current one, so a new dictation never buries an old one. */
