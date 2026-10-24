import { Bell, LogIn, Monitor, Moon, PanelBottom, RotateCcw, Sun, X } from "lucide-preact";
import { Button, Switch } from "../../components/controls";
import { Page, Row, Rows, Section } from "../../components/layout";
import { Wave } from "../../components/Wave";
import { navigate } from "../../lib/router";
import { saveSettings, settings } from "../../lib/store";

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
    <Page title="General" lede="How Onda looks and behaves on your PC.">
      <Section title="Appearance">
        <div class="theme-picker" role="presentation">
          {(
            [
              { id: "system", label: "Match Windows", icon: <Monitor size={16} /> },
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

      <Section title="While you dictate">
        <Rows>
          <Row
            icon={<PanelBottom size={18} />}
            title="Show the dictation pill"
            description="A small indicator above the taskbar shows when Onda is listening. It never takes focus from your app."
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

