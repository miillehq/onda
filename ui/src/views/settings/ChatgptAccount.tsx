import { LogOut, Sparkles, Trash2 } from "lucide-preact";
import { ask } from "../../api";
import { useCloudModels } from "../../lib/useCloudModels";
import { Button, Chip, Select } from "../../components/controls";
import { Row } from "../../components/layout";
import type { AccountInfo, CloudModel } from "../../generated/api";
import { activeProfile, chatgptReady, refresh, thisDevice, toast, toastError } from "../../lib/store";

export async function performAccountAction(label: string, request: Parameters<typeof ask>[0]): Promise<void> {
  try {
    await ask(request, "unit");
    await refresh();
  } catch (error) {
    toastError(error, label);
  }
}

export function AccountRow({ account }: { account: AccountInfo }) {
  const signOut = async () => {
    try {
      const result = await ask({ command: "signOutChatgptProfile", args: { profileId: account.id } }, "signOut");
      await refresh();
      if (result.remoteRevocationConfirmed) toast({ tone: "ok", title: "Signed out" });
      else toast({ tone: "info", title: `Signed out on ${thisDevice.value}`, body: "ChatGPT didn't confirm it removed access. You can remove Onda in your ChatGPT account settings." }, 9000);
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
          <Button size="sm" variant="primary" onClick={() => void performAccountAction("Couldn't reconnect", { command: "startChatgptSignIn", args: { profileId: account.id } })}>
            Reconnect
          </Button>
        ) : (
          !account.planUsageEnabled && (
            <Button size="sm" variant="primary" onClick={() => void performAccountAction("Couldn't ask for permission", { command: "enableChatgptPlanUsage", args: { profileId: account.id } })}>
              Allow plan usage
            </Button>
          )
        )}
        {!account.active && account.signedIn && (
          <Button size="sm" onClick={() => void performAccountAction("Couldn't switch account", { command: "selectChatgptProfile", args: { profileId: account.id } })}>
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
        void performAccountAction("Model not changed", { command: "setChatgptModel", args: { profileId: profile.id, slug } });
      }}
    />
  );
}

export function ModelPicker() {
  const { models } = useCloudModels();
  if (!activeProfile.value || !chatgptReady.value) return null;
  return <Row icon={<Sparkles size={18} />} title="ChatGPT model" description={modelAdvice(models)} control={<ChatgptModelSelect />} />;
}
