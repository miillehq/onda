import { useState, useEffect } from "preact/hooks";
import { KeyRound, Trash2 } from "lucide-preact";
import { Button } from "../../components/controls";
import type { AiProvider } from "../../generated/core";
import { isMac, isWindows, removeProviderKey, saveProviderKey, toast } from "../../lib/store";

/** An address field that saves when it loses focus; native code checks the address policy. */
export function AddressField({ id, label, hint, value, placeholder, onSave }: { id: string; label: string; hint: string; value: string | null; placeholder: string; onSave: (value: string | null) => void }) {
  const [text, setText] = useState(value ?? "");
  useEffect(() => setText(value ?? ""), [value]);
  return (
    <div class="provider-field">
      <label class="field-label" for={id}>
        {label}
      </label>
      <input
        id={id}
        class="input mono"
        type="url"
        inputMode="url"
        spellcheck={false}
        autoComplete="off"
        placeholder={placeholder}
        value={text}
        onInput={(event) => setText(event.currentTarget.value)}
        onBlur={() => {
          const next = text.trim() || null;
          if (next !== value) onSave(next);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
        }}
      />
      <p class="field-hint">{hint}</p>
    </div>
  );
}

/** Saved keys are never shown again; the field only replaces or removes them. */
export function KeyField({ provider, saved, required }: { provider: AiProvider; saved: boolean; required: boolean }) {
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const store = isWindows.value ? "protected for your Windows account" : isMac.value ? "in your Mac's keychain" : "in your desktop keyring";
  const save = async () => {
    setBusy(true);
    if (await saveProviderKey(provider, key)) {
      setKey("");
      toast({ tone: "ok", title: "Key saved", body: `Stored ${store}.` });
    }
    setBusy(false);
  };
  return (
    <div class="provider-field">
      <span class="field-label">API key{required ? "" : " (if your server needs one)"}</span>
      {saved ? (
        <div class="key-saved">
          <KeyRound size={16} aria-hidden="true" />
          <span>Saved, {store}.</span>
          <Button size="sm" variant="ghost" icon={<Trash2 size={15} />} onClick={() => void removeProviderKey(provider)}>
            Remove
          </Button>
        </div>
      ) : (
        <div class="key-entry">
          <input class="input mono" type="password" autoComplete="off" spellcheck={false} aria-label="API key" placeholder="Paste the key" value={key} onInput={(event) => setKey(event.currentTarget.value)} />
          <Button size="sm" disabled={!key.trim()} busy={busy} onClick={() => void save()}>
            Save key
          </Button>
        </div>
      )}
      <p class="field-hint">Onda keeps the key out of settings, logs and this window, and sends it only to this provider.</p>
    </div>
  );
}
