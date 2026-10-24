import { ArrowRight, AudioLines, ExternalLink, FileText, LogOut, Plus, ScanText, Sparkles, TriangleAlert, UserRound } from "lucide-preact";
import { useEffect, useState } from "preact/hooks";
import { ask } from "../../api";
import { Button, Chip, Select, Switch } from "../../components/controls";
import { Callout, Page, Row, Rows, Section } from "../../components/layout";
import type { AccountInfo } from "../../generated/api";
import { activeProfile, aiReady, cloudModels, oauthStatus, refresh, saveSettings, settings, snapshot, toast, toastError } from "../../lib/store";

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
        <Button size="sm" variant="ghost" icon={<LogOut size={15} />} onClick={() => void signOut()}>
          Sign out
        </Button>
      </div>
    </div>
  );
}

function ModelPicker() {
  const profile = activeProfile.value;
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    if (!aiReady.value || cloudModels.value) return;
    setLoading(true);
    void ask({ command: "listAvailableModels" }, "cloudModels")
      .then((models) => (cloudModels.value = models))
      .catch(() => (cloudModels.value = []))
      .finally(() => setLoading(false));
  }, [aiReady.value]);
  if (!profile || !aiReady.value) return null;
  const models = cloudModels.value ?? [];
  const options = [...(profile.preferredModel ? [] : [{ value: "", label: loading ? "Loading models…" : "Recommended" }]), ...models.map((model) => ({ value: model.slug, label: model.name }))];
  return (
    <Row
      icon={<Sparkles size={18} />}
      title="ChatGPT model"
      description="Models come from your plan. Faster models keep dictation snappy."
      control={
        <Select
          label="ChatGPT model"
          value={profile.preferredModel ?? ""}
          options={options}
          onChange={(slug) => {
            if (!slug) return;
            void act("Model not changed", { command: "setChatgptModel", args: { profileId: profile.id, slug } });
          }}
        />
      }
    />
  );
}

export function AiPolish() {
  const value = settings.value;
  if (!value) return null;
  const profiles = snapshot.value?.profiles ?? [];
  const cloudError = snapshot.value?.cloudError;
  const authorizing = oauthStatus.value === "authorizing";
  return (
    <Page title="AI polish" lede="Optionally let ChatGPT tidy punctuation, filler words and formatting, using your own ChatGPT plan.">
      <div class="sent-diagram" aria-label="What is sent to ChatGPT">
        <div class="sent-box is-local">
          <AudioLines size={18} aria-hidden="true" />
          <span>
            <b>Your audio</b>
            <span class="flow-sub">Stays on this PC</span>
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
        <div class="sent-box is-cloud">
          <Sparkles size={18} aria-hidden="true" />
          <span>
            <b>ChatGPT</b>
            <span class="flow-sub">Returns polished text</span>
          </span>
        </div>
      </div>

      {cloudError && (
        <Callout tone="error" icon={<TriangleAlert size={20} />} title="ChatGPT isn't available on this PC">
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

      <Section title="Polish">
        <Rows>
          <Row
            icon={<Sparkles size={18} />}
            title="Polish my dictation"
            description={aiReady.value ? "After Onda writes out your words, ChatGPT fixes punctuation, filler words and formatting." : "Connect ChatGPT above to turn this on."}
            control={<Switch checked={value.enhancementEnabled} disabled={!aiReady.value && !value.enhancementEnabled} label="Polish my dictation" onChange={(enhancementEnabled) => void saveSettings({ enhancementEnabled })} />}
          />
          <Row
            icon={<ScanText size={18} />}
            title="Use nearby text for context"
            description="Onda reads a little text around your cursor so ChatGPT can match names and tone. Only sent with polish on."
            control={<Switch checked={value.contextEnabled} label="Use nearby text for context" onChange={(contextEnabled) => void saveSettings({ contextEnabled })} />}
          />
          <ModelPicker />
        </Rows>
        <p class="footnote">If ChatGPT is slow or unavailable, Onda types your original words instead, so dictation never gets stuck.</p>
      </Section>
    </Page>
  );
}
