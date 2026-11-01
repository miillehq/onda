import { Bell, ImagePlus, LogIn, Monitor, Moon, PanelBottom, RotateCcw, Sun, Trash2, X } from "lucide-preact";
import { useEffect, useState } from "preact/hooks";
import { Avatar } from "../../components/Avatar";
import { Mark } from "../../components/Brand";
import { Button, Switch } from "../../components/controls";
import { Page, Row, Rows, Section } from "../../components/layout";
import { Wave } from "../../components/Wave";
import type { TrayIconStyle } from "../../generated/core";
import { greetingFor } from "../../lib/format";
import { navigate } from "../../lib/router";
import { chooseProfilePicture, isWindows, profile, removeProfilePicture, saveSettings, setProfileName, settings } from "../../lib/store";

function ProfileEditor() {
  const person = profile.value;
  const [name, setName] = useState(person?.name ?? "");
  const [busy, setBusy] = useState(false);
  useEffect(() => setName(person?.name ?? ""), [person?.name]);
  const commit = () => {
    if ((person?.name ?? "") !== name.trim()) void setProfileName(name);
  };
  const pick = async () => {
    setBusy(true);
    await chooseProfilePicture();
    setBusy(false);
  };
  return (
    <div class="profile-editor">
      <Avatar profile={person} size={72} />
      <div class="profile-fields">
        <label class="field-label" for="profile-name">
          Your name
        </label>
        <input
          id="profile-name"
          class="input"
          type="text"
          maxLength={80}
          autoComplete="given-name"
          placeholder="Optional"
          value={name}
          onInput={(event) => setName(event.currentTarget.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") event.currentTarget.blur();
          }}
        />
        <p class="field-hint">{name.trim() ? `Onda will say “${greetingFor(name)}”.` : "Used in greetings and on Stats."}</p>
        <div class="profile-actions">
          <Button size="sm" icon={<ImagePlus size={15} />} busy={busy} onClick={() => void pick()}>
            {person?.picture ? "Replace picture…" : "Choose picture…"}
          </Button>
          {person?.picture && (
            <Button size="sm" variant="ghost" icon={<Trash2 size={15} />} onClick={() => void removeProfilePicture()}>
              Remove picture
            </Button>
          )}
        </div>
        <p class="field-hint">PNG, JPEG or WebP. Onda keeps a small copy without its location or other details.</p>
      </div>
    </div>
  );
}

const TRAY_OPTIONS: { id: TrayIconStyle; label: string }[] = [
  { id: "auto", label: "Automatic" },
  { id: "white", label: "White" },
  { id: "black", label: "Black" },
];

function TrayIconPicker({ value }: { value: TrayIconStyle }) {
  return (
    <div class="tray-picker" role="radiogroup" aria-label="Tray icon">
      {TRAY_OPTIONS.map((option) => (
        <button type="button" role="radio" key={option.id} class={`tray-option${value === option.id ? " is-selected" : ""}`} aria-checked={value === option.id} onClick={() => void saveSettings({ trayIcon: option.id })}>
          <span class="tray-swatches" aria-hidden="true">
            {(option.id === "auto" || option.id === "white") && (
              <span class="tray-swatch is-dark">
                <Mark height={14} />
              </span>
            )}
            {(option.id === "auto" || option.id === "black") && (
              <span class="tray-swatch is-light">
                <Mark height={14} />
              </span>
            )}
          </span>
          <span class="theme-label">{option.label}</span>
        </button>
      ))}
    </div>
  );
}

function ThemePreview() {
  return (
    <span class="theme-preview" aria-hidden="true">
      <span class="theme-preview-side" />
      <span class="theme-preview-main">
        <i />
        <i />
        <i />
      </span>
    </span>
  );
}

export function General() {
  const value = settings.value;
  if (!value) return null;
  return (
    <Page title="General" lede={`How Onda looks and behaves on your ${isWindows.value ? "PC" : "computer"}.`}>
      <Section title="Profile" description="Kept on this computer. A shared stats image includes it only if you switch it on there.">
        <ProfileEditor />
      </Section>

      <Section title="Appearance">
        <div class="theme-picker" role="presentation">
          {(
            [
              { id: "system", label: isWindows.value ? "Match Windows" : "Match system", icon: <Monitor size={16} /> },
              { id: "light", label: "Light", icon: <Sun size={16} /> },
              { id: "dark", label: "Dark", icon: <Moon size={16} /> },
            ] as const
          ).map((option) => (
            <button
              type="button"
              key={option.id}
              class={`theme-option theme-${option.id}${value.theme === option.id ? " is-selected" : ""}`}
              aria-pressed={value.theme === option.id}
              onClick={() => void saveSettings({ theme: option.id })}
            >
              {option.id === "system" ? (
                <span class="theme-preview is-split" aria-hidden="true">
                  <ThemePreview />
                  <span class="theme-dark">
                    <ThemePreview />
                  </span>
                </span>
              ) : (
                <ThemePreview />
              )}
              <span class="theme-label">
                {option.icon}
                {option.label}
              </span>
            </button>
          ))}
        </div>
      </Section>

      <Section
        title="Tray icon"
        description={
          isWindows.value
            ? "Automatic is white on a dark taskbar and black on a light one."
            : "The Onda mark in the top bar or panel. Linux doesn't report the panel's colour, so Automatic uses white, which suits most dark panels; choose Black for a light one."
        }
      >
        <TrayIconPicker value={value.trayIcon} />
      </Section>

      <Section title="While you dictate">
        <Rows>
          <Row
            icon={<PanelBottom size={18} />}
            title="Show the dictation pill"
            description={isWindows.value ? "A small indicator above the taskbar shows when Onda is listening. It never takes focus from your app." : "A small indicator at the bottom of the screen shows when Onda is listening. Some Wayland desktops place it themselves."}
            control={<Switch checked={value.showPill} label="Show the dictation pill" onChange={(showPill) => void saveSettings({ showPill })} />}
          >
            <span class={`pill-capsule pill-preview${value.showPill ? "" : " is-off"}`} aria-hidden="true">
              <Wave mode="still" idle={0.55} glyph width={64} height={24} />
            </span>
          </Row>
          <Row
            icon={<Bell size={18} />}
            title="Play sounds"
            description="A soft tone when Onda starts and stops listening, so you know without looking."
            control={<Switch checked={value.playSounds} label="Play sounds" onChange={(playSounds) => void saveSettings({ playSounds })} />}
          />
        </Rows>
      </Section>

      <Section title="Startup">
        <Rows>
          <Row
            icon={<LogIn size={18} />}
            title="Start Onda when you sign in"
            description="Onda waits quietly in the tray, ready for your shortcut."
            control={<Switch checked={value.launchAtLogin} label="Start Onda when you sign in" onChange={(launchAtLogin) => void saveSettings({ launchAtLogin })} />}
          />
          <Row icon={<X size={18} />} title="Closing the window" description="Onda keeps running in the tray so your shortcut still works. Quit from the tray icon's menu." />
          <Row
            icon={<RotateCcw size={18} />}
            title="Setup guide"
            description="Walk through microphone, model and shortcut setup again."
            control={
              <Button size="sm" onClick={() => navigate("onboarding")}>
                Run setup
              </Button>
            }
          />
        </Rows>
      </Section>
    </Page>
  );
}

