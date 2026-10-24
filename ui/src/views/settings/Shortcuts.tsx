import { Ban, CircleCheck, Hand, Mic, Repeat, TriangleAlert, WandSparkles } from "lucide-preact";
import { Keys, Switch } from "../../components/controls";
import { Callout, Page, Row, Rows, Section } from "../../components/layout";
import { ShortcutRecorder } from "../../components/ShortcutRecorder";
import type { Settings } from "../../generated/core";
import { saveSettings, settings, snapshot } from "../../lib/store";

function save(patch: Partial<Settings>) {
  return saveSettings(patch, { quiet: true });
}

export function Shortcuts() {
  const value = settings.value;
  if (!value) return null;
  const status = snapshot.value?.shortcutStatus ?? "starting";
  return (
    <Page title="Shortcuts" lede="Onda listens for these anywhere in Windows. Esc always cancels.">
      {status === "unavailable" ? (
        <Callout tone="error" icon={<TriangleAlert size={20} />} title="Shortcuts aren't working right now">
          The shortcut helper couldn't start. Restart Onda; if it keeps happening, reinstall it.
        </Callout>
      ) : status === "recovering" ? (
        <Callout tone="warn" icon={<TriangleAlert size={20} />} title="Reconnecting shortcuts">
          This can happen after waking from sleep or switching desktops. It usually takes a moment.
        </Callout>
      ) : (
        <p class="status-line">
          <CircleCheck size={16} aria-hidden="true" /> Shortcuts are active
        </p>
      )}

      <Section title="Dictation">
        <Rows>
          <Row icon={<Hand size={18} />} title="Hold to talk" description="Hold the keys while you speak. Let go to type.">
            <ShortcutRecorder value={value.pushToTalk.keys} label="Hold to talk" onSave={(keys) => save({ pushToTalk: { keys } })} />
          </Row>
          <Row
            icon={<Repeat size={18} />}
            title="Hands-free"
            description="Press once to start, again to finish. Good for longer thoughts."
            control={<Switch checked={value.toggleEnabled} label="Hands-free shortcut" onChange={(toggleEnabled) => void saveSettings({ toggleEnabled })} />}
          >
            {value.toggleEnabled && <ShortcutRecorder value={value.toggleShortcut.keys} label="Hands-free" onSave={(keys) => save({ toggleShortcut: { keys } })} />}
          </Row>
          <Row icon={<Ban size={18} />} title="Cancel" description="Stops listening and discards what you said." control={<Keys keys={[0x1b]} size="md" label="Escape" />} />
        </Rows>
      </Section>

      <Section title="Editing with ChatGPT" description="These need AI polish to be connected.">
        <Rows>
          <Row icon={<WandSparkles size={18} />} title="Rewrite selected text" description="Applies your default rewrite to the selection.">
            <ShortcutRecorder value={value.transformShortcut.keys} label="Rewrite selected text" onSave={(keys) => save({ transformShortcut: { keys } })} />
          </Row>
          <Row icon={<Mic size={18} />} title="Edit with your voice" description="Select text, hold the keys and say what to change.">
            <ShortcutRecorder value={value.commandShortcut.keys} label="Edit with your voice" onSave={(keys) => save({ commandShortcut: { keys } })} />
          </Row>
        </Rows>
      </Section>
      <p class="footnote">Shortcuts that use the Windows key can also trigger Windows features, such as switching keyboard layouts.</p>
    </Page>
  );
}
