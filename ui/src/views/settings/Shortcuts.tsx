import { Ban, CircleCheck, ClipboardPaste, Hand, Hourglass, Mic, Repeat, TriangleAlert, WandSparkles } from "lucide-preact";
import { useEffect, useState } from "preact/hooks";
import { Button, Keys, Switch } from "../../components/controls";
import { Callout, Page, Row, Rows, Section } from "../../components/layout";
import { ShortcutRecorder } from "../../components/ShortcutRecorder";
import type { Settings } from "../../generated/core";
import { allowTyping, cancelKeys, isMac, isWindows, refresh, restartShortcuts, saveSettings, settings, snapshot, stopTyping } from "../../lib/store";

function save(patch: Partial<Settings>) {
  return saveSettings(patch, { quiet: true });
}

/** Linux: whether dictation is pasted into the app or left on the clipboard. */
function TypingIntoApps() {
  const access = snapshot.value?.typingAccess ?? "unavailable";
  const [busy, setBusy] = useState(false);
  const act = async (action: () => Promise<void>) => {
    setBusy(true);
    try {
      await action();
    } finally {
      setBusy(false);
    }
  };
  const description =
    access === "allowed"
      ? "On. Onda pastes what you say into the app you're using. GNOME shows a remote-access icon in the top bar while this is on."
      : access === "notAllowed"
        ? "Off. Dictation is copied for you to paste with Ctrl+V, except into text fields Onda can fill in directly. Your desktop asks once."
        : "This desktop doesn't let apps type, so dictation is copied for you to paste with Ctrl+V, except into text fields Onda can fill in directly.";
  return (
    <Section title="Typing into apps">
      <Rows>
        <Row
          icon={<ClipboardPaste size={18} />}
          title="Paste into the app you're using"
          description={description}
          control={
            access === "allowed" ? (
              <Button variant="ghost" busy={busy} onClick={() => void act(stopTyping)}>
                Turn off
              </Button>
            ) : access === "notAllowed" ? (
              <Button variant="primary" busy={busy} onClick={() => void act(allowTyping)}>
                Allow
              </Button>
            ) : undefined
          }
        />
      </Rows>
    </Section>
  );
}

/** macOS: Onda pastes for you once the user turns it on under Accessibility in System Settings. */
function MacTypingIntoApps() {
  const allowed = snapshot.value?.typingAccess === "allowed";
  const [busy, setBusy] = useState(false);
  // The switch is in System Settings, so look again whenever the user comes back.
  useEffect(() => {
    const recheck = () => void refresh().catch(() => undefined);
    window.addEventListener("focus", recheck);
    return () => window.removeEventListener("focus", recheck);
  }, []);
  const act = async (action: () => Promise<void>) => {
    setBusy(true);
    try {
      await action();
    } finally {
      setBusy(false);
    }
  };
  return (
    <Section title="Typing into apps">
      <Rows>
        <Row
          icon={<ClipboardPaste size={18} />}
          title="Paste into the app you're using"
          description={
            allowed
              ? "On. Onda pastes what you say where your cursor is, and puts back what you had copied. To stop, turn Onda off under Privacy & Security › Accessibility."
              : "Off. What you say is copied, ready for ⌘V. Turn on Onda under Privacy & Security › Accessibility and it pastes for you."
          }
          control={
            allowed ? (
              <Button variant="ghost" busy={busy} onClick={() => void act(stopTyping)}>
                Open System Settings
              </Button>
            ) : (
              <Button variant="primary" busy={busy} onClick={() => void act(allowTyping)}>
                Allow
              </Button>
            )
          }
        />
      </Rows>
      <p class="footnote">Onda never pastes into password fields, or while an app protects what you type. After an update, macOS may need Onda switched off and on again there.</p>
    </Section>
  );
}

/** Linux without a shortcuts portal: the desktop runs a command instead. */
function DesktopShortcuts() {
  return (
    <Page title="Shortcuts" lede="Your desktop can't give Onda shortcuts, so its own keyboard settings start dictation.">
      <Callout icon={<Hand size={20} />} title="Add one shortcut in your desktop's settings">
        Make it run <span class="mono selectable">onda --dictate</span>. Press it once to start listening and again to finish. The tray menu does the same.
      </Callout>
      <Section title="Where to add it">
        <Rows>
          <Row icon={<Mic size={18} />} title="GNOME" description="Settings › Keyboard › View and Customize Shortcuts › Custom Shortcuts › Add Shortcut." />
          <Row icon={<Mic size={18} />} title="KDE Plasma" description="System Settings › Keyboard › Shortcuts › Add New › Command or Script." />
          <Row icon={<Mic size={18} />} title="Other desktops" description="Any setting that runs a command on a key works; use the full path to onda if it isn't on your PATH." />
        </Rows>
        <p class="footnote">Hold-to-talk, rewriting selected text and cancelling by key need a desktop with the global shortcuts portal, such as GNOME 48 or KDE Plasma 6. Until then, use the tray menu's Cancel dictation.</p>
      </Section>
      <TypingIntoApps />
    </Page>
  );
}

function ShortcutStatus({ status }: { status: string }) {
  const [busy, setBusy] = useState(false);
  const askAgain = async () => {
    setBusy(true);
    try {
      await restartShortcuts();
    } finally {
      setBusy(false);
    }
  };
  if (status === "unavailable" && isMac.value)
    return (
      <Callout tone="error" icon={<TriangleAlert size={20} />} title="A shortcut is taken">
        macOS or another app already uses one of these shortcuts, so Onda couldn't claim it. Choose other keys for it below.
      </Callout>
    );
  if (status === "unavailable")
    return (
      <Callout tone="error" icon={<TriangleAlert size={20} />} title="Shortcuts aren't working right now">
        The shortcut helper couldn't start. Restart Onda; if it keeps happening, reinstall it.
      </Callout>
    );
  if (status === "recovering")
    return (
      <Callout tone="warn" icon={<TriangleAlert size={20} />} title="Reconnecting shortcuts">
        This can happen after waking from sleep or switching desktops. It usually takes a moment.
      </Callout>
    );
  if (status === "declined")
    return (
      <Callout
        tone="warn"
        icon={<TriangleAlert size={20} />}
        title="Your desktop didn't approve the shortcuts"
        action={
          <Button variant="primary" busy={busy} onClick={() => void askAgain()}>
            Ask again
          </Button>
        }
      >
        Without them, start dictation from the tray menu.
      </Callout>
    );
  if (status === "starting" && !isWindows.value && !isMac.value)
    return (
      <p class="status-line">
        <Hourglass size={16} aria-hidden="true" /> Waiting for your desktop to approve the shortcuts
      </p>
    );
  return (
    <p class="status-line">
      <CircleCheck size={16} aria-hidden="true" /> Shortcuts are active
    </p>
  );
}

export function Shortcuts() {
  const value = settings.value;
  if (!value) return null;
  const status = snapshot.value?.shortcutStatus ?? "starting";
  if (!isWindows.value && status === "desktop") return <DesktopShortcuts />;
  const cancel = cancelKeys(value.pushToTalk.keys);
  const escOnly = cancel.length === 1;
  const lede = isWindows.value
    ? "Onda listens for these anywhere in Windows. Esc always cancels."
    : isMac.value
      ? "Onda listens for these anywhere on your Mac. Esc cancels while Onda is listening."
      : "These work anywhere. Your desktop asks once before Onda can use them, and may let you pick other keys.";
  return (
    <Page title="Shortcuts" lede={lede}>
      <ShortcutStatus status={status} />

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
          <Row
            icon={<Ban size={18} />}
            title="Cancel"
            description={escOnly ? "Stops listening and discards what you said." : "Stops listening and discards what you said. Esc goes with your dictation keys, so other apps keep Esc."}
            control={escOnly ? <Keys keys={cancel} size="md" label="Escape" /> : <Keys keys={cancel} size="md" />}
          />
        </Rows>
      </Section>

      <Section title="Editing with AI" description="These use the provider set up in AI polish.">
        <Rows>
          <Row icon={<WandSparkles size={18} />} title="Rewrite selected text" description="Applies your default rewrite to the selection.">
            <ShortcutRecorder value={value.transformShortcut.keys} label="Rewrite selected text" onSave={(keys) => save({ transformShortcut: { keys } })} />
          </Row>
          <Row icon={<Mic size={18} />} title="Edit with your voice" description="Select text, hold the keys and say what to change.">
            <ShortcutRecorder value={value.commandShortcut.keys} label="Edit with your voice" onSave={(keys) => save({ commandShortcut: { keys } })} />
          </Row>
        </Rows>
      </Section>
      {isWindows.value ? (
        <p class="footnote">Shortcuts that use the Windows key can also trigger Windows features, such as switching keyboard layouts.</p>
      ) : isMac.value ? (
        <MacTypingIntoApps />
      ) : (
        <>
          <p class="footnote">Changing a shortcut here asks your desktop again. Keys you chose in its dialog are listed in its keyboard settings.</p>
          <TypingIntoApps />
        </>
      )}
    </Page>
  );
}
