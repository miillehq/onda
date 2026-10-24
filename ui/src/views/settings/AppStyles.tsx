import { AppWindow, Plus, Trash2 } from "lucide-preact";
import { useState } from "preact/hooks";
import { Button, IconButton, Select, Switch } from "../../components/controls";
import { EmptyState, Page, Section } from "../../components/layout";
import type { AppProfile, PasteShortcut } from "../../generated/core";
import { appName } from "../../lib/format";
import { saveSettings, settings } from "../../lib/store";

const SUGGESTIONS = ["slack.exe", "ms-teams.exe", "OUTLOOK.EXE", "WINWORD.EXE", "chrome.exe", "Code.exe", "WindowsTerminal.exe"];
const PASTE: { value: PasteShortcut; label: string }[] = [
  { value: "controlV", label: "Ctrl + V" },
  { value: "controlShiftV", label: "Ctrl + Shift + V" },
  { value: "shiftInsert", label: "Shift + Insert" },
];

function ProfileCard({ profile, onChange, onRemove }: { profile: AppProfile; onChange: (next: AppProfile) => void; onRemove: () => void }) {
  const [instruction, setInstruction] = useState(profile.cleanupInstruction ?? "");
  const name = appName(profile.executable);
  return (
    <li class="app-card">
      <header class="app-card-head">
        <span class="app-badge" aria-hidden="true">
          {name.charAt(0)}
        </span>
        <div>
          <h3>{name}</h3>
          <span class="mono muted">{profile.executable}</span>
        </div>
        <IconButton label={`Remove ${name}`} icon={<Trash2 size={17} />} onClick={onRemove} />
      </header>
      <div class="app-card-grid">
        <label class="check-row">
          <Switch checked={profile.enhancementEnabled} label={`Polish with ChatGPT in ${name}`} onChange={(enhancementEnabled) => onChange({ ...profile, enhancementEnabled })} />
          <span>Polish with ChatGPT</span>
        </label>
        <label class="check-row">
          <Switch checked={profile.contextEnabled} label={`Use nearby text in ${name}`} onChange={(contextEnabled) => onChange({ ...profile, contextEnabled })} />
          <span>Use nearby text</span>
        </label>
        <div class="check-row">
          <span>Paste with</span>
          <Select label={`Paste shortcut for ${name}`} value={profile.pasteShortcut} options={PASTE} onChange={(pasteShortcut) => onChange({ ...profile, pasteShortcut })} />
        </div>
      </div>
      <label class="field">
        <span class="field-label">Style for this app</span>
        <textarea
          class="textarea"
          rows={2}
          value={instruction}
          placeholder="Casual and brief. No sign-off."
          onInput={(event) => setInstruction((event.currentTarget as HTMLTextAreaElement).value)}
          onBlur={() => {
            const next = instruction.trim() || null;
            if (next !== profile.cleanupInstruction) onChange({ ...profile, cleanupInstruction: next });
          }}
        />
        <span class="field-hint">Used when ChatGPT polishes text in {name}.</span>
      </label>
    </li>
  );
}

export function AppStyles() {
  const value = settings.value;
  const [executable, setExecutable] = useState("");
  const [error, setError] = useState<string | null>(null);
  if (!value) return null;
  const profiles = value.appProfiles;
  const save = (next: AppProfile[]) => void saveSettings({ appProfiles: next });
  const add = (name: string) => {
    const exe = name.trim();
    if (!/^[^\\/:*?"<>|]+\.exe$/i.test(exe)) {
      setError("Enter the app's program name, ending in .exe, like slack.exe.");
      return;
    }
    if (profiles.some((profile) => profile.executable.toLowerCase() === exe.toLowerCase())) {
      setError(`${appName(exe)} already has a style.`);
      return;
    }
    setError(null);
    setExecutable("");
    save([...profiles, { executable: exe, contextEnabled: value.contextEnabled, enhancementEnabled: value.enhancementEnabled, cleanupInstruction: null, pasteShortcut: "controlV" }]);
  };
  const unused = SUGGESTIONS.filter((exe) => !profiles.some((profile) => profile.executable.toLowerCase() === exe.toLowerCase()));
  return (
    <Page title="App styles" lede="Change how Onda behaves in specific apps, like a casual tone in chat or a different paste key in terminals.">
      <Section>
        <form
          class="add-app"
          onSubmit={(event) => {
            event.preventDefault();
            add(executable);
          }}
        >
          <label class="field grow">
            <span class="field-label">Add an app</span>
            <input class="input mono" value={executable} placeholder="slack.exe" aria-invalid={!!error} onInput={(event) => setExecutable((event.currentTarget as HTMLInputElement).value)} />
          </label>
          <Button type="submit" icon={<Plus size={16} />}>
            Add
          </Button>
        </form>
        {error && (
          <p class="field-error" role="alert">
            {error}
          </p>
        )}
        {unused.length > 0 && (
          <div class="suggestions" aria-label="Suggestions">
            {unused.map((exe) => (
              <button type="button" class="chip chip-button" key={exe} onClick={() => add(exe)}>
                <Plus size={13} aria-hidden="true" />
                {appName(exe)}
              </button>
            ))}
          </div>
        )}
      </Section>
      {profiles.length ? (
        <ul class="app-cards">
          {profiles.map((profile, index) => (
            <ProfileCard
              key={profile.executable}
              profile={profile}
              onChange={(next) => save(profiles.map((item, at) => (at === index ? next : item)))}
              onRemove={() => save(profiles.filter((_, at) => at !== index))}
            />
          ))}
        </ul>
      ) : (
        <EmptyState art={<AppWindow size={20} />} title="Every app uses your general settings">
          Add an app above when you want it to behave differently.
        </EmptyState>
      )}
    </Page>
  );
}
