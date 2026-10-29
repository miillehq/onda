import { ArrowLeft, ArrowRight, Check, CircleCheck, Cpu, Download, Globe, Lock, Mic, Sparkles, WandSparkles } from "lucide-preact";
import type { ComponentChildren } from "preact";
import { useEffect, useRef, useState } from "preact/hooks";
import iconUrl from "../assets/onda-icon-dark.svg";
import { ask } from "../api";
import { Lockup } from "../components/Brand";
import { Button, Keys, ProgressBar, Segmented, Select, Switch } from "../components/controls";
import { LevelBars } from "../components/feedback";
import { Wave } from "../components/Wave";
import { formatBytes } from "../lib/format";
import { navigate } from "../lib/router";
import {
  chatgptReady,
  catalog,
  cloudModels,
  devices,
  downloadModel,
  downloads,
  installedIds,
  lastResult,
  loadDevices,
  micLevel,
  oauthStatus,
  operation,
  refresh,
  saveSettings,
  settings,
  snapshot,
  toastError,
} from "../lib/store";
import { ChatgptModelSelect, modelAdvice } from "./settings/AiPolish";
import { MicTest } from "./settings/Microphone";

type Step = "welcome" | "microphone" | "model" | "try" | "polish" | "done";
const STEPS: Step[] = ["welcome", "microphone", "model", "try", "polish", "done"];

function Art({ step }: { step: Step }) {
  switch (step) {
    case "welcome":
      return <img src={iconUrl} alt="" width={168} height={168} class="art-icon" />;
    case "microphone":
      return (
        <div class="art-meter">
          <LevelBars level={micLevel} bars={18} active />
        </div>
      );
    case "model":
      return (
        <div class="art-chip">
          <Cpu size={56} strokeWidth={1.4} />
          <span>Runs on this PC</span>
        </div>
      );
    case "try": {
      const keys = settings.value?.pushToTalk.keys ?? [0xa2, 0x5b, 0x20];
      return (
        // A looping demonstration of holding, since a tap of the shortcut records nothing.
        <div class="art-try">
          <Keys keys={keys} size="hero" />
          <span class="hold-meter">
            <i />
          </span>
          <span class="hold-caption">
            <span class="hold-caption-hold">Hold while you speak</span>
            <span class="hold-caption-release">Let go to type</span>
          </span>
          <span class="pill-capsule">
            <Wave mode="live" level={micLevel} glyph width={84} height={30} />
          </span>
        </div>
      );
    }
    case "polish":
      return (
        <div class="art-chip">
          <Sparkles size={56} strokeWidth={1.4} />
          <span>Optional</span>
        </div>
      );
    case "done":
      return (
        <div class="art-chip">
          <CircleCheck size={64} strokeWidth={1.4} />
        </div>
      );
  }
}

function StepBody({ title, lede, children }: { title: string; lede: ComponentChildren; children?: ComponentChildren }) {
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => heading.current?.focus({ preventScroll: true }), [title]);
  return (
    <div class="setup-body">
      <h1 ref={heading} tabIndex={-1}>
        {title}
      </h1>
      <p class="setup-lede">{lede}</p>
      {children}
    </div>
  );
}

function Welcome() {
  return (
    <StepBody title="Welcome to Onda" lede="Speak, and your words appear wherever your cursor is. Setup takes about a minute.">
      <ul class="promises">
        <li>
          <span class="promise-icon">
            <Lock size={18} />
          </span>
          <span>
            <b>Private by default.</b> Speech recognition runs on this PC. Audio is never uploaded.
          </span>
        </li>
        <li>
          <span class="promise-icon">
            <Globe size={18} />
          </span>
          <span>
            <b>Works everywhere.</b> Email, chat, documents, code: wherever you can type.
          </span>
        </li>
        <li>
          <span class="promise-icon">
            <WandSparkles size={18} />
          </span>
          <span>
            <b>Gets to know you.</b> Teach it names and jargon, and save phrases you use often.
          </span>
        </li>
      </ul>
    </StepBody>
  );
}

function MicrophoneStep() {
  const value = settings.value;
  useEffect(() => {
    if (devices.value === null) void loadDevices().catch(() => undefined);
  }, []);
  if (!value) return null;
  return (
    <StepBody title="Check your microphone" lede="Pick the mic you'll speak into, then say a few words.">
      <div class="setup-field">
        <span class="field-label">Microphone</span>
        <Select
          label="Microphone"
          wide
          value={value.microphoneId ?? ""}
          onChange={(id) => void saveSettings({ microphoneId: id || null })}
          options={[{ value: "", label: "Windows default" }, ...(devices.value ?? []).map((device) => ({ value: device.id, label: device.name }))]}
        />
      </div>
      <div class="card">
        <MicTest compact />
      </div>
    </StepBody>
  );
}

function ModelStep() {
  const value = settings.value;
  if (!value) return null;
  const english = value.language === "en";
  // Keep a model the person already has; otherwise offer the small Fast model.
  const resolved = snapshot.value?.selectedModelId ?? null;
  const resolvedReady = !!resolved && installedIds.value.has(resolved);
  const id = resolvedReady && resolved ? resolved : english ? "whisper-tiny-en" : "whisper-tiny";
  const model = catalog.value.find((item) => item.id === id);
  const installed = installedIds.value.has(id);
  const progress = downloads.value[id];
  const keepCustom = resolvedReady && value.qualityProfile === "custom";
  const description = resolvedReady ? "Already on this PC." : english ? "Quick and accurate for English." : "Detects the language automatically.";
  return (
    <StepBody title="Get the speech model" lede="Onda recognises speech on your PC, so it needs a model once. You can switch to a more accurate one later.">
      <div class="setup-field">
        <span class="field-label">What will you speak?</span>
        <Segmented
          label="Languages"
          value={english ? "en" : "many"}
          onChange={(choice) => {
            const language = choice === "en" ? "en" : null;
            void saveSettings(keepCustom ? { language } : { language, qualityProfile: "fast" });
          }}
          options={[
            { value: "en", label: "English only" },
            { value: "many", label: "Other or several languages" },
          ]}
        />
      </div>
      <div class={`card download-card${installed ? " is-done" : ""}`}>
        <div class="download-card-head">
          <span class="flow-icon is-tile">
            <Cpu size={20} />
          </span>
          <div>
            <h3>{model?.name ?? "Fast model"}</h3>
            <p class="muted">{description}</p>
          </div>
          {installed ? (
            <span class="in-use">
              <Check size={16} aria-hidden="true" /> Ready
            </span>
          ) : (
            !progress && (
              <Button variant="primary" icon={<Download size={16} />} onClick={() => void downloadModel(id, { qualityProfile: "fast" })}>
                Download {model ? formatBytes(model.downloadSizeBytes) : ""}
              </Button>
            )
          )}
        </div>
        {progress && !installed && (
          <div class="download-card-progress">
            <ProgressBar value={progress.stage === "verifying" ? null : progress.completedBytes / Math.max(1, progress.totalBytes)} label="Model download" />
            <span class="muted tabular">{progress.stage === "verifying" ? "Checking the download…" : `${formatBytes(progress.completedBytes)} of ${formatBytes(progress.totalBytes)}`}</span>
          </div>
        )}
      </div>
    </StepBody>
  );
}

function TryStep({ onSuccess }: { onSuccess: () => void }) {
  const value = settings.value;
  const field = useRef<HTMLTextAreaElement>(null);
  const start = useRef(lastResult.value?.operationId ?? null);
  const succeeded = !!lastResult.value && lastResult.value.operationId !== start.current && lastResult.value.inserted;
  const tapped = operation.value?.phase === "error" && operation.value.error?.code === "shortPress";
  useEffect(() => field.current?.focus(), []);
  useEffect(() => {
    if (succeeded) onSuccess();
  }, [succeeded]);
  if (!value) return null;
  return (
    <StepBody
      title="Try it"
      lede={
        <>
          Click in the box, then press and hold <Keys keys={value.pushToTalk.keys} /> while you say something like “Hello from Onda”. Let go to type.
        </>
      }
    >
      <textarea ref={field} class="textarea practice-field serif" rows={4} placeholder="Your words appear here" aria-label="Practice text" />
      {succeeded ? (
        <p class="success-line" role="status">
          <CircleCheck size={18} aria-hidden="true" /> That's it. It works the same in every app.
        </p>
      ) : tapped ? (
        <p class="hint-line" role="status">
          That was a quick tap. Keep all three keys held while you speak, and let go when you're done.
        </p>
      ) : (
        <p class="muted">A small pill appears at the bottom of your screen while Onda listens.</p>
      )}
    </StepBody>
  );
}

function PolishStep() {
  const authorizing = oauthStatus.value === "authorizing";
  const connected = chatgptReady.value;
  return (
    <StepBody title="Polish with ChatGPT (optional)" lede="Connect your ChatGPT plan to clean up punctuation and filler words, and to rewrite selected text. Only written text is sent, never audio. To use your own AI service or a model on this computer instead, choose it later in Settings › AI polish.">
      {connected ? (
        <>
          <p class="success-line" role="status">
            <CircleCheck size={18} aria-hidden="true" /> Connected. AI polish is on.
          </p>
          <div class="setup-field">
            <span class="field-label">Model for polish</span>
            <ChatgptModelSelect />
            <p class="muted setup-hint">{modelAdvice(cloudModels.value ?? [])} You can change it later in Settings.</p>
          </div>
        </>
      ) : (
        <div class="button-row">
          <Button
            variant="primary"
            busy={authorizing}
            onClick={() =>
              void ask({ command: "startChatgptSignIn", args: { profileId: null } }, "unit")
                .then(refresh)
                .catch((error) => toastError(error, "Sign-in didn't start"))
            }
          >
            {authorizing ? "Finish in your browser…" : "Continue with ChatGPT"}
          </Button>
          <span class="muted">You can do this any time in Settings.</span>
        </div>
      )}
    </StepBody>
  );
}

function DoneStep({ launch, onLaunch }: { launch: boolean; onLaunch: (value: boolean) => void }) {
  const value = settings.value;
  if (!value) return null;
  return (
    <StepBody title="You're all set" lede="Onda stays in the tray, ready whenever you hold your shortcut.">
      <ul class="promises">
        <li>
          <span class="promise-icon">
            <Mic size={18} />
          </span>
          <span>
            <b>Dictate:</b> hold <Keys keys={value.pushToTalk.keys} /> anywhere.
          </span>
        </li>
        <li>
          <span class="promise-icon">
            <WandSparkles size={18} />
          </span>
          <span>
            <b>Teach it:</b> add names and jargon in Dictionary.
          </span>
        </li>
      </ul>
      <label class="check-row setup-launch">
        <Switch checked={launch} onChange={onLaunch} label="Start Onda when you sign in" />
        <span>Start Onda when you sign in</span>
      </label>
    </StepBody>
  );
}

export function Onboarding() {
  const [index, setIndex] = useState(0);
  const [launch, setLaunch] = useState(true);
  const [finishing, setFinishing] = useState(false);
  const step = STEPS[index] ?? "welcome";
  const modelId = settings.value?.language === "en" ? "whisper-tiny-en" : "whisper-tiny";
  const modelInstalled = installedIds.value.has(snapshot.value?.selectedModelId ?? modelId);
  const canContinue = step !== "model" || modelInstalled;
  const finish = async () => {
    setFinishing(true);
    const error = await saveSettings({ onboardingCompleted: true, launchAtLogin: launch });
    setFinishing(false);
    if (!error) navigate("home");
  };
  const next = () => (step === "done" ? void finish() : setIndex(Math.min(STEPS.length - 1, index + 1)));
  const rerun = !!settings.value?.onboardingCompleted;
  return (
    <div class="setup">
      <aside class="setup-art" aria-hidden="true">
        <Lockup height={26} />
        <div class="setup-illustration">
          <Art step={step} />
        </div>
        <p class="setup-line">Your voice, typed where your cursor is.</p>
      </aside>
      <main class="setup-main">
        <div class="setup-progress" role="progressbar" aria-label="Setup progress" aria-valuemin={1} aria-valuemax={STEPS.length} aria-valuenow={index + 1}>
          {STEPS.map((item, at) => (
            <i key={item} class={at <= index ? "on" : undefined} />
          ))}
        </div>
        {step === "welcome" && <Welcome />}
        {step === "microphone" && <MicrophoneStep />}
        {step === "model" && <ModelStep />}
        {step === "try" && <TryStep onSuccess={() => undefined} />}
        {step === "polish" && <PolishStep />}
        {step === "done" && <DoneStep launch={launch} onLaunch={setLaunch} />}
        <footer class="setup-foot">
          {index > 0 ? (
            <Button variant="ghost" icon={<ArrowLeft size={16} />} onClick={() => setIndex(index - 1)}>
              Back
            </Button>
          ) : rerun ? (
            <Button variant="ghost" onClick={() => navigate("home")}>
              Close
            </Button>
          ) : (
            <span />
          )}
          <div class="button-row">
            {(step === "microphone" || step === "try" || step === "polish") && (
              <Button variant="ghost" onClick={next}>
                Skip
              </Button>
            )}
            <Button variant="primary" busy={finishing} disabled={!canContinue} onClick={next}>
              {step === "welcome" ? "Get started" : step === "done" ? "Start using Onda" : "Continue"}
              {step !== "done" && <ArrowRight size={16} />}
            </Button>
          </div>
        </footer>
      </main>
    </div>
  );
}
