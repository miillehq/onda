import { useSignal } from "@preact/signals";
import { ArrowRight, AudioLines, Copy, Keyboard, MessageSquare, Mic, Sparkles } from "lucide-preact";
import { useEffect, useRef, useState } from "preact/hooks";
import { Button, Chip, Keys, ProgressBar } from "../components/controls";
import { Callout } from "../components/layout";
import { Wave } from "../components/Wave";
import { presentError } from "../lib/errors";
import { formatBytes, greetingFor, pasteKeys } from "../lib/format";
import { navigate } from "../lib/router";
import { HistoryItem } from "./History";
import {
  acceptLearning,
  catalog,
  commandShortcutOnly,
  copySuggestion,
  dismissError,
  downloadModel,
  downloads,
  history,
  isWindows,
  lastResult,
  learning,
  modelReady,
  operation,
  platform,
  profile,
  provisional,
  settings,
  snapshot,
  thisDevice,
} from "../lib/store";
import { Stats } from "./home/ActivitySummary";
import { ResultCard } from "./home/LastResult";

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
      {!commandShortcutOnly.value ? (
        <div class="hero-copy">
          <p class="hero-eyebrow">Your dictation shortcut</p>
          <Keys keys={value.pushToTalk.keys} size="hero" />
          <h2 id="hero-title" class="hero-title">
            Hold to talk.
            <br />
            Let go to type.
          </h2>
          <p class="hero-text">
            {isWindows.value || snapshot.value?.typingAccess === "allowed"
              ? "Works in any app, wherever your cursor is."
              : snapshot.value?.typingAccess === "notAllowed"
                ? "Works in any app. Allow typing in Shortcuts so Onda can paste for you."
                : `Works in any app. Where Onda can't type, your words wait on the clipboard for ${pasteKeys(platform.value === "macos")}.`}
          </p>
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
            Press again to finish.
          </h2>
          <p class="hero-text">Bind the command to a key in your desktop's settings. Where Onda can't type, your words wait on the clipboard for Ctrl+V.</p>
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
        `Speech recognition runs on ${thisDevice.value}, so Onda needs its model once.`
      )}
    </Callout>
  );
}

function OperationError() {
  const state = operation.value;
  const dismissed = useSignal<string | null>(null);
  if (!state || state.phase !== "error" || !state.error || state.error.code === "noModel" || dismissed.value === state.operationId) return null;
  const presentation = presentError(state.error);
  const saved = lastResult.value?.operationId === state.operationId ? lastResult.value : null;
  // Offer the polished version only when its polish is what failed.
  const suggestion = state.error.code === "invalidEnhancement" ? saved?.suggestion ?? null : null;
  // History holds nothing for an operation that failed before it had any text.
  const action = presentation.action?.route === "history" && !saved ? undefined : presentation.action;
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
              Copy polished version
            </Button>
          ) : (
            action && (
              <Button variant="primary" size="sm" onClick={() => navigate(action.route)}>
                {action.label}
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

