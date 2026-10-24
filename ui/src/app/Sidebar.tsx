import { useComputed } from "@preact/signals";
import {
  AppWindow,
  ArrowLeft,
  AudioLines,
  BookOpen,
  History as HistoryIcon,
  House,
  Keyboard,
  LifeBuoy,
  Mic,
  Search,
  Settings as SettingsIcon,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  TextQuote,
  WandSparkles,
} from "lucide-preact";
import type { ComponentChildren } from "preact";
import { useEffect } from "preact/hooks";
import { languageName } from "../lib/languages";
import { inSettings, leaveSettings, navigate, route, type Route } from "../lib/router";
import { devices, dictionary, isBusy, loadDevices, micLevel, micName, modelReady, operation, selectedModel, settings, snapshot, snippets, transforms } from "../lib/store";
import { openPalette } from "./CommandPalette";

interface NavItem {
  to: Route;
  label: string;
  icon: ComponentChildren;
  count?: number;
}

function NavLink({ item }: { item: NavItem }) {
  const current = route.value === item.to;
  return (
    <a
      href={`#${item.to}`}
      class={`nav${current ? " is-current" : ""}`}
      aria-current={current ? "page" : undefined}
      title={item.label}
      onClick={(event) => {
        event.preventDefault();
        navigate(item.to);
      }}
    >
      {item.icon}
      <span class="nav-label">{item.label}</span>
      {item.count !== undefined && item.count > 0 && <span class="nav-count tabular">{item.count}</span>}
    </a>
  );
}

const QUALITY_LABEL = { fast: "Fast", balanced: "Balanced", bestQuality: "Quality", custom: "Custom" } as const;

function StatusCard() {
  useEffect(() => {
    if (devices.value === null) void loadDevices().catch(() => undefined);
  }, []);
  const status = useComputed(() => {
    const state = operation.value;
    const shortcut = snapshot.value?.shortcutStatus;
    if (!modelReady.value) return { tone: "warn", title: "Setup needed", to: "settings/speech" as Route };
    if (shortcut === "unavailable") return { tone: "err", title: "Shortcuts unavailable", to: "settings/shortcuts" as Route };
    switch (state?.phase) {
      case "recording":
        return { tone: "live", title: "Listening", to: "home" as Route };
      case "finalizingAudio":
      case "transcribing":
      case "inserting":
        return { tone: "live", title: "Writing it out", to: "home" as Route };
      case "enhancing":
        return { tone: "live", title: state.route === "chatgptPlan" ? "Polishing" : "Working", to: "home" as Route };
      case "error":
        return { tone: "err", title: "Needs attention", to: "home" as Route };
      default:
        return { tone: "ok", title: shortcut === "recovering" ? "Reconnecting shortcuts" : "Ready", to: "settings/speech" as Route };
    }
  });
  const value = settings.value;
  if (!value) return null;
  // Catalog names read "Quality · Multilingual"; the language is shown separately.
  const quality = value.qualityProfile === "custom" ? selectedModel.value?.name.split(" · ")[0] ?? "Custom model" : QUALITY_LABEL[value.qualityProfile];
  const { tone, title, to } = status.value;
  return (
    <button
      type="button"
      class="status"
      onClick={() => navigate(to)}
      data-model-id={snapshot.value?.selectedModelId ?? ""}
      data-testid="status"
      aria-label={`${title}. ${quality}, ${languageName(value.language)}. ${micName.value}.`}
    >
      <span class="status-top">
        <span class={`dot dot-${tone}`} aria-hidden="true" />
        <span class="status-title">{title}</span>
      </span>
      <span class="status-sub">
        {quality} · {languageName(value.language)} · on this PC
      </span>
      <span class="status-mic">
        <Mic size={15} aria-hidden="true" />
        <span class="status-mic-name">{micName.value}</span>
        <MiniMeter />
      </span>
    </button>
  );
}

function MiniMeter() {
  const active = isBusy.value && operation.value?.phase === "recording";
  const heights = [0.35, 0.7, 1, 0.55, 0.3];
  return (
    <span class={`mini-meter${active ? " is-active" : ""}`} aria-hidden="true">
      {heights.map((weight, index) => (
        <i key={index} style={{ transform: `scaleY(${active ? Math.max(0.18, Math.min(1, micLevel.value * 1.4 * weight + 0.12)) : weight * 0.6 + 0.2})` }} />
      ))}
    </span>
  );
}

export function Sidebar() {
  const settingsMode = inSettings.value;
  const main: NavItem[] = [
    { to: "home", label: "Home", icon: <House size={19} /> },
    { to: "history", label: "History", icon: <HistoryIcon size={19} /> },
    { to: "dictionary", label: "Dictionary", icon: <BookOpen size={19} />, count: dictionary.value.length },
    { to: "snippets", label: "Snippets", icon: <TextQuote size={19} />, count: snippets.value.length },
    { to: "rewrites", label: "Rewrites", icon: <WandSparkles size={19} />, count: transforms.value.length },
  ];
  const sections: NavItem[] = [
    { to: "settings/general", label: "General", icon: <SlidersHorizontal size={19} /> },
    { to: "settings/shortcuts", label: "Shortcuts", icon: <Keyboard size={19} /> },
    { to: "settings/microphone", label: "Microphone", icon: <Mic size={19} /> },
    { to: "settings/speech", label: "Speech and language", icon: <AudioLines size={19} /> },
    { to: "settings/ai", label: "AI polish", icon: <Sparkles size={19} /> },
    { to: "settings/apps", label: "App styles", icon: <AppWindow size={19} /> },
    { to: "settings/privacy", label: "Privacy", icon: <ShieldCheck size={19} /> },
    { to: "settings/about", label: "Updates and help", icon: <LifeBuoy size={19} /> },
  ];
  return (
    <nav class="sidebar" aria-label={settingsMode ? "Settings" : "Main"}>
      {settingsMode ? (
        <button type="button" class="back" onClick={leaveSettings}>
          <span class="back-icon">
            <ArrowLeft size={16} />
          </span>
          <span class="nav-label">Settings</span>
        </button>
      ) : (
        <button type="button" class="search" onClick={openPalette} title="Search (Ctrl+K)">
          <Search size={16} />
          <span class="nav-label">Search</span>
          <kbd class="search-key">Ctrl K</kbd>
        </button>
      )}
      <div class="nav-list">
        {(settingsMode ? sections : main).map((item) => (
          <NavLink item={item} key={item.to} />
        ))}
      </div>
      <div class="sidebar-spacer" />
      <StatusCard />
      {!settingsMode && <NavLink item={{ to: "settings/general", label: "Settings", icon: <SettingsIcon size={19} /> }} />}
    </nav>
  );
}
