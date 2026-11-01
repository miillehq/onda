import { CircleCheck, Cpu, Download, FolderInput, HardDrive, Languages, Layers, Mic, ShieldCheck, TextCursor, TriangleAlert, X } from "lucide-preact";
import { useState } from "preact/hooks";
import { Mark } from "../../components/Brand";
import { Button, Chip, Field, ProgressBar, Segmented, Select } from "../../components/controls";
import { Callout, Dialog, Page, Row, Rows, Section } from "../../components/layout";
import type { ModelInfo } from "../../generated/api";
import type { QualityProfile, Settings } from "../../generated/core";
import { formatBytes, providerName } from "../../lib/format";
import { LANGUAGES, languageName } from "../../lib/languages";
import {
  aiReady,
  cancelDownload,
  catalog,
  downloadModel,
  downloads,
  installedIds,
  micName,
  removeModel,
  saveSettings,
  selectedModel,
  settings,
  settingsPending,
  snapshot,
} from "../../lib/store";

type Profile = Exclude<QualityProfile, "custom">;

const PROFILES: { id: Profile; name: string; description: string; speed: number; accuracy: number; models: [english: string, multilingual: string] }[] = [
  { id: "fast", name: "Fast", description: "Near-instant. Great for messages and quick notes.", speed: 5, accuracy: 2, models: ["whisper-tiny-en", "whisper-tiny"] },
  { id: "balanced", name: "Balanced", description: "Fewer mistakes on names and longer sentences.", speed: 4, accuracy: 3, models: ["whisper-base-en-q5", "whisper-base-q5"] },
  { id: "bestQuality", name: "Quality", description: "Most accurate. Best for long, careful writing.", speed: 2, accuracy: 4, models: ["whisper-small-en-q5", "whisper-small-q5"] },
];

/**
 * The model these settings ask for. While a change is being saved the native answer still
 * describes the previous settings, so the UI shows the requested model instead of flickering
 * through "Selected" and memory notes.
 */
function modelInUse(value: Settings): string | null {
  if (!settingsPending.value) return snapshot.value?.selectedModelId ?? null;
  if (value.qualityProfile === "custom") return value.modelId;
  return PROFILES.find((profile) => profile.id === value.qualityProfile)?.models[value.language === "en" ? 0 : 1] ?? null;
}

function Meter({ label, value }: { label: string; value: number }) {
  return (
    <div class="meter-row">
      <span>{label}</span>
      <span class="segments" role="img" aria-label={`${label}: ${value} of 5`}>
        {[1, 2, 3, 4, 5].map((step) => (
          <i key={step} class={step <= value ? "on" : undefined} />
        ))}
      </span>
    </div>
  );
}

function DownloadState({ model }: { model: ModelInfo }) {
  const progress = downloads.value[model.id];
  if (!progress) return null;
  const verifying = progress.stage === "verifying";
  return (
    <div class="download-state">
      <ProgressBar value={verifying ? null : progress.completedBytes / Math.max(1, progress.totalBytes)} label={`Downloading ${model.name}`} />
      <div class="download-meta">
        <span class="tabular">{verifying ? "Checking the download…" : `${formatBytes(progress.completedBytes)} of ${formatBytes(progress.totalBytes || model.downloadSizeBytes)}`}</span>
        <Button variant="ghost" size="sm" icon={<X size={14} />} onClick={() => void cancelDownload(model.id)}>
          Cancel
        </Button>
      </div>
    </div>
  );
}

function ProfileCard({ profile }: { profile: (typeof PROFILES)[number] }) {
  const value = settings.value;
  if (!value) return null;
  const modelId = profile.models[value.language === "en" ? 0 : 1];
  const model = catalog.value.find((item) => item.id === modelId);
  const installed = installedIds.value.has(modelId);
  // A custom pick of exactly this card's model (as older versions saved it) counts as this card.
  const selected = value.qualityProfile === profile.id || (value.qualityProfile === "custom" && value.modelId === modelId);
  // Onda can fall back to a smaller model on low-memory PCs; only the model really used is "In use".
  const inUse = modelInUse(value) === modelId;
  const downloading = !!downloads.value[modelId];
  const choose = () => {
    if (selected) return;
    if (installed) void saveSettings({ qualityProfile: profile.id });
    else if (!downloading) void downloadModel(modelId, { qualityProfile: profile.id });
  };
  return (
    <div class={`model-card${selected ? " is-selected" : ""}`}>
      <button type="button" class="model-choose" role="radio" aria-checked={selected} onClick={choose} aria-label={`${profile.name}. ${profile.description}${installed ? "" : " Needs a download."}`}>
        <span class="model-top">
          <span class="model-name">{profile.name}</span>
          <span class="radio" aria-hidden="true" />
        </span>
        <span class="model-description">{profile.description}</span>
        <Meter label="Speed" value={profile.speed} />
        <Meter label="Accuracy" value={profile.accuracy} />
      </button>
      <div class="model-foot">
        {model && downloading ? (
          <DownloadState model={model} />
        ) : (
          <>
            {selected && installed && inUse ? (
              <span class="in-use">
                <CircleCheck size={15} aria-hidden="true" /> In use
              </span>
            ) : selected && installed ? (
              <span class="in-use is-muted">Selected</span>
            ) : installed ? (
              <Button size="sm" onClick={choose} aria-label={`Use ${profile.name}`}>
                Use
              </Button>
            ) : (
              <Button size="sm" icon={<Download size={15} />} onClick={choose} aria-label={`Download and use ${profile.name}`}>
                Download
              </Button>
            )}
            <span class="model-size tabular">{model ? formatBytes(model.downloadSizeBytes) : ""}</span>
          </>
        )}
      </div>
    </div>
  );
}

function MoveModelsDialog({ onClose }: { onClose: () => void }) {
  const value = settings.value;
  const [path, setPath] = useState(value?.modelDirectory ?? "");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const save = async (event: Event) => {
    event.preventDefault();
    setSaving(true);
    const failure = await saveSettings({ modelDirectory: path.trim() || null }, { quiet: true });
    setSaving(false);
    if (failure) setError(failure.message);
    else onClose();
  };
  return (
    <Dialog
      open
      title="Move speech models"
      description="Onda copies your models to the new folder, checks them, then switches over."
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" form="move-form" busy={saving}>
            Move models
          </Button>
        </>
      }
    >
      <form id="move-form" class="form" onSubmit={save}>
        <Field label="Folder" hint="A folder on a local drive, for example D:\Onda\models. Leave empty to use Onda's own folder." error={error}>
          {({ id, describedBy, invalid }) => <input id={id} data-autofocus class="input mono" value={path} placeholder="D:\Onda\models" aria-describedby={describedBy} aria-invalid={invalid} onInput={(event) => setPath((event.currentTarget as HTMLInputElement).value)} />}
        </Field>
      </form>
    </Dialog>
  );
}

function AllModels() {
  const value = settings.value;
  if (!value) return null;
  return (
    <details class="disclosure">
      <summary>
        <Layers size={18} aria-hidden="true" />
        <span>
          <b>All models</b>
          <span class="muted"> · pick a specific model or remove ones you don't use</span>
        </span>
      </summary>
      <ul class="model-list">
        {catalog.value.map((model) => {
          const installed = installedIds.value.has(model.id);
          const inUse = !!value && modelInUse(value) === model.id;
          const downloading = !!downloads.value[model.id];
          return (
            <li key={model.id} class="model-line">
              <div class="model-line-text">
                <span class="model-line-name">
                  {model.name}
                  {inUse && <Chip tone="strong">In use</Chip>}
                </span>
                <span class="muted tabular">
                  {formatBytes(model.downloadSizeBytes)} download · about {formatBytes(model.memoryBytes)} memory · {model.license} licence
                </span>
                {downloading && <DownloadState model={model} />}
              </div>
              {!downloading && (
                <div class="button-row">
                  {installed && !inUse && (
                    <Button size="sm" onClick={() => void saveSettings({ qualityProfile: "custom", modelId: model.id })}>
                      Use
                    </Button>
                  )}
                  {!installed && (
                    <Button size="sm" icon={<Download size={15} />} onClick={() => void downloadModel(model.id)}>
                      Download
                    </Button>
                  )}
                  {installed && !inUse && (
                    <Button size="sm" variant="ghost" onClick={() => void removeModel(model.id)}>
                      Remove
                    </Button>
                  )}
                </div>
              )}
            </li>
          );
        })}
      </ul>
    </details>
  );
}

/** What the language setting can and can't promise with the model in use. */
function LanguageNotes({ language, englishOnlyModel }: { language: string | null; englishOnlyModel: string | null }) {
  return (
    <>
      {englishOnlyModel && language !== "en" && (
        <Callout tone="warn" icon={<TriangleAlert size={20} />} title={`${englishOnlyModel} understands English only`}>
          It ignores the spoken language setting and writes everything as English. Choose a multilingual model above or in All models to dictate in {language ? languageName(language) : "other languages"}.
        </Callout>
      )}
      {language === "zh" && (
        <p class="footnote">
          One setting covers all Chinese. The models learned mostly from Mandarin and may mix Simplified and Traditional characters. Cantonese isn't supported.
        </p>
      )}
      {language !== "zh" && (
        <p class="footnote">The multilingual models know 99 languages; this list has the most common.</p>
      )}
    </>
  );
}

export function Speech() {
  const value = settings.value;
  const [moving, setMoving] = useState(false);
  if (!value) return null;
  const hardware = snapshot.value?.hardware;
  const expected = value.qualityProfile === "custom" ? null : PROFILES.find((profile) => profile.id === value.qualityProfile)?.models[value.language === "en" ? 0 : 1];
  const resolved = modelInUse(value);
  const downgraded = expected && resolved && expected !== resolved;
  const resolvedModel = catalog.value.find((model) => model.id === resolved) ?? selectedModel.value;
  const missing = resolved && !installedIds.value.has(resolved) && !downloads.value[resolved];
  const gpus = hardware?.gpus.map((gpu) => gpu.name).join(", ");
  return (
    <Page title="Speech and language" lede="How Onda turns your voice into text.">
      <div class="flow" aria-label="Where your audio goes">
        <div class="flow-track">
          <div class="flow-node">
            <span class="flow-icon">
              <Mic size={20} />
            </span>
            <span>
              <b>Your voice</b>
              <span class="flow-sub">{micName.value}</span>
            </span>
          </div>
          <span class="flow-line" aria-hidden="true" />
          <div class="flow-node">
            <span class="flow-icon is-tile">
              <Mark height={14} />
            </span>
            <span>
              <b>Onda, on this PC</b>
              <span class="flow-sub">{resolvedModel?.name ?? "Speech model"}</span>
            </span>
          </div>
          <span class="flow-line" aria-hidden="true" />
          <div class="flow-node">
            <span class="flow-icon">
              <TextCursor size={20} />
            </span>
            <span>
              <b>Your cursor</b>
              <span class="flow-sub">In any app</span>
            </span>
          </div>
        </div>
        <p class="flow-foot">
          <ShieldCheck size={16} aria-hidden="true" />
          <b>Your audio never leaves your PC.</b>
          <span>{value.enhancementEnabled && aiReady.value ? (value.aiProvider === "local" ? "AI polish uses a model on this computer, so your text stays here too." : `With AI polish on, only the written text goes to ${providerName(value)}.`) : "AI polish is off, so your text stays here too."}</span>
        </p>
      </div>

      {missing && resolvedModel && (
        <Callout
          tone="warn"
          icon={<TriangleAlert size={20} />}
          title={`${resolvedModel.name} isn't downloaded yet`}
          action={
            <Button variant="primary" icon={<Download size={16} />} onClick={() => void downloadModel(resolvedModel.id)}>
              Download {formatBytes(resolvedModel.downloadSizeBytes)}
            </Button>
          }
        >
          Onda needs it before you can dictate with these settings.
        </Callout>
      )}

      <Section title="Recognition quality" description="Bigger models make fewer mistakes but take a little longer to write out what you said.">
        <div class="model-cards" role="radiogroup" aria-label="Recognition quality">
          {PROFILES.map((profile) => (
            <ProfileCard profile={profile} key={profile.id} />
          ))}
        </div>
        {downgraded && resolvedModel && (
          <p class="footnote">
            This PC has {formatBytes(hardware?.totalMemoryBytes ?? 0)} of memory, which isn't enough for {PROFILES.find((profile) => profile.id === value.qualityProfile)?.name}, so Onda uses {resolvedModel.name} instead.
          </p>
        )}
        {value.qualityProfile === "custom" && resolvedModel && !PROFILES.some((profile) => profile.models[value.language === "en" ? 0 : 1] === resolvedModel.id) && (
          <p class="footnote">You picked {resolvedModel.name} from all models. Choose a card above to go back to a preset.</p>
        )}
      </Section>

      <Section title="Language and processing" description="The defaults work well for most people.">
        <Rows>
          <Row
            icon={<Languages size={18} />}
            title="Spoken language"
            description="Auto-detect handles many languages. If you only speak English, pick it for faster, more accurate results."
            control={
              <Select
                label="Spoken language"
                value={value.language ?? ""}
                onChange={(code) => void saveSettings({ language: code || null })}
                options={[{ value: "", label: "Auto-detect" }, ...LANGUAGES.map((language) => ({ value: language.code, label: language.name }))]}
              />
            }
          >
            <LanguageNotes language={value.language} englishOnlyModel={resolvedModel?.englishOnly ? resolvedModel.name : null} />
          </Row>
          <Row icon={<Cpu size={18} />} title="Processor" description={gpus ? `Automatic uses your graphics card (${gpus}) when it helps.` : "Automatic uses your graphics card when it helps."}>
            <Segmented
              label="Processor"
              value={value.executionPreference}
              onChange={(executionPreference) => void saveSettings({ executionPreference })}
              options={[
                { value: "auto", label: "Automatic" },
                { value: "cpuOnly", label: "CPU only" },
                { value: "gpuPreferred", label: "Graphics card" },
              ]}
            />
            {value.executionPreference === "gpuPreferred" && hardware && !hardware.vulkanWorkerInstalled && <p class="footnote">This build doesn't include graphics card acceleration, so Onda uses the CPU.</p>}
          </Row>
          <Row
            icon={<HardDrive size={18} />}
            title="Model storage"
            description={value.modelDirectory ? <span class="mono selectable">{value.modelDirectory}</span> : "In Onda's own app data folder."}
            control={
              <Button size="sm" icon={<FolderInput size={15} />} onClick={() => setMoving(true)}>
                Move
              </Button>
            }
          />
        </Rows>
        <AllModels />
      </Section>
      {moving && <MoveModelsDialog onClose={() => setMoving(false)} />}
    </Page>
  );
}
