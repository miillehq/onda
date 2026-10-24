import { ArrowRight, Download, Mic, Pencil, Plus, Search, Sparkles, Star, Trash2, Upload } from "lucide-preact";
import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { ask, toAppError } from "../api";
import { consumeEditorIntent, editorIntent } from "../app/intents";
import { Button, Chip, IconButton, Segmented, Switch } from "../components/controls";
import { EmptyState, Page } from "../components/layout";
import type { DictionaryEntry, DictionaryKind } from "../generated/core";
import { plural } from "../lib/format";
import { dictionary, saveSettings, settings, toast, toastError } from "../lib/store";
import { TeachWord } from "./TeachWord";

type Draft = { id: string | null; phrase: string; replacement: string };

function Editor({ kind, draft, onCancel, onSaved }: { kind: DictionaryKind; draft: Draft; onCancel: () => void; onSaved: (entry: DictionaryEntry) => void }) {
  const [phrase, setPhrase] = useState(draft.phrase);
  const [replacement, setReplacement] = useState(draft.replacement);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const first = useRef<HTMLInputElement>(null);
  useEffect(() => first.current?.focus(), []);
  const existing = draft.id ? dictionary.value.find((entry) => entry.id === draft.id) : undefined;

  const submit = async (event: Event) => {
    event.preventDefault();
    if (!phrase.trim() || (kind === "replacement" && !replacement.trim())) {
      setError(kind === "replacement" ? "Fill in both what you say and what to write." : "Type the word or phrase.");
      return;
    }
    const entry: DictionaryEntry = {
      id: draft.id ?? crypto.randomUUID(),
      kind,
      phrase: phrase.trim(),
      replacement: kind === "replacement" ? replacement.trim() : null,
      enabled: existing?.enabled ?? true,
      starred: existing?.starred ?? false,
      source: existing?.source ?? "manual",
    };
    setSaving(true);
    try {
      dictionary.value = await ask({ command: "saveDictionaryEntry", args: { entry } }, "dictionary");
      onSaved(entry);
    } catch (failure) {
      setError(toAppError(failure).message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <form class="entry-editor" onSubmit={submit} onKeyDown={(event) => event.key === "Escape" && onCancel()}>
      <div class="entry-fields">
        <label class="inline-field">
          <span class="field-label">{kind === "replacement" ? "When you say" : "Word or phrase"}</span>
          <input ref={first} class="input" value={phrase} onInput={(event) => setPhrase((event.currentTarget as HTMLInputElement).value)} placeholder={kind === "replacement" ? "cooper netties" : "Kubernetes"} aria-invalid={!!error} />
        </label>
        {kind === "replacement" && (
          <>
            <ArrowRight size={18} class="entry-arrow" aria-hidden="true" />
            <label class="inline-field">
              <span class="field-label">Write</span>
              <input class="input" value={replacement} onInput={(event) => setReplacement((event.currentTarget as HTMLInputElement).value)} placeholder="Kubernetes" aria-invalid={!!error} />
            </label>
          </>
        )}
      </div>
      {error && (
        <p class="field-error" role="alert">
          {error}
        </p>
      )}
      <div class="button-row">
        <Button variant="primary" type="submit" busy={saving}>
          {draft.id ? "Save" : kind === "replacement" ? "Add replacement" : "Add word"}
        </Button>
        <Button variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}

function EntryRow({ entry, onEdit, onTeach }: { entry: DictionaryEntry; onEdit: () => void; onTeach: () => void }) {
  const update = async (patch: Partial<DictionaryEntry>) => {
    try {
      dictionary.value = await ask({ command: "saveDictionaryEntry", args: { entry: { ...entry, ...patch } } }, "dictionary");
    } catch (error) {
      toastError(error, "Not saved");
    }
  };
  const remove = async () => {
    try {
      await ask({ command: "deleteDictionaryEntry", args: { id: entry.id } }, "unit");
      dictionary.value = dictionary.value.filter((item) => item.id !== entry.id);
      toast({
        tone: "info",
        title: `Removed “${entry.replacement ?? entry.phrase}”`,
        action: { label: "Undo", run: () => void ask({ command: "saveDictionaryEntry", args: { entry } }, "dictionary").then((list) => (dictionary.value = list)).catch((error) => toastError(error)) },
      });
    } catch (error) {
      toastError(error, "Not removed");
    }
  };
  return (
    <li class={`entry${entry.enabled ? "" : " is-disabled"}`}>
      <div class="entry-text" onDblClick={onEdit}>
        {entry.kind === "replacement" ? (
          <>
            <span class="entry-said">{entry.phrase}</span>
            <ArrowRight size={15} aria-label="becomes" />
            <span class="entry-phrase">{entry.replacement}</span>
          </>
        ) : (
          <span class="entry-phrase">{entry.phrase}</span>
        )}
        {entry.source === "autoLearned" && (
          <Chip tone="strong" icon={<Sparkles size={13} />}>
            Learned
          </Chip>
        )}
        {entry.source === "imported" && <Chip>Imported</Chip>}
      </div>
      <div class="entry-actions">
        <IconButton label={entry.starred ? "Unstar" : "Star (prioritise)"} class={entry.starred ? "is-on" : undefined} icon={<Star size={17} fill={entry.starred ? "currentColor" : "none"} />} onClick={() => void update({ starred: !entry.starred })} aria-pressed={entry.starred} />
        {entry.kind === "vocabulary" && <IconButton label="Say it to teach Onda how it sounds" icon={<Mic size={17} />} onClick={onTeach} />}
        <IconButton label="Edit" icon={<Pencil size={17} />} onClick={onEdit} />
        <IconButton label="Remove" icon={<Trash2 size={17} />} onClick={() => void remove()} />
        <Switch checked={entry.enabled} label={`Use “${entry.replacement ?? entry.phrase}”`} onChange={(enabled) => void update({ enabled })} />
      </div>
    </li>
  );
}

export function Dictionary() {
  const [kind, setKind] = useState<DictionaryKind>("vocabulary");
  const [query, setQuery] = useState("");
  const [draft, setDraft] = useState<Draft | null>(null);
  const [teaching, setTeaching] = useState<DictionaryEntry | null>(null);
  const file = useRef<HTMLInputElement>(null);
  const value = settings.value;

  useEffect(() => {
    if (consumeEditorIntent("dictionary")) setDraft({ id: null, phrase: "", replacement: "" });
  }, [editorIntent.value]);

  const counts = useMemo(
    () => ({
      vocabulary: dictionary.value.filter((entry) => entry.kind === "vocabulary").length,
      replacement: dictionary.value.filter((entry) => entry.kind === "replacement").length,
    }),
    [dictionary.value],
  );
  const visible = useMemo(() => {
    const term = query.trim().toLocaleLowerCase();
    return dictionary.value
      .filter((entry) => entry.kind === kind)
      .filter((entry) => !term || entry.phrase.toLocaleLowerCase().includes(term) || (entry.replacement ?? "").toLocaleLowerCase().includes(term))
      .sort((a, b) => Number(b.starred) - Number(a.starred) || (a.replacement ?? a.phrase).localeCompare(b.replacement ?? b.phrase));
  }, [dictionary.value, kind, query]);

  const importFile = async (event: Event) => {
    const input = event.currentTarget as HTMLInputElement;
    const chosen = input.files?.[0];
    input.value = "";
    if (!chosen) return;
    try {
      const added = await ask({ command: "importDictionary", args: { json: await chosen.text() } }, "count");
      dictionary.value = await ask({ command: "getDictionary" }, "dictionary");
      toast({ tone: "ok", title: added ? `Imported ${plural(added, "entry", "entries")}` : "Nothing new to import" });
    } catch (error) {
      toastError(error, "Import didn't work");
    }
  };
  const exportFile = async () => {
    try {
      const json = await ask({ command: "exportDictionary" }, "text");
      const url = URL.createObjectURL(new Blob([json], { type: "application/json" }));
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = "onda-dictionary.json";
      anchor.click();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (error) {
      toastError(error, "Export didn't work");
    }
  };

  return (
    <Page
      title="Dictionary"
      lede="Teach Onda the names, jargon and spellings you use. It applies them every time you dictate."
      actions={
        <>
          <input ref={file} type="file" accept=".json,application/json" class="visually-hidden" tabIndex={-1} onChange={importFile} aria-hidden="true" />
          <Button variant="ghost" icon={<Upload size={16} />} onClick={() => file.current?.click()}>
            Import
          </Button>
          <Button variant="ghost" icon={<Download size={16} />} onClick={() => void exportFile()} disabled={!dictionary.value.length}>
            Export
          </Button>
          <Button variant="primary" icon={<Plus size={16} />} onClick={() => setDraft({ id: null, phrase: "", replacement: "" })}>
            {kind === "replacement" ? "Add replacement" : "Add word"}
          </Button>
        </>
      }
    >
      <div class="explain">
        {kind === "vocabulary" ? (
          <div class="explain-art" aria-hidden="true">
            <span class="explain-said">“koo·ber·net·eez”</span>
            <ArrowRight size={16} />
            <span class="explain-written">Kubernetes</span>
          </div>
        ) : (
          <div class="explain-art" aria-hidden="true">
            <span class="explain-said">“my email”</span>
            <ArrowRight size={16} />
            <span class="explain-written">name@company.com</span>
          </div>
        )}
        <p>
          {kind === "vocabulary"
            ? "Words help Onda recognise and spell unusual terms. Star the ones that matter most. For a name spelled unlike it sounds, press the microphone next to it and say it."
            : "Replacements swap exactly what you say for exactly what you want written."}
        </p>
      </div>

      <div class="toolbar">
        <Segmented
          label="Entry type"
          value={kind}
          onChange={(next) => {
            setKind(next);
            setDraft(null);
          }}
          options={[
            { value: "vocabulary", label: `Words · ${counts.vocabulary}` },
            { value: "replacement", label: `Replacements · ${counts.replacement}` },
          ]}
        />
        <label class="search-field">
          <Search size={16} aria-hidden="true" />
          <input type="search" placeholder="Search" aria-label="Search dictionary" value={query} onInput={(event) => setQuery((event.currentTarget as HTMLInputElement).value)} />
        </label>
      </div>

      {draft && (
        <Editor
          key={draft.id ?? "new"}
          kind={kind}
          draft={draft}
          onCancel={() => setDraft(null)}
          onSaved={(entry) => {
            const isNewWord = !draft.id && entry.kind === "vocabulary";
            toast(
              isNewWord
                ? { tone: "ok", title: `Added “${entry.phrase}”`, body: "Spelled unlike it sounds? Say it so Onda learns what it hears.", action: { label: "Teach how it sounds", run: () => setTeaching(entry) } }
                : { tone: "ok", title: draft.id ? "Saved" : "Added" },
              isNewWord ? 8000 : 2400,
            );
            setDraft(null);
          }}
        />
      )}

      {visible.length ? (
        <ul class="entries" aria-label={kind === "vocabulary" ? "Words" : "Replacements"}>
          {visible.map((entry) => (
            <EntryRow key={entry.id} entry={entry} onTeach={() => setTeaching(entry)} onEdit={() => setDraft({ id: entry.id, phrase: entry.phrase, replacement: entry.replacement ?? "" })} />
          ))}
        </ul>
      ) : query ? (
        <EmptyState title={`Nothing matches “${query}”`}>Try a shorter search.</EmptyState>
      ) : (
        !draft && (
          <EmptyState
            art={<Sparkles size={20} />}
            title={kind === "vocabulary" ? "Add the words Onda should know" : "Add your first replacement"}
            action={
              <Button variant="primary" icon={<Plus size={16} />} onClick={() => setDraft({ id: null, phrase: "", replacement: "" })}>
                {kind === "vocabulary" ? "Add word" : "Add replacement"}
              </Button>
            }
          >
            {kind === "vocabulary" ? "Names, product terms and acronyms are a good start." : "Great for addresses, links and phrases you type often."}
          </EmptyState>
        )
      )}

      {value && (
        <div class="card learn-setting">
          <div>
            <h3>Learn from my corrections</h3>
            <p class="muted">When you fix a word right after dictating, Onda suggests adding it. Nothing is added without your OK.</p>
          </div>
          <Switch checked={value.learningEnabled} label="Learn from my corrections" onChange={(learningEnabled) => void saveSettings({ learningEnabled })} />
        </div>
      )}
      {teaching && <TeachWord entry={teaching} onClose={() => setTeaching(null)} />}
    </Page>
  );
}
