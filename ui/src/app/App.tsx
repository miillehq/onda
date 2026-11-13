import { useComputed, useSignalEffect } from "@preact/signals";
import { useEffect, useRef } from "preact/hooks";
import { Lockup } from "../components/Brand";
import { Button } from "../components/controls";
import { Toasts } from "../components/feedback";
import { route } from "../lib/router";
import { operation, settings, snapshot, startupError } from "../lib/store";
import { Dictionary } from "../views/Dictionary";
import { History } from "../views/History";
import { Home } from "../views/Home";
import { Onboarding } from "../views/Onboarding";
import { Rewrites } from "../views/Rewrites";
import { SettingsView } from "../views/settings/SettingsView";
import { Snippets } from "../views/Snippets";
import { Stats } from "../views/Stats";
import { CommandPalette } from "./CommandPalette";
import { Sidebar } from "./Sidebar";

const PHASE_ANNOUNCEMENT: Partial<Record<string, string>> = {
  recording: "Listening",
  transcribing: "Writing it out",
  enhancing: "Polishing",
  completed: "Inserted",
  cancelled: "Cancelled",
};

/** Polite, phase-level announcements only; never streamed text. */
function Announcer() {
  const message = useComputed(() => {
    const state = operation.value;
    if (!state) return "";
    if (state.phase === "error") return state.error?.message ?? "Something went wrong";
    return PHASE_ANNOUNCEMENT[state.phase] ?? "";
  });
  return (
    <div class="visually-hidden" aria-live="polite" role="status">
      {message.value}
    </div>
  );
}

function View() {
  const current = route.value;
  switch (current) {
    case "home":
      return <Home />;
    case "stats":
      return <Stats />;
    case "history":
      return <History />;
    case "dictionary":
      return <Dictionary />;
    case "snippets":
      return <Snippets />;
    case "rewrites":
      return <Rewrites />;
    default:
      return current.startsWith("settings/") ? <SettingsView /> : <Home />;
  }
}

export function App() {
  const scroller = useRef<HTMLDivElement>(null);

  useSignalEffect(() => {
    const theme = settings.value?.theme ?? "system";
    const root = document.documentElement;
    if (theme === "system") delete root.dataset.theme;
    else root.dataset.theme = theme;
  });

  // New page, new reading position, and focus moves to the page heading for screen readers.
  useEffect(() => {
    scroller.current?.scrollTo({ top: 0 });
    scroller.current?.querySelector<HTMLElement>("h1")?.focus({ preventScroll: true });
  }, [route.value]);

  if (startupError.value) {
    return (
      <div class="splash">
        <Lockup height={30} />
        <h1>Onda couldn't start</h1>
        <p>{startupError.value.message}</p>
        <Button variant="primary" onClick={() => window.location.reload()}>
          Try again
        </Button>
      </div>
    );
  }
  if (!snapshot.value) {
    return (
      <div class="splash is-loading" aria-busy="true">
        <Lockup height={30} />
      </div>
    );
  }
  if (!snapshot.value.settings.onboardingCompleted || route.value === "onboarding") {
    return (
      <>
        <Onboarding />
        <Toasts />
      </>
    );
  }
  return (
    <div class="shell">
      <Sidebar />
      <main class="main" aria-label="Content" data-geo="content">
        <div class="main-scroll" ref={scroller}>
          <View />
        </div>
      </main>
      <CommandPalette />
      <Toasts />
      <Announcer />
    </div>
  );
}
