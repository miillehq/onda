import { CircleCheck, Mic, Timer, TriangleAlert, Volume1 } from "lucide-preact";
import { useEffect, useState } from "preact/hooks";
import { Button, Select } from "../../components/controls";
import { LevelBars } from "../../components/feedback";
import { Callout, Page, Row, Rows, Section } from "../../components/layout";
import type { MicrophoneReport } from "../../generated/api";
import { micVerdict, micWording } from "../../lib/microphone";
import { devices, isBusy, loadDevices, micLevel, platform, saveSettings, settings, testMicrophone, toastError } from "../../lib/store";

const LIMITS = [
  { value: "60", label: "1 minute" },
  { value: "180", label: "3 minutes" },
  { value: "300", label: "5 minutes" },
  { value: "600", label: "10 minutes" },
];

export function MicTest({ compact = false }: { compact?: boolean }) {
  const [testing, setTesting] = useState(false);
  const [report, setReport] = useState<MicrophoneReport | null>(null);
  const run = async () => {
    setReport(null);
    setTesting(true);
    setReport(await testMicrophone(3000));
    setTesting(false);
  };
  const result = report ? micVerdict(report, platform.value) : null;
  return (
    <div class={`mic-test${compact ? " is-compact" : ""}`}>
      <LevelBars level={micLevel} active={testing} bars={compact ? 20 : 28} />
      <div class="mic-test-foot">
        <Button variant={report ? "secondary" : "primary"} icon={<Mic size={16} />} disabled={testing || isBusy.value} aria-live="polite" onClick={() => void run()}>
          {testing ? "Listening… say a few words" : report ? "Test again" : "Test microphone"}
        </Button>
        {result && (
          <p class={`mic-verdict tone-${result.tone}`} role="status">
            {result.tone === "ok" ? <CircleCheck size={17} /> : <TriangleAlert size={17} />}
            <span>
              <b>{result.title}.</b> {result.body}
            </span>
          </p>
        )}
      </div>
    </div>
  );
}

export function Microphone() {
  const value = settings.value;
  useEffect(() => {
    void loadDevices().catch((error) => toastError(error, "Couldn't list microphones"));
  }, []);
  if (!value) return null;
  const list = devices.value ?? [];
  const selectedMissing = value.microphoneId !== null && devices.value !== null && !list.some((device) => device.id === value.microphoneId);
  const defaultName = list.find((device) => device.isDefault)?.name;
  const wording = micWording(platform.value);
  return (
    <Page title="Microphone" lede="Choose the mic Onda listens to, and check that it hears you well.">
      {selectedMissing && (
        <Callout tone="warn" icon={<TriangleAlert size={20} />} title="Your chosen microphone isn't connected">
          Onda won't switch mics on its own. Plug it back in, or choose another one below.
        </Callout>
      )}
      <Section>
        <Rows>
          <Row
            icon={<Mic size={18} />}
            title="Microphone"
            description={value.microphoneId === null ? `Follows ${wording.system}${defaultName ? `: currently ${defaultName}` : ""}.` : "Onda always uses this mic."}
            control={
              <Select
                label="Microphone"
                wide
                value={value.microphoneId ?? ""}
                onChange={(id) => void saveSettings({ microphoneId: id || null })}
                options={[
                  { value: "", label: wording.defaultLabel },
                  ...list.map((device) => ({ value: device.id, label: device.name })),
                  ...(selectedMissing && value.microphoneId ? [{ value: value.microphoneId, label: "Disconnected microphone" }] : []),
                ]}
              />
            }
          />
        </Rows>
      </Section>
      <Section title="Check your level" description="Speak normally for three seconds.">
        <div class="card">
          <MicTest />
        </div>
      </Section>
      <Section>
        <Rows>
          <Row
            icon={<Timer size={18} />}
            title="Longest recording"
            description="Onda stops listening after this and writes out what you said."
            control={<Select label="Longest recording" value={String(value.maxRecordingSeconds)} onChange={(seconds) => void saveSettings({ maxRecordingSeconds: Number(seconds) })} options={LIMITS.some((limit) => limit.value === String(value.maxRecordingSeconds)) ? LIMITS : [...LIMITS, { value: String(value.maxRecordingSeconds), label: `${value.maxRecordingSeconds} seconds` }]} />}
          />
          <Row icon={<Volume1 size={18} />} title="Input volume" description={wording.volumeHint} />
        </Rows>
      </Section>
    </Page>
  );
}
