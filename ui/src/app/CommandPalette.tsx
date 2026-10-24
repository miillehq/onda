import { signal } from "@preact/signals";
import { CornerDownLeft, Search } from "lucide-preact";
import { Fragment } from "preact";
import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { navigate, type Route } from "../lib/router";
import { copyResult, lastResult, saveSettings, settings } from "../lib/store";
import { requestEditor } from "./intents";

const open = signal(false);
export const openPalette = () => {
  open.value = true;
};

interface Command {
  id: string;
  label: string;
  group: "Go to" | "Settings" | "Actions";
  keywords?: string | undefined;
  run: () => void;
}

function go(id: string, label: string, to: Route, group: Command["group"], keywords?: string): Command {
  return { id, label, group, keywords, run: () => navigate(to) };
}

function commands(): Command[] {
  const list: Command[] = [
    go("home", "Home", "home", "Go to"),
    go("history", "History", "history", "Go to", "recent past dictations transcripts said copy"),
    go("dictionary", "Dictionary", "dictionary", "Go to", "words vocabulary names replacements"),
    go("snippets", "Snippets", "snippets", "Go to", "text expansion phrases"),
    go("rewrites", "Rewrites", "rewrites", "Go to", "transform selected text chatgpt"),
    go("general", "General", "settings/general", "Settings", "theme dark light startup login pill sounds"),
    go("shortcuts", "Shortcuts", "settings/shortcuts", "Settings", "keys hotkey hands-free toggle push to talk"),
    go("microphone", "Microphone", "settings/microphone", "Settings", "mic input test level device"),
    go("speech", "Speech and language", "settings/speech", "Settings", "model quality language download fast balanced gpu"),
    go("ai", "AI polish", "settings/ai", "Settings", "chatgpt account sign in cleanup polish"),
    go("apps", "App styles", "settings/apps", "Settings", "per app profile paste"),
    go("privacy", "Privacy", "settings/privacy", "Settings", "data learning diagnostics history retention keep delete"),
    go("about", "Updates and help", "settings/about", "Settings", "version update help license"),
    { id: "add-word", label: "Add a word to the dictionary", group: "Actions", keywords: "new vocabulary", run: () => requestEditor("dictionary") },
    { id: "add-snippet", label: "Create a snippet", group: "Actions", keywords: "new", run: () => requestEditor("snippets") },
    { id: "add-rewrite", label: "Create a rewrite", group: "Actions", keywords: "new transform", run: () => requestEditor("rewrites") },
    { id: "test-mic", label: "Test microphone", group: "Actions", run: () => navigate("settings/microphone") },
  ];
  const theme = settings.value?.theme;
  list.push({
    id: "theme",
    label: theme === "dark" ? "Switch to light theme" : "Switch to dark theme",
    group: "Actions",
    keywords: "appearance mode",
    run: () => void saveSettings({ theme: theme === "dark" ? "light" : "dark" }),
  });
  if (lastResult.value) list.push({ id: "copy", label: "Copy last dictation", group: "Actions", run: () => void copyResult() });
  return list;
}

export function CommandPalette() {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        open.value = !open.value;
      } else if (event.ctrlKey && event.key === ",") {
        event.preventDefault();
        navigate("settings/general");
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    if (open.value) {
      setQuery("");
      setIndex(0);
      requestAnimationFrame(() => input.current?.focus());
    }
  }, [open.value]);

  const results = useMemo(() => {
    const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    return commands().filter((command) => {
      const haystack = `${command.label} ${command.keywords ?? ""} ${command.group}`.toLowerCase();
      return terms.every((term) => haystack.includes(term));
    });
  }, [query, open.value]);

  if (!open.value) return null;

  const close = () => {
    open.value = false;
  };
  const runAt = (at: number) => {
    const command = results[at];
    if (!command) return;
    close();
    command.run();
  };
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setIndex((value) => Math.min(results.length - 1, value + 1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setIndex((value) => Math.max(0, value - 1));
    } else if (event.key === "Enter") {
      event.preventDefault();
      runAt(index);
    } else if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  };
  let lastGroup = "";
  return (
    <div class="palette-layer" onMouseDown={(event) => event.target === event.currentTarget && close()}>
      <div class="palette" role="dialog" aria-modal="true" aria-label="Search Onda">
        <div class="palette-input">
          <Search size={18} aria-hidden="true" />
          <input
            ref={input}
            value={query}
            placeholder="Search settings and actions"
            aria-label="Search settings and actions"
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-list"
            aria-activedescendant={results[index] ? `palette-${results[index]?.id}` : undefined}
            onInput={(event) => {
              setQuery((event.currentTarget as HTMLInputElement).value);
              setIndex(0);
            }}
            onKeyDown={onKeyDown}
          />
          <kbd class="search-key">Esc</kbd>
        </div>
        <ul class="palette-list" id="palette-list" role="listbox">
          {results.length === 0 && <li class="palette-empty">Nothing matches “{query}”.</li>}
          {results.map((command, at) => {
            const header = command.group !== lastGroup ? command.group : null;
            lastGroup = command.group;
            return (
              <Fragment key={command.id}>
                {header && (
                  <li class="palette-group" role="presentation">
                    {header}
                  </li>
                )}
                <li
                  id={`palette-${command.id}`}
                  role="option"
                  aria-selected={at === index}
                  class={`palette-item${at === index ? " is-active" : ""}`}
                  onMouseMove={() => setIndex(at)}
                  onClick={() => runAt(at)}
                >
                  <span>{command.label}</span>
                  {at === index && <CornerDownLeft size={15} aria-hidden="true" />}
                </li>
              </Fragment>
            );
          })}
        </ul>
      </div>
    </div>
  );
}
