import { useComputed, useSignal } from "@preact/signals";
import {
  ArrowRight,
  AudioLines,
  BookOpen,
  ClipboardPaste,
  Copy,
  Cpu,
  Gauge,
  History as HistoryIcon,
  Keyboard,
  MessageSquare,
  Mic,
  RotateCcw,
  Sparkles,
  Trash2,
  Undo2,
} from "lucide-preact";
import { useEffect, useRef, useState } from "preact/hooks";
import { Button, Chip, Keys, ProgressBar } from "../components/controls";
import { Diff, Suggestion } from "../components/feedback";
import { Callout, EmptyState } from "../components/layout";
import { Wave } from "../components/Wave";
import { presentError } from "../lib/errors";
import { formatBytes, formatDuration, formatInteger, greetingFor, routeLabel } from "../lib/format";
import { navigate } from "../lib/router";
import { HistoryItem, retentionPhrase } from "./History";
import {
  acceptLearning,
  catalog,
  copyResult,
  copySuggestion,
  dictionary,
  discardResult,
  dismissError,
  downloadModel,
  downloads,
  history,
  isBusy,
  lastResult,
  learning,
  modelReady,
  operation,
  pasteAgain,
  provisional,
  retryPolish,
  settings,
  snapshot,
  undoRewrite,
  isWindows,
  profile,
  usage,
} from "../lib/store";

const TYPING_WPM = 40;
const SAMPLE = "Running ten minutes late, start without me";

/* ------------------------------------------------------------------ hero */

function HeroScene() {
  const reduced = typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const [typed, setTyped] = useState(reduced ? SAMPLE.length : 0);
  const level = useSignal(0);
  const done = typed >= SAMPLE.length;
  useEffect(() => {
    if (reduced) return;
    let at = 0;
    const timer = window.setInterval(() => {
      at += 1;
      setTyped(at);
      level.value = 0.35 + Math.random() * 0.55;
      if (at >= SAMPLE.length) {
        level.value = 0;
        window.clearInterval(timer);
      }
    }, 46);
    return () => window.clearInterval(timer);
  }, []);
  return (
    <div class="hero-scene" aria-hidden="true">
      <div class="scene-message">
        <div class="scene-head">
          <MessageSquare size={15} />
          <b>Message</b>
          <span>to Design team</span>
        </div>
        <div class="scene-text">
          {SAMPLE.slice(0, typed)}
          <span class={`caret${done ? " is-done" : ""}`} />
        </div>
      </div>
      <span class="pill-capsule scene-pill">
        <Wave mode={done ? "still" : "live"} level={level} glyph idle={0.15} width={80} height={30} />
      </span>
    </div>
  );
}

function Hero({ onPractice }: { onPractice: () => void }) {
  const value = settings.value;
  if (!value) return null;
  return (
    <section class="hero" aria-labelledby="hero-title">
      {isWindows.value ? (
        <div class="hero-copy">
          <p class="hero-eyebrow">Your dictation shortcut</p>
          <Keys keys={value.pushToTalk.keys} size="hero" />
          <h2 id="hero-title" class="hero-title">
            Hold to talk.
            <br />
            Let go to type.
          </h2>
          <p class="hero-text">Works in any app, wherever your cursor is.</p>
          <div class="hero-actions">
            <Button variant="inverse" icon={<Mic size={16} />} onClick={onPractice}>
              Try it here
            </Button>
            <Button variant="inverse-ghost" icon={<Keyboard size={16} />} onClick={() => navigate("settings/shortcuts")}>
              Change shortcut
            </Button>
          </div>
        </div>
      ) : (
        <div class="hero-copy">
          <p class="hero-eyebrow">Your dictation shortcut</p>
          <p class="hero-command mono">onda --dictate</p>
          <h2 id="hero-title" class="hero-title">
            Press to talk.
            <br />
            Press again, then paste.
          </h2>
          <p class="hero-text">Bind the command to a key in your desktop's settings. Your words wait on the clipboard for Ctrl+V.</p>
          <div class="hero-actions">
            <Button variant="inverse" icon={<Keyboard size={16} />} onClick={() => navigate("settings/shortcuts")}>
              Set up the shortcut
            </Button>
          </div>
        </div>
      )}
      <HeroScene />
    </section>
  );
}

/** A real text field in Onda's own window, so the first dictation needs no other app. */
function Practice({ onClose }: { onClose: () => void }) {
  const field = useRef<HTMLTextAreaElement>(null);
  const value = settings.value;
  useEffect(() => field.current?.focus(), []);
  if (!value) return null;
  return (
    <section class="card practice" aria-labelledby="practice-title">
      <div class="practice-head">
        <div>
          <h3 id="practice-title">Practice here</h3>
          <p class="muted">
            Keep this box focused, hold <Keys keys={value.pushToTalk.keys} /> and say something. Let go when you're done.
          </p>
        </div>
        <Button variant="ghost" size="sm" onClick={onClose}>
          Done
        </Button>
      </div>
      <textarea ref={field} class="textarea practice-field serif" rows={3} placeholder="Your words will appear here" aria-label="Practice text" />
    </section>
  );
}

/* ------------------------------------------------------------------ status */

function SetupCallout() {
  const value = settings.value;
  const id = snapshot.value?.selectedModelId ?? null;
  const model = catalog.value.find((item) => item.id === id) ?? null;
  if (!value || modelReady.value || !id) return null;
  const progress = downloads.value[id];
  return (
    <Callout
      tone="warn"
      icon={<AudioLines size={20} />}
      title={progress ? `Downloading ${model?.name ?? "speech model"}` : "Download a speech model to start dictating"}
      action={
        progress ? undefined : (
          <Button variant="primary" onClick={() => void downloadModel(id)}>
            Download {model ? formatBytes(model.downloadSizeBytes) : ""}
          </Button>
        )
      }
    >
      {progress ? (
        <ProgressBar value={progress.stage === "verifying" ? null : progress.completedBytes / Math.max(1, progress.totalBytes)} label="Model download" />
      ) : operation.value?.error?.code === "noModel" ? (
        <span data-testid="operation-error">{operation.value.error.message}</span>
      ) : (
        "Speech recognition runs on this PC, so Onda needs its model once."
      )}
    </Callout>
  );
}

function OperationError() {
  const state = operation.value;
  const dismissed = useSignal<string | null>(null);
  if (!state || state.phase !== "error" || !state.error || state.error.code === "noModel" || dismissed.value === state.operationId) return null;
  const presentation = presentError(state.error);
  // Offer ChatGPT's version only when its polish is what failed.
  const suggestion = state.error.code === "invalidEnhancement" && lastResult.value?.operationId === state.operationId ? lastResult.value.suggestion : null;
  const dismiss = () => {
    dismissed.value = state.operationId;
    if (state.operationId) void dismissError(state.operationId);
  };
  return (
    <Callout
      tone="error"
      title={presentation.title}
      action={
        <div class="button-row">
          {suggestion ? (
            <Button variant="primary" size="sm" icon={<Copy size={15} />} onClick={() => void copySuggestion()}>
              Copy ChatGPT's version
            </Button>
          ) : (
            presentation.action && (
              <Button variant="primary" size="sm" onClick={() => navigate(presentation.action!.route)}>
                {presentation.action.label}
              </Button>
            )
          )}
          <Button variant="ghost" size="sm" onClick={dismiss}>
            Dismiss
          </Button>
        </div>
      }
    >
      <span data-testid="operation-error">{state.error.message}</span>
      {presentation.hint && <span class="callout-hint"> {presentation.hint}</span>}
    </Callout>
  );
}

/* ------------------------------------------------------------------ stats */

function dayKey(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

function useWeek() {
  return useComputed(() => {
    const report = usage.value;
    const byDay = new Map(report?.days.map((day) => [day.day, day]) ?? []);
    const [year, month, date] = (report?.today ?? dayKey(new Date())).split("-").map(Number);
    const today = new Date(year ?? 1970, (month ?? 1) - 1, date ?? 1);
    const monday = new Date(today);
    monday.setDate(today.getDate() - ((today.getDay() + 6) % 7));
    const week = Array.from({ length: 7 }, (_, index) => {
      const day = new Date(monday);
      day.setDate(monday.getDate() + index);
      const key = dayKey(day);
      return { key, label: day.toLocaleDateString(undefined, { weekday: "narrow" }), name: day.toLocaleDateString(undefined, { weekday: "long" }), words: byDay.get(key)?.words ?? 0, today: key === dayKey(today), future: day > today };
    });
    let rollingWords = 0;
    let rollingAudio = 0;
    for (let offset = 0; offset < 7; offset += 1) {
      const day = new Date(today);
      day.setDate(today.getDate() - offset);
      const entry = byDay.get(dayKey(day));
      rollingWords += entry?.words ?? 0;
      rollingAudio += entry?.audioMs ?? 0;
    }
    // Under a minute of speech the rate is mostly pauses, so wait for a fairer sample.
    const wpm = rollingAudio >= 60_000 && rollingWords > 0 ? rollingWords / (rollingAudio / 60_000) : null;
    const savedMs = wpm ? Math.max(0, (rollingWords / TYPING_WPM) * 60_000 - rollingAudio) : 0;
    return { week, weekWords: week.reduce((sum, day) => sum + day.words, 0), wpm, savedMs, loaded: report !== null };
  });
}

function Stats() {
  const stats = useWeek().value;
  const max = Math.max(1, ...stats.week.map((day) => day.words));
  const seen = new Set<string>();
  const learned = [...dictionary.value]
    .filter((entry) => entry.enabled)
    .sort((a, b) => Number(b.source === "autoLearned") - Number(a.source === "autoLearned") || Number(b.starred) - Number(a.starred))
    .filter((entry) => {
      const label = (entry.replacement ?? entry.phrase).toLocaleLowerCase();
      if (seen.has(label)) return false;
      seen.add(label);
      return true;
    })
    .slice(0, 5);
  return (
    <div class="stats">
      <section class="tile" aria-labelledby="stat-week">
        <h3 class="tile-label" id="stat-week">
          <AudioLines size={16} aria-hidden="true" />
          Words this week
        </h3>
        <p class="tile-value tabular">{formatInteger(stats.weekWords)}</p>
        <div class="week-chart" role="img" aria-label={stats.week.map((day) => `${day.name}: ${day.words} words`).join(", ")}>
          {stats.week.map((day) => (
            <div class={`week-day${day.today ? " is-today" : ""}${day.future ? " is-future" : ""}`} key={day.key}>
              <i style={{ height: `${Math.max(6, (day.words / max) * 100)}%` }} />
              <span>{day.label}</span>
            </div>
          ))}
        </div>
      </section>
      <section class="tile" aria-labelledby="stat-speed">
        <h3 class="tile-label" id="stat-speed">
          <Gauge size={16} aria-hidden="true" />
          Your speaking speed
        </h3>
        {stats.wpm ? (
          <>
            <p class="tile-value tabular">
              {Math.round(stats.wpm)}
              <small>wpm</small>
            </p>
            <div class="versus" role="img" aria-label={`${Math.round(stats.wpm)} words per minute speaking, compared with about ${TYPING_WPM} typing`}>
              <div class="versus-row">
                <span>Speaking</span>
                <i style={{ width: `${(stats.wpm / Math.max(stats.wpm, TYPING_WPM)) * 100}%` }} />
              </div>
              <div class="versus-row is-muted">
                <span>Typing</span>
                <i style={{ width: `${(TYPING_WPM / Math.max(stats.wpm, TYPING_WPM)) * 100}%` }} />
              </div>
            </div>
            <p class="tile-foot">
              {stats.wpm >= TYPING_WPM * 1.1 ? (
                <>
                  <b>{(stats.wpm / TYPING_WPM).toFixed(1)}× faster</b> than typing{stats.savedMs >= 60_000 ? ` · ${formatDuration(stats.savedMs)} saved` : ""}
                </>
              ) : (
                <>Typical typing is about {TYPING_WPM} wpm. Speaking naturally, without long pauses, gets you past it.</>
              )}
            </p>
          </>
        ) : (
          <>
            <p class="tile-value is-empty">—</p>
            <p class="tile-foot">After a minute or so of dictation, see how your speed compares with typing.</p>
          </>
        )}
      </section>
      <section class="tile" aria-labelledby="stat-dictionary">
        <h3 class="tile-label" id="stat-dictionary">
          <BookOpen size={16} aria-hidden="true" />
          Your dictionary
        </h3>
        <p class="tile-value tabular">
          {formatInteger(dictionary.value.length)}
          <small>{dictionary.value.length === 1 ? "word" : "words"}</small>
        </p>
        {learned.length ? (
          <div class="chip-cloud">
            {learned.map((entry) => (
              <Chip key={entry.id} tone={entry.source === "autoLearned" ? "strong" : "neutral"} title={entry.source === "autoLearned" ? "Learned from your corrections" : undefined}>
                {entry.replacement ?? entry.phrase}
              </Chip>
            ))}
          </div>
        ) : (
          <button type="button" class="link-button" onClick={() => navigate("dictionary")}>
            Add names and jargon <ArrowRight size={14} />
          </button>
        )}
      </section>
    </div>
  );
}

/* ------------------------------------------------------------------ result */

const CLOUD_CODES = new Set(["unauthorized", "quota", "cloud", "offline", "invalidEnhancement"]);

function ResultCard() {
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
          Earlier ones stay in History {retentionPhrase(days)}, encrypted on this PC, so you can always copy them again.
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
          {result && !result.inserted && state?.phase === "completed" && <Chip tone="ok">On the clipboard · Ctrl+V</Chip>}
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
function Recent() {
  const currentId = lastResult.value?.operationId ?? provisional.value?.operationId;
  const recent = (history.value ?? []).filter((entry) => entry.id !== currentId).slice(0, 4);
  if (!recent.length) return null;
  return (
    <section class="card recent" aria-labelledby="recent-title">
      <header class="recent-head">
        <h3 id="recent-title">Earlier</h3>
        <button type="button" class="link-button" onClick={() => navigate("history")}>
          All history <ArrowRight size={14} />
        </button>
      </header>
      <ol class="history-list">
        {recent.map((entry) => (
          <HistoryItem key={entry.id} entry={entry} compact />
        ))}
      </ol>
    </section>
  );
}

function LearningCard() {
  const proposal = learning.value;
  if (!proposal) return null;
  return (
    <section class="card learn" aria-labelledby="learn-title">
      <span class="learn-icon" aria-hidden="true">
        <Sparkles size={18} />
      </span>
      <div class="learn-body">
        <h3 id="learn-title">Add “{proposal.corrected}” to your dictionary?</h3>
        <p class="fix">
          <span>You changed</span>
          <del>{proposal.original}</del>
          <ArrowRight size={15} aria-hidden="true" />
          <Chip tone="strong">{proposal.corrected}</Chip>
          <span>right after dictating.</span>
        </p>
      </div>
      <div class="button-row">
        <Button variant="ghost" onClick={() => (learning.value = null)}>
          Not now
        </Button>
        <Button variant="primary" onClick={() => void acceptLearning(proposal)}>
          Add word
        </Button>
      </div>
    </section>
  );
}

export function Home() {
  const [practicing, setPracticing] = useState(false);
  return (
    <div class="page home">
      <header class="page-head">
        <div class="page-heading">
          <h1 tabIndex={-1}>{greetingFor(profile.value?.name)}</h1>
        </div>
      </header>
      <div class="stack">
        <SetupCallout />
        <OperationError />
        <Hero onPractice={() => setPracticing(true)} />
        {practicing && <Practice onClose={() => setPracticing(false)} />}
        <LearningCard />
        <Stats />
        <ResultCard />
        <Recent />
      </div>
    </div>
  );
}

