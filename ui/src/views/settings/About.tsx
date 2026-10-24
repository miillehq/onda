import { Download, RefreshCw, RotateCcw } from "lucide-preact";
import { useState } from "preact/hooks";
import iconUrl from "../../assets/onda-icon-dark.svg";
import { ask } from "../../api";
import { Button, Keys } from "../../components/controls";
import { Page, Row, Rows, Section } from "../../components/layout";
import type { UpdateInfo } from "../../generated/api";
import { navigate } from "../../lib/router";
import { settings, snapshot, toastError } from "../../lib/store";

export function About() {
  const value = settings.value;
  const info = snapshot.value;
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  if (!value || !info) return null;
  const check = async () => {
    setChecking(true);
    try {
      setUpdate(await ask({ command: "checkUpdate" }, "update"));
    } catch (error) {
      toastError(error, "Couldn't check for updates");
    } finally {
      setChecking(false);
    }
  };
  const install = async () => {
    setInstalling(true);
    try {
      await ask({ command: "installUpdate" }, "unit");
    } catch (error) {
      setInstalling(false);
      toastError(error, "Update not installed");
    }
  };
  const cheats: { keys: number[]; what: string }[] = [
    { keys: value.pushToTalk.keys, what: "Hold to dictate" },
    ...(value.toggleEnabled ? [{ keys: value.toggleShortcut.keys, what: "Hands-free on and off" }] : []),
    { keys: value.transformShortcut.keys, what: "Rewrite selected text" },
    { keys: value.commandShortcut.keys, what: "Edit selected text with your voice" },
    { keys: [0x1b], what: "Cancel" },
    { keys: [0xa2, 0x4b], what: "Search Onda (in the app)" },
  ];
  return (
    <Page title="Updates and help">
      <div class="about">
        <img src={iconUrl} alt="" width={84} height={84} />
        <div>
          <h2>{info.appName}</h2>
          <p class="muted">
            Version <span class="tabular">{info.version}</span> · by miille
          </p>
        </div>
      </div>

      <Section title="Updates">
        <Rows>
          <Row
            icon={<RefreshCw size={18} />}
            title={update ? (update.available ? `Version ${update.version} is available` : "You're up to date") : "Check for updates"}
            description={info.updaterConfigured ? "Updates are signed and verified before they install." : "This build doesn't include automatic updates."}
            control={
              update?.available ? (
                <Button variant="primary" icon={<Download size={16} />} busy={installing} onClick={() => void install()}>
                  Install and restart
                </Button>
              ) : (
                <Button size="sm" busy={checking} disabled={!info.updaterConfigured} onClick={() => void check()}>
                  Check now
                </Button>
              )
            }
          />
        </Rows>
      </Section>

      <Section title="Shortcuts at a glance">
        <ul class="cheats">
          {cheats.map((cheat) => (
            <li key={cheat.what}>
              <span>{cheat.what}</span>
              <Keys keys={cheat.keys} size="md" />
            </li>
          ))}
        </ul>
      </Section>

      <Section title="Help">
        <Rows>
          <Row
            icon={<RotateCcw size={18} />}
            title="Run setup again"
            description="Check your microphone, model and shortcut step by step."
            control={
              <Button size="sm" onClick={() => navigate("onboarding")}>
                Run setup
              </Button>
            }
          />
        </Rows>
        <p class="footnote">Onda is open source under the Apache 2.0 licence. Speech recognition uses whisper.cpp and OpenAI Whisper models (MIT).</p>
      </Section>
    </Page>
  );
}
