import { ArrowRight, AudioLines, Cloud, Computer, ExternalLink, FileText, KeyRound, LogOut, Plus, RefreshCw, ScanText, Server, Sparkles, Trash2, TriangleAlert, UserRound } from "lucide-preact";
import { useEffect, useState } from "preact/hooks";
import { ask } from "../../api";
import { Button, Chip, Select, Switch } from "../../components/controls";
import { Callout, Card, Page, Row, Rows, Section } from "../../components/layout";
import type { AccountInfo, CloudModel } from "../../generated/api";
import { toAppError } from "../../api";
import type { AiProvider, Settings } from "../../generated/core";
import { activeProfile, aiReady, chatgptReady, cloudModels, isWindows, listProviderModels, oauthStatus, refresh, removeProviderKey, saveProviderKey, saveSettings, settings, snapshot, toast, toastError } from "../../lib/store";

async function act(label: string, request: Parameters<typeof ask>[0]): Promise<void> {
  try {
    await ask(request, "unit");
    await refresh();
  } catch (error) {
    toastError(error, label);
  }
}

function AccountRow({ account }: { account: AccountInfo }) {
  const signOut = async () => {
    try {
      const result = await ask({ command: "signOutChatgptProfile", args: { profileId: account.id } }, "signOut");
      await refresh();
      if (result.remoteRevocationConfirmed) toast({ tone: "ok", title: "Signed out" });
      else toast({ tone: "info", title: "Signed out on this PC", body: "ChatGPT didn't confirm it removed access. You can remove Onda in your ChatGPT account settings." }, 9000);
    } catch (error) {
      toastError(error, "Couldn't sign out");
    }
  };
  const remove = async () => {
    try {
      await ask({ command: "removeChatgptProfile", args: { profileId: account.id } }, "signOut");
      await refresh();
      toast({ tone: "ok", title: `Removed ${account.label || "account"}` });
    } catch (error) {
      toastError(error, "Couldn't remove account");
    }
  };
  const initial = (account.label || account.email || "?").charAt(0).toUpperCase();
  return (
    <div class="row account">
      <span class="avatar" aria-hidden="true">
        {initial}
      </span>
      <div class="row-text">
        <div class="row-title">
          {account.label}
          {account.active && <Chip tone="strong">In use</Chip>}
          {!account.signedIn && <Chip tone="warn">Signed out</Chip>}
          {account.signedIn && !account.planUsageEnabled && <Chip tone="warn">Needs permission</Chip>}
        </div>
        {account.email && <div class="row-description selectable">{account.email}</div>}
      </div>
      <div class="row-control button-row">
        {!account.signedIn ? (
          <Button size="sm" variant="primary" onClick={() => void act("Couldn't reconnect", { command: "startChatgptSignIn", args: { profileId: account.id } })}>
            Reconnect
          </Button>
        ) : (
          !account.planUsageEnabled && (
            <Button size="sm" variant="primary" onClick={() => void act("Couldn't ask for permission", { command: "enableChatgptPlanUsage", args: { profileId: account.id } })}>
              Allow plan usage
            </Button>
          )
        )}
        {!account.active && account.signedIn && (
          <Button size="sm" onClick={() => void act("Couldn't switch account", { command: "selectChatgptProfile", args: { profileId: account.id } })}>
            Use this account
          </Button>
        )}
        {account.signedIn ? (
          <Button size="sm" variant="ghost" icon={<LogOut size={15} />} onClick={() => void signOut()}>
            Sign out
          </Button>
        ) : (
          <Button size="sm" variant="ghost" icon={<Trash2 size={15} />} onClick={() => void remove()} aria-label={`Remove ${account.label || "account"} from Onda`}>
            Remove
          </Button>
        )}
      </div>
    </div>
  );
}

/** The account's models, fetched once per window; the light one Onda uses by default is marked. */
function useCloudModels(): { models: CloudModel[]; loading: boolean } {
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    if (!chatgptReady.value || cloudModels.value) return;
    setLoading(true);
    void ask({ command: "listAvailableModels" }, "cloudModels")
      .then((models) => (cloudModels.value = models))
      .catch(() => (cloudModels.value = []))
      .finally(() => setLoading(false));
  }, [chatgptReady.value]);
  return { models: cloudModels.value ?? [], loading };
}

/** Why a light model is the right default, naming the one this account offers. */
export function modelAdvice(models: CloudModel[]): string {
  const light = models.find((model) => model.recommended);
  return light
    ? `${light.name} is light and quick, which is all polish needs. Bigger models take longer and can use up your plan's limits faster.`
    : "Lighter models are quick, which is all polish needs. Bigger models take longer and can use up your plan's limits faster.";
}

/** Choose the ChatGPT model; "Recommended" names the model Onda would use. */
export function ChatgptModelSelect() {
  const profile = activeProfile.value;
  const { models, loading } = useCloudModels();
  if (!profile || !chatgptReady.value) return null;
  const recommended = models.find((model) => model.recommended);
  const options = [
    ...(profile.preferredModel ? [] : [{ value: "", label: loading ? "Loading models…" : recommended ? `Recommended (${recommended.name})` : "Recommended" }]),
    ...models.map((model) => ({ value: model.slug, label: model.recommended && profile.preferredModel ? `${model.name} (recommended)` : model.name })),
  ];
  return (
    <Select
      label="ChatGPT model"
      value={profile.preferredModel ?? ""}
      options={options}
      onChange={(slug) => {
        if (!slug) return;
        void act("Model not changed", { command: "setChatgptModel", args: { profileId: profile.id, slug } });
      }}
    />
  );
}

function ModelPicker() {
  const { models } = useCloudModels();
  if (!activeProfile.value || !chatgptReady.value) return null;
  return <Row icon={<Sparkles size={18} />} title="ChatGPT model" description={modelAdvice(models)} control={<ChatgptModelSelect />} />;
}

const PROVIDERS: { id: AiProvider; name: string; detail: string; icon: preact.ComponentChildren }[] = [
  { id: "chatgpt", name: "ChatGPT", detail: "Sign in and use your ChatGPT plan", icon: <Sparkles size={18} /> },
  { id: "openaiCompatible", name: "OpenAI-compatible", detail: "A service or server you choose, with your key", icon: <Server size={18} /> },
  { id: "anthropic", name: "Anthropic", detail: "Claude models, with your Anthropic API key", icon: <Cloud size={18} /> },
  { id: "local", name: "On this computer", detail: "Ollama, LM Studio or llama.cpp; nothing leaves it", icon: <Computer size={18} /> },
];

/** Where a local server usually listens. llama.cpp moved its default port to 9931 in 2026. */
const LOCAL_PRESETS = [
  { name: "Ollama", url: "http://127.0.0.1:11434/v1" },
  { name: "LM Studio", url: "http://127.0.0.1:1234/v1" },
  { name: "llama.cpp", url: "http://127.0.0.1:9931/v1" },
];

function hostOf(url: string | null): string | null {
  if (!url) return null;
  try {
    return new URL(url).host;
  } catch {
    return null;
  }
}

/** Who receives the text, for the diagram and descriptions. */
function destination(value: Settings): { name: string; detail: string; local: boolean } {
  switch (value.aiProvider) {
    case "openaiCompatible":
      return { name: hostOf(value.openaiBaseUrl) ?? "Your server", detail: "Returns polished text", local: false };
    case "anthropic":
      return { name: "Anthropic", detail: "api.anthropic.com", local: false };
    case "local":
      return { name: "Local model", detail: hostOf(value.localBaseUrl) ?? "On this computer", local: true };
    default:
      return { name: "ChatGPT", detail: "Returns polished text", local: false };
  }
}

function ProviderPicker({ value }: { value: AiProvider }) {
  return (
    <div class="provider-picker" role="radiogroup" aria-label="AI provider">
      {PROVIDERS.map((provider) => (
        <button type="button" role="radio" key={provider.id} aria-checked={value === provider.id} class={`provider-option${value === provider.id ? " is-selected" : ""}`} onClick={() => void saveSettings({ aiProvider: provider.id })}>
          <span class="provider-icon" aria-hidden="true">
            {provider.icon}
          </span>
          <span>
            <b>{provider.name}</b>
            <span class="flow-sub">{provider.detail}</span>
          </span>
        </button>
      ))}
    </div>
  );
}

/** An address field that saves when it loses focus; native code checks the address policy. */
function AddressField({ id, label, hint, value, placeholder, onSave }: { id: string; label: string; hint: string; value: string | null; placeholder: string; onSave: (value: string | null) => void }) {
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
function KeyField({ provider, saved, required }: { provider: AiProvider; saved: boolean; required: boolean }) {
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const store = isWindows.value ? "protected for your Windows account" : "in your desktop keyring";
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

/** Model name with suggestions from the server; checking also confirms the connection. */
function ModelField({ provider, value, field, ready, hint }: { provider: AiProvider; value: string | null; field: "openaiModel" | "anthropicModel" | "localModel"; ready: boolean; hint: string }) {
  const [models, setModels] = useState<CloudModel[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [text, setText] = useState(value ?? "");
  useEffect(() => setText(value ?? ""), [value]);
  const check = async () => {
    setChecking(true);
    setError(null);
    try {
      setModels(await listProviderModels(provider));
    } catch (failure) {
      setModels(null);
      setError(toAppError(failure).message);
    } finally {
      setChecking(false);
    }
  };
  useEffect(() => {
    if (ready) void check();
    else setModels(null);
  }, [provider, ready]);
  const listId = `models-${provider}`;
  return (
    <div class="provider-field">
      <label class="field-label" for={`model-${provider}`}>
        Model
      </label>
      <div class="key-entry">
        <input
          id={`model-${provider}`}
          class="input mono"
          list={listId}
          spellcheck={false}
          autoComplete="off"
          placeholder={models?.length ? "Choose or type a model" : "Model name"}
          value={text}
          onInput={(event) => setText(event.currentTarget.value)}
          onChange={(event) => {
            const next = event.currentTarget.value.trim() || null;
            if (next !== value) void saveSettings({ [field]: next });
          }}
        />
        <datalist id={listId}>
          {models?.map((model) => (
            <option key={model.slug} value={model.slug}>
              {model.name}
            </option>
          ))}
        </datalist>
        <Button size="sm" variant="ghost" icon={<RefreshCw size={15} />} busy={checking} disabled={!ready} onClick={() => void check()}>
          Check connection
        </Button>
      </div>
      {error ? (
        <p class="field-hint is-error" role="alert">
          {error}
        </p>
      ) : models ? (
        <p class="field-hint is-ok" role="status">
          Connected · {models.length === 1 ? "1 model" : `${models.length} models`} available
        </p>
      ) : null}
      <p class="field-hint">{hint}</p>
    </div>
  );
}

function ChatgptSetup({ cloudError }: { cloudError: { message: string } | null | undefined }) {
  const profiles = snapshot.value?.profiles ?? [];
  const authorizing = oauthStatus.value === "authorizing";
  return (
    <>
      {cloudError && (
        <Callout tone="error" icon={<TriangleAlert size={20} />} title={`ChatGPT isn't available on this ${isWindows.value ? "PC" : "computer"}`}>
          {cloudError.message}
        </Callout>
      )}
      <Section title="Account">
        {profiles.length ? (
          <Rows>
            {profiles.map((account) => (
              <AccountRow key={account.id} account={account} />
            ))}
          </Rows>
        ) : (
          <div class="card connect">
            <span class="connect-icon" aria-hidden="true">
              <UserRound size={22} />
            </span>
            <div>
              <h3>Connect your ChatGPT plan</h3>
              <p class="muted">You'll sign in on chatgpt.com in your browser. Onda never sees your password.</p>
            </div>
          </div>
        )}
        <div class="button-row section-actions">
          {authorizing ? (
            <>
              <span class="muted">
                <ExternalLink size={15} aria-hidden="true" /> Finish signing in in your browser…
              </span>
              <Button variant="ghost" onClick={() => void act("Couldn't cancel", { command: "cancelChatgptSignIn" })}>
                Cancel
              </Button>
            </>
          ) : (
            <Button variant={profiles.length ? "secondary" : "primary"} icon={profiles.length ? <Plus size={16} /> : undefined} disabled={!!cloudError} onClick={() => void act("Sign-in didn't start", { command: "startChatgptSignIn", args: { profileId: null } })}>
              {profiles.length ? "Add another account" : "Continue with ChatGPT"}
            </Button>
          )}
        </div>
      </Section>
    </>
  );
}

function OpenaiSetup({ value }: { value: Settings }) {
  const keys = snapshot.value?.providerKeys;
  return (
    <Section title="Server" description="Any service or server that speaks OpenAI's Chat Completions API. Your text is sent to the address below.">
      <Card class="provider-card">
        <AddressField
          id="openai-url"
          label="Server address"
          hint="Usually ends in /v1. HTTPS is required, except for an address on this computer."
          value={value.openaiBaseUrl}
          placeholder="https://api.example.com/v1"
          onSave={(openaiBaseUrl) => void saveSettings({ openaiBaseUrl })}
        />
        <KeyField provider="openaiCompatible" saved={!!keys?.openaiCompatible} required={false} />
        <ModelField provider="openaiCompatible" field="openaiModel" value={value.openaiModel} ready={!!value.openaiBaseUrl} hint="Suggestions come from the server's model list; you can also type a name it accepts." />
      </Card>
    </Section>
  );
}

function AnthropicSetup({ value }: { value: Settings }) {
  const saved = !!snapshot.value?.providerKeys.anthropic;
  return (
    <Section title="Anthropic" description="Onda talks to Anthropic's own Messages API at api.anthropic.com. Each polish is billed to your Anthropic account.">
      <Card class="provider-card">
        <KeyField provider="anthropic" saved={saved} required />
        <ModelField provider="anthropic" field="anthropicModel" value={value.anthropicModel} ready={saved} hint="Claude Opus 5.5 is Anthropic's default; lighter models answer sooner and cost less. Models that support it are asked for low effort, which suits clean-up." />
      </Card>
    </Section>
  );
}

function LocalSetup({ value }: { value: Settings }) {
  const keys = snapshot.value?.providerKeys;
  return (
    <>
      <Section title="Local server" description="Onda talks to a model server running on this computer. Only addresses on this computer are allowed, so your text never leaves it.">
        <Card class="provider-card">
          <div class="provider-field">
            <span class="field-label">Server</span>
            <div class="button-row">
              {LOCAL_PRESETS.map((preset) => (
                <Button key={preset.name} size="sm" variant={value.localBaseUrl === preset.url ? "primary" : "secondary"} onClick={() => void saveSettings({ localBaseUrl: preset.url })}>
                  {preset.name}
                </Button>
              ))}
            </div>
          </div>
          <AddressField id="local-url" label="Address" hint="127.0.0.1, ::1 or localhost only. Older llama.cpp builds listen on port 8080." value={value.localBaseUrl} placeholder="http://127.0.0.1:11434/v1" onSave={(localBaseUrl) => void saveSettings({ localBaseUrl })} />
          <ModelField provider="local" field="localModel" value={value.localModel} ready={!!value.localBaseUrl} hint="Onda asks the model not to think out loud, which keeps small models quick." />
          <KeyField provider="local" saved={!!keys?.local} required={false} />
        </Card>
      </Section>
      <Section title="Which model for this computer" description="Polish is short work, so small models do it well. Speeds are rough; the first request after starting the server also loads the model.">
        <div class="table-wrap">
          <table class="plain-table">
            <thead>
              <tr>
                <th scope="col">Computer</th>
                <th scope="col">Try</th>
                <th scope="col">Memory</th>
                <th scope="col">What to expect</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>CPU only, 8 GB</td>
                <td class="mono">Qwen3.5 0.8B (Q4_K_M)</td>
                <td>About 1 GB</td>
                <td>2–4 s for a sentence on 3 cores of a 2013-era CPU (measured); light clean-up only</td>
              </tr>
              <tr>
                <td>Recent laptop, 16 GB</td>
                <td class="mono">Qwen3.5 2B (Q4_K_M)</td>
                <td>About 2 GB</td>
                <td>Better wording; integrated graphics help through Vulkan builds</td>
              </tr>
              <tr>
                <td>Dedicated GPU, 8 GB+</td>
                <td class="mono">Qwen3.5 4B (Q4_K_M)</td>
                <td>About 3–4 GB</td>
                <td>Fast on the GPU; turns thinking off in the template</td>
              </tr>
            </tbody>
          </table>
        </div>
        <p class="footnote">Qwen3.5 models are Apache-2.0 and run in Ollama, LM Studio and llama.cpp. Onda doesn't download or run them itself. Only the CPU figure was measured; GPU speeds are untested.</p>
      </Section>
    </>
  );
}

export function AiPolish() {
  const value = settings.value;
  if (!value) return null;
  const cloudError = snapshot.value?.cloudError;
  const target = destination(value);
  const ready = aiReady.value;
  return (
    <Page title="AI polish" lede="Optionally let an AI model tidy punctuation, filler words and formatting: ChatGPT, a service you choose, Anthropic, or a model on this computer.">
      <div class="sent-diagram" aria-label={target.local ? "Your text stays on this computer" : `What is sent to ${target.name}`}>
        <div class="sent-box is-local">
          <AudioLines size={18} aria-hidden="true" />
          <span>
            <b>Your audio</b>
            <span class="flow-sub">Stays on this {isWindows.value ? "PC" : "computer"}</span>
          </span>
        </div>
        <div class="sent-box">
          <FileText size={18} aria-hidden="true" />
          <span>
            <b>Written text</b>
            <span class="flow-sub">{value.contextEnabled ? "Plus a little nearby text" : "Only what you said"}</span>
          </span>
        </div>
        <ArrowRight size={18} class="sent-arrow" aria-hidden="true" />
        <div class={`sent-box ${target.local ? "is-local" : "is-cloud"}`}>
          {target.local ? <Computer size={18} aria-hidden="true" /> : <Sparkles size={18} aria-hidden="true" />}
          <span>
            <b>{target.name}</b>
            <span class="flow-sub">{target.local ? `${target.detail} · never leaves it` : target.detail}</span>
          </span>
        </div>
      </div>

      <Section title="Provider" description="Only the provider you choose is ever used. If it fails, Onda types your original words; it never tries another provider.">
        <ProviderPicker value={value.aiProvider} />
      </Section>

      {value.aiProvider === "chatgpt" && <ChatgptSetup cloudError={cloudError} />}
      {value.aiProvider === "openaiCompatible" && <OpenaiSetup value={value} />}
      {value.aiProvider === "anthropic" && <AnthropicSetup value={value} />}
      {value.aiProvider === "local" && <LocalSetup value={value} />}

      <Section title="Polish">
        <Rows>
          <Row
            icon={<Sparkles size={18} />}
            title="Polish my dictation"
            description={ready ? `After Onda writes out your words, ${target.local ? "the local model" : target.name} fixes punctuation, filler words and formatting.` : "Finish setting up the provider above to turn this on."}
            control={<Switch checked={value.enhancementEnabled} disabled={!ready && !value.enhancementEnabled} label="Polish my dictation" onChange={(enhancementEnabled) => void saveSettings({ enhancementEnabled })} />}
          />
          <Row
            icon={<ScanText size={18} />}
            title="Use nearby text for context"
            description={`Onda reads a little text around your cursor so the model can match names and tone. Only sent with polish on${isWindows.value ? "" : ", and not available on Linux yet"}.`}
            control={<Switch checked={value.contextEnabled} label="Use nearby text for context" onChange={(contextEnabled) => void saveSettings({ contextEnabled })} />}
          />
          {value.aiProvider === "chatgpt" && <ModelPicker />}
        </Rows>
        <p class="footnote">If the model is slow or unavailable, Onda types your original words instead, so dictation never gets stuck.</p>
      </Section>
    </Page>
  );
}
