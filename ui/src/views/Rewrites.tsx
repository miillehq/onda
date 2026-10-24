import { Copy, Mic, MousePointer2, Pencil, Plus, Sparkles, Trash2, WandSparkles, X } from "lucide-preact";
import { useEffect, useState } from "preact/hooks";
import { ask, toAppError } from "../api";
import { consumeEditorIntent, editorIntent } from "../app/intents";
import { Button, Chip, Field, IconButton, Keys, Select, Switch } from "../components/controls";
import { Diff } from "../components/feedback";
import { Callout, Dialog, Page } from "../components/layout";
import { ShortcutRecorder } from "../components/ShortcutRecorder";
import type { OperationResult, Transform } from "../generated/core";
import { navigate } from "../lib/router";
import { aiReady, isBusy, saveSettings, settings, toast, toastError, transforms } from "../lib/store";

function RewriteDialog({ transform, onClose }: { transform: Transform | null; onClose: () => void }) {
  const [name, setName] = useState(transform?.name ?? "");
  const [instruction, setInstruction] = useState(transform?.instruction ?? "");
  const [shortcut, setShortcut] = useState<number[] | null>(transform?.shortcut?.keys ?? null);
  const [preserve, setPreserve] = useState(transform?.preserveProtectedTokens ?? true);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const save = async (event: Event) => {
    event.preventDefault();
    if (!name.trim() || !instruction.trim()) {
      setError("Give the rewrite a name and an instruction.");
      return;
    }
    setSaving(true);
    try {
      transforms.value = await ask(
        {
          command: "saveTransform",
          args: {
            transform: {
              id: transform?.id ?? crypto.randomUUID(),
              name: name.trim(),
              instruction: instruction.trim(),
              shortcut: shortcut ? { keys: shortcut } : null,
              modelPreference: transform?.modelPreference ?? null,
              preserveProtectedTokens: preserve,
              builtIn: transform?.builtIn ?? false,
            },
          },
        },
        "transforms",
      );
      toast({ tone: "ok", title: transform ? "Rewrite saved" : "Rewrite created" }, 2400);
      onClose();
    } catch (failure) {
      setError(toAppError(failure).message);
    } finally {
      setSaving(false);
    }
  };
  return (
    <Dialog
      open
      size="lg"
      title={transform ? `Edit “${transform.name}”` : "New rewrite"}
      description="Write the instruction the way you'd ask a colleague."
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" form="rewrite-form" busy={saving}>
            {transform ? "Save" : "Create rewrite"}
          </Button>
        </>
      }
    >
      <form id="rewrite-form" class="form" onSubmit={save}>
        <Field label="Name" error={null}>
          {({ id }) => <input id={id} data-autofocus class="input" value={name} placeholder="Friendly reply" onInput={(event) => setName((event.currentTarget as HTMLInputElement).value)} />}
        </Field>
        <Field label="Instruction" hint="For example: Rewrite as a warm, brief reply. Keep any dates and names." error={error}>
          {({ id, describedBy, invalid }) => (
            <textarea id={id} class="textarea" rows={4} value={instruction} aria-describedby={describedBy} aria-invalid={invalid} onInput={(event) => setInstruction((event.currentTarget as HTMLTextAreaElement).value)} />
          )}
        </Field>
        <div class="field">
          <span class="field-label">Its own shortcut</span>
          {shortcut ? (
            <div class="button-row">
              <ShortcutRecorder
                value={shortcut}
                label={name || "this rewrite"}
                onSave={async (keys) => {
                  setShortcut(keys);
                  return null;
                }}
              />
              <IconButton label="Remove shortcut" icon={<X size={16} />} onClick={() => setShortcut(null)} />
            </div>
          ) : (
            <div>
              <Button size="sm" icon={<Plus size={15} />} onClick={() => setShortcut(settings.value?.transformShortcut.keys.slice(0, -1).concat(0x31) ?? [0xa2, 0xa4, 0x31])}>
                Add a shortcut
              </Button>
              <p class="field-hint">Optional. Without one, use it from this page or make it the default.</p>
            </div>
          )}
        </div>
        <label class="check-row">
          <Switch checked={preserve} onChange={setPreserve} label="Keep links, emails and numbers exactly" />
          <span>Keep links, emails and numbers exactly</span>
        </label>
      </form>
    </Dialog>
  );
}

function TryPanel({ initial }: { initial: string }) {
  const [source, setSource] = useState("");
  const [transformId, setTransformId] = useState(initial);
  const [result, setResult] = useState<OperationResult | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => setTransformId(initial), [initial]);
  const run = async () => {
    setRunning(true);
    setError(null);
    try {
      setResult(await ask({ command: "previewTransform", args: { source, transformId } }, "result"));
    } catch (failure) {
      setResult(null);
      setError(toAppError(failure).message);
    } finally {
      setRunning(false);
    }
  };
  return (
    <section class="card try" aria-labelledby="try-title">
      <div class="try-head">
        <h3 id="try-title">Try a rewrite</h3>
        <Select label="Rewrite" value={transformId} onChange={setTransformId} options={transforms.value.map((transform) => ({ value: transform.id, label: transform.name }))} />
      </div>
      <div class="try-grid">
        <textarea class="textarea" rows={5} placeholder="Paste or type some text" aria-label="Text to rewrite" value={source} onInput={(event) => setSource((event.currentTarget as HTMLTextAreaElement).value)} />
        <div class="try-output" aria-live="polite">
          {error ? (
            <p class="field-error">{error}</p>
          ) : result ? (
            <>
              <Diff segments={result.diff} />
              <Button
                size="sm"
                icon={<Copy size={15} />}
                onClick={() =>
                  void navigator.clipboard
                    .writeText(result.output)
                    .then(() => toast({ tone: "ok", title: "Copied" }, 2000))
                    .catch(() => toast({ tone: "error", title: "Couldn't copy" }))
                }
              >
                Copy result
              </Button>
            </>
          ) : (
            <p class="muted">The rewritten text appears here, with changes marked.</p>
          )}
        </div>
      </div>
      <div class="button-row">
        <Button variant="primary" icon={<WandSparkles size={16} />} busy={running} disabled={!source.trim() || !aiReady.value || isBusy.value} onClick={() => void run()}>
          Rewrite
        </Button>
        {!aiReady.value && <span class="muted">Connect ChatGPT first.</span>}
      </div>
    </section>
  );
}

export function Rewrites() {
  const value = settings.value;
  const [editing, setEditing] = useState<Transform | null | "new">(null);
  const [tryId, setTryId] = useState(value?.defaultTransformId ?? "polish");
  useEffect(() => {
    if (consumeEditorIntent("rewrites")) setEditing("new");
  }, [editorIntent.value]);
  if (!value) return null;

  const remove = async (transform: Transform) => {
    try {
      await ask({ command: "deleteTransform", args: { id: transform.id } }, "unit");
      transforms.value = transforms.value.filter((item) => item.id !== transform.id);
      toast({ tone: "info", title: `Removed “${transform.name}”` });
    } catch (error) {
      toastError(error, "Not removed");
    }
  };

  return (
    <Page
      title="Rewrites"
      lede="Select text in any app, press a shortcut, and ChatGPT rewrites it in place."
      actions={
        <Button variant="primary" icon={<Plus size={16} />} onClick={() => setEditing("new")}>
          New rewrite
        </Button>
      }
    >
      {!aiReady.value && (
        <Callout
          tone="info"
          icon={<Sparkles size={20} />}
          title="Connect ChatGPT to use rewrites"
          action={
            <Button variant="primary" onClick={() => navigate("settings/ai")}>
              Set up AI polish
            </Button>
          }
        >
          Rewrites use your ChatGPT plan. Dictation keeps working on this PC without it.
        </Callout>
      )}

      <ol class="steps" aria-label="How rewrites work">
        <li>
          <span class="step-icon">
            <MousePointer2 size={18} />
          </span>
          <span>
            <b>Select text</b> in any app
          </span>
        </li>
        <li>
          <span class="step-icon">
            <Keys keys={value.transformShortcut.keys} />
          </span>
          <span>
            <b>Press</b> the rewrite shortcut
          </span>
        </li>
        <li>
          <span class="step-icon">
            <WandSparkles size={18} />
          </span>
          <span>
            <b>It's replaced</b>, ready to undo from Home
          </span>
        </li>
      </ol>

      <div class="rows">
        <div class="row">
          <span class="row-icon">
            <WandSparkles size={18} />
          </span>
          <div class="row-text">
            <div class="row-title">The shortcut runs</div>
            <div class="row-description">
              <Keys keys={value.transformShortcut.keys} /> applies this rewrite to your selection.
            </div>
          </div>
          <div class="row-control">
            <Select label="Default rewrite" value={value.defaultTransformId} onChange={(defaultTransformId) => void saveSettings({ defaultTransformId })} options={transforms.value.map((transform) => ({ value: transform.id, label: transform.name }))} />
          </div>
        </div>
        <div class="row">
          <span class="row-icon">
            <Mic size={18} />
          </span>
          <div class="row-text">
            <div class="row-title">Edit with your voice</div>
            <div class="row-description">
              Select text, hold <Keys keys={value.commandShortcut.keys} /> and say what to change, like “make this friendlier”.
            </div>
          </div>
          <div class="row-control">
            <Button variant="ghost" size="sm" onClick={() => navigate("settings/shortcuts")}>
              Change keys
            </Button>
          </div>
        </div>
      </div>

      <ul class="rewrite-list" aria-label="Your rewrites">
        {transforms.value.map((transform) => (
          <li class="rewrite" key={transform.id}>
            <div class="rewrite-text">
              <div class="rewrite-name">
                {transform.name}
                {transform.id === value.defaultTransformId && <Chip tone="strong">Default</Chip>}
                {transform.shortcut && <Keys keys={transform.shortcut.keys} />}
              </div>
              <p class="rewrite-instruction">{transform.instruction}</p>
            </div>
            <div class="rewrite-actions">
              <Button size="sm" variant="ghost" onClick={() => setTryId(transform.id)}>
                Try
              </Button>
              <IconButton label={`Edit ${transform.name}`} icon={<Pencil size={17} />} onClick={() => setEditing(transform)} />
              {!transform.builtIn && <IconButton label={`Remove ${transform.name}`} icon={<Trash2 size={17} />} onClick={() => void remove(transform)} />}
            </div>
          </li>
        ))}
      </ul>

      <TryPanel initial={tryId} />
      {editing && <RewriteDialog transform={editing === "new" ? null : editing} onClose={() => setEditing(null)} />}
    </Page>
  );
}
