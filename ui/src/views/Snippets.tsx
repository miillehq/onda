import { ArrowRight, Pencil, Plus, TextQuote, Trash2 } from "lucide-preact";
import { useEffect, useState } from "preact/hooks";
import { ask, toAppError } from "../api";
import { consumeEditorIntent, editorIntent } from "../app/intents";
import { Button, Field, IconButton, Switch } from "../components/controls";
import { Dialog, EmptyState, Page } from "../components/layout";
import type { Snippet } from "../generated/core";
import { snippets, toast, toastError } from "../lib/store";

function SnippetDialog({ snippet, onClose }: { snippet: Snippet | null; onClose: () => void }) {
  const [trigger, setTrigger] = useState(snippet?.trigger ?? "");
  const [expansion, setExpansion] = useState(snippet?.expansion ?? "");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const save = async (event: Event) => {
    event.preventDefault();
    if (!trigger.trim() || !expansion.trim()) {
      setError("Fill in both the phrase and the text.");
      return;
    }
    setSaving(true);
    try {
      snippets.value = await ask({ command: "saveSnippet", args: { snippet: { id: snippet?.id ?? crypto.randomUUID(), trigger: trigger.trim(), expansion, enabled: snippet?.enabled ?? true } } }, "snippets");
      toast({ tone: "ok", title: snippet ? "Snippet saved" : "Snippet created" }, 2400);
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
      title={snippet ? "Edit snippet" : "New snippet"}
      description="Say the phrase while dictating and Onda types the full text instead."
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" form="snippet-form" busy={saving}>
            {snippet ? "Save" : "Create snippet"}
          </Button>
        </>
      }
    >
      <form id="snippet-form" class="form" onSubmit={save}>
        <Field label="When you say" hint="A short, distinctive phrase works best." error={null}>
          {({ id, describedBy }) => <input id={id} data-autofocus class="input" value={trigger} placeholder="my address" aria-describedby={describedBy} onInput={(event) => setTrigger((event.currentTarget as HTMLInputElement).value)} />}
        </Field>
        <Field label="Type this" error={error}>
          {({ id, describedBy, invalid }) => (
            <textarea id={id} class="textarea" rows={5} value={expansion} placeholder="221B Baker Street, London NW1 6XE" aria-describedby={describedBy} aria-invalid={invalid} onInput={(event) => setExpansion((event.currentTarget as HTMLTextAreaElement).value)} />
          )}
        </Field>
      </form>
    </Dialog>
  );
}

export function Snippets() {
  const [editing, setEditing] = useState<Snippet | null | "new">(null);
  useEffect(() => {
    if (consumeEditorIntent("snippets")) setEditing("new");
  }, [editorIntent.value]);

  const toggle = async (snippet: Snippet, enabled: boolean) => {
    try {
      snippets.value = await ask({ command: "saveSnippet", args: { snippet: { ...snippet, enabled } } }, "snippets");
    } catch (error) {
      toastError(error, "Not saved");
    }
  };
  const remove = async (snippet: Snippet) => {
    try {
      await ask({ command: "deleteSnippet", args: { id: snippet.id } }, "unit");
      snippets.value = snippets.value.filter((item) => item.id !== snippet.id);
      toast({
        tone: "info",
        title: `Removed “${snippet.trigger}”`,
        action: { label: "Undo", run: () => void ask({ command: "saveSnippet", args: { snippet } }, "snippets").then((list) => (snippets.value = list)).catch((error) => toastError(error)) },
      });
    } catch (error) {
      toastError(error, "Not removed");
    }
  };

  const list = [...snippets.value].sort((a, b) => a.trigger.localeCompare(b.trigger));
  return (
    <Page
      title="Snippets"
      lede="Say a short phrase and Onda types the full text: addresses, links, sign-offs, anything you repeat."
      actions={
        list.length > 0 && (
          <Button variant="primary" icon={<Plus size={16} />} onClick={() => setEditing("new")}>
            New snippet
          </Button>
        )
      }
    >
      {list.length ? (
        <ul class="snippet-grid">
          {list.map((snippet) => (
            <li class={`snippet${snippet.enabled ? "" : " is-disabled"}`} key={snippet.id}>
              <div class="snippet-said">
                <span class="said-quote">“{snippet.trigger}”</span>
                <ArrowRight size={15} aria-hidden="true" />
              </div>
              <p class="snippet-text selectable">{snippet.expansion}</p>
              <div class="snippet-actions">
                <Switch checked={snippet.enabled} label={`Use “${snippet.trigger}”`} onChange={(enabled) => void toggle(snippet, enabled)} />
                <span class="grow" />
                <IconButton label="Edit" icon={<Pencil size={17} />} onClick={() => setEditing(snippet)} />
                <IconButton label="Remove" icon={<Trash2 size={17} />} onClick={() => void remove(snippet)} />
              </div>
            </li>
          ))}
        </ul>
      ) : (
        <EmptyState
          art={<TextQuote size={20} />}
          title="Create your first snippet"
          action={
            <Button variant="primary" icon={<Plus size={16} />} onClick={() => setEditing("new")}>
              New snippet
            </Button>
          }
        >
          For example, say “my address” and Onda types your full postal address.
        </EmptyState>
      )}
      {editing && <SnippetDialog snippet={editing === "new" ? null : editing} onClose={() => setEditing(null)} />}
    </Page>
  );
}
