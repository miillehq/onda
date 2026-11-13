import { BrainCircuit, Check, Cloud, Eraser, HardDrive, History as HistoryIcon, Minus, ScanText } from "lucide-preact";
import { useState } from "preact/hooks";
import { Button, Select, Switch } from "../../components/controls";
import { Dialog, Page, Row, Rows, Section } from "../../components/layout";
import { plural, providerName } from "../../lib/format";
import { navigate } from "../../lib/router";
import { aiReady, clearHistory, history, isMac, isWindows, saveSettings, settings, thisDevice, toast, yourDevice } from "../../lib/store";
import { retentionPhrase } from "../History";

const NEVER = ["Recordings of your voice", "Your words in logs or diagnostics", "Analytics or usage tracking"];
const RETENTION = [
  { value: "0", label: "Until I quit Onda" },
  { value: "1", label: "1 day" },
  { value: "3", label: "3 days" },
  { value: "7", label: "7 days" },
  { value: "14", label: "14 days" },
  { value: "30", label: "30 days" },
];

export function Privacy() {
  const value = settings.value;
  const [confirming, setConfirming] = useState(false);
  if (!value) return null;
  const kept = history.value?.length ?? 0;
  const days = String(value.historyDays);
  const local = [
    "Your microphone audio",
    "Speech recognition",
    `What you said, encrypted, ${retentionPhrase(value.historyDays)}`,
    "Your dictionary and snippets",
    "Daily word counts (numbers only)",
  ];
  return (
    <Page title="Privacy" lede={`Onda is built to keep what you say on ${yourDevice.value}.`}>
      <div class="privacy-grid">
        <section class="card privacy-card" aria-labelledby="privacy-local">
          <h3 id="privacy-local">
            <HardDrive size={18} aria-hidden="true" /> Stays on {thisDevice.value}
          </h3>
          <ul class="ticks">
            {local.map((item) => (
              <li key={item}>
                <Check size={16} aria-hidden="true" />
                {item}
              </li>
            ))}
          </ul>
        </section>
        <section class="card privacy-card" aria-labelledby="privacy-cloud">
          <h3 id="privacy-cloud">
            <Cloud size={18} aria-hidden="true" /> Sent only if you turn it on
          </h3>
          <ul class="ticks">
            <li>
              <Check size={16} aria-hidden="true" />
              {value.aiProvider === "local" ? "Written text, to your local model for AI polish" : `Written text, to ${providerName(value)} for AI polish`}{" "}
              {value.enhancementEnabled && aiReady.value ? "(on)" : "(off)"}
            </li>
            <li>
              <Check size={16} aria-hidden="true" />
              Nearby text, for context {value.contextEnabled ? "(on)" : "(off)"}
            </li>
          </ul>
          <h3 class="privacy-never">
            <Minus size={18} aria-hidden="true" /> Never
          </h3>
          <ul class="ticks is-never">
            {NEVER.map((item) => (
              <li key={item}>
                <Minus size={16} aria-hidden="true" />
                {item}
              </li>
            ))}
          </ul>
        </section>
      </div>

      <Section title="Your controls">
        <Rows>
          <Row
            icon={<BrainCircuit size={18} />}
            title="Learn from my corrections"
            description="For up to 12 seconds after Onda types, it checks that field for your fixes and suggests dictionary words. Only text Onda typed is compared."
            control={<Switch checked={value.learningEnabled} label="Learn from my corrections" onChange={(learningEnabled) => void saveSettings({ learningEnabled })} />}
          />
          <Row
            icon={<ScanText size={18} />}
            title="Use nearby text for context"
            description={value.aiProvider === "local" ? "Goes only to your local model, with AI polish on." : `Only sent to ${providerName(value)}, and only with AI polish on.`}
            control={
              <Button size="sm" variant="ghost" onClick={() => navigate("settings/ai")}>
                {value.contextEnabled ? "On" : "Off"} · Change
              </Button>
            }
          />
          <Row
            icon={<HistoryIcon size={18} />}
            title="Keep history"
            description={
              isWindows.value
                ? "So you can copy a dictation again if it didn't make it into your app. Encrypted with your Windows account and deleted automatically."
                : isMac.value
                  ? "So you can copy a dictation again if it didn't make it into your app. Encrypted with a key in your Mac's keychain and deleted automatically."
                  : "So you can copy a dictation again if it didn't make it into your app. Encrypted with a key in your desktop keyring and deleted automatically. Without a keyring, it's kept only until Onda quits."
            }
            control={
              <Select
                label="Keep history"
                value={days}
                options={RETENTION.some((option) => option.value === days) ? RETENTION : [...RETENTION, { value: days, label: `${days} days` }]}
                onChange={(next) => void saveSettings({ historyDays: Number(next) })}
              />
            }
          />
          <Row
            icon={<Eraser size={18} />}
            title="Clear history"
            description={kept ? `${plural(kept, "entry", "entries")} kept right now.` : "Nothing is kept right now."}
            control={
              <Button size="sm" disabled={!kept} onClick={() => setConfirming(true)}>
                Clear now
              </Button>
            }
          />
        </Rows>
      </Section>

      <Dialog
        open={confirming}
        title="Clear history?"
        description={`This deletes ${plural(kept, "entry", "entries")} from ${thisDevice.value}. You can't undo it.`}
        onClose={() => setConfirming(false)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
            <Button
              variant="danger"
              onClick={() =>
                void clearHistory().then((done) => {
                  setConfirming(false);
                  if (done) toast({ tone: "ok", title: "History cleared" }, 2400);
                })
              }
            >
              Clear history
            </Button>
          </>
        }
      />
    </Page>
  );
}
