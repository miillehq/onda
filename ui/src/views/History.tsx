import { ChevronDown, Copy, Cpu, History as HistoryIcon, Lock, Search, Sparkles, Trash2 } from "lucide-preact";
import { useMemo, useState } from "preact/hooks";
import { Button, Chip, IconButton } from "../components/controls";
import { Dialog, EmptyState, Page } from "../components/layout";
import type { HistoryEntry } from "../generated/core";
import { presentError } from "../lib/errors";
import { appName, plural, routeLabel } from "../lib/format";
import { navigate } from "../lib/router";
import { Suggestion } from "../components/feedback";
import { clearHistory, copyHistoryEntry, copyHistorySuggestion, deleteHistoryEntry, history, isWindows, settings, thisDevice, toast } from "../lib/store";

const PAGE_SIZE = 60;

/** "for 3 days" / "until you quit Onda" — how long History keeps an entry. */
export function retentionPhrase(days: number): string {
  if (days <= 0) return "until you quit Onda";
  return days === 1 ? "for a day" : `for ${days} days`;
}

function dayKey(ms: number): string {
  const date = new Date(ms);
  return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
}

function dayLabel(ms: number): string {
  const now = Date.now();
  if (dayKey(ms) === dayKey(now)) return "Today";
  if (dayKey(ms) === dayKey(now - 86_400_000)) return "Yesterday";
  return new Date(ms).toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" });
}

const time = (ms: number) => new Date(ms).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });

const MODE_LABEL: Partial<Record<HistoryEntry["mode"], string>> = { spokenCommand: "Voice edit", transform: "Rewrite" };

function failed(entry: HistoryEntry): string | null {
  // A skipped polish leaves the original where it was, so nothing went missing.
  if (entry.inserted || !entry.errorCode || entry.errorCode === "invalidEnhancement") return null;
  return presentError({ code: entry.errorCode, message: "", retryable: false }).title;
}

/** One remembered dictation or edit. Compact rows sit on Home; full rows on the History page. */
export function HistoryItem({ entry, compact = false }: { entry: HistoryEntry; compact?: boolean }) {
  const [open, setOpen] = useState(false);
  const [comparing, setComparing] = useState(false);
  const failure = failed(entry);
  const skipped = entry.fallbackUsed || !!entry.suggestion;
  const edit = entry.mode !== "dictation";
  const changed = entry.original !== entry.output;
  const mode = MODE_LABEL[entry.mode];
  return (
    <li class={`history-item${failure ? " is-failed" : ""}${compact ? " is-compact" : ""}`}>
      <time class="history-time tabular" dateTime={new Date(entry.createdAt).toISOString()}>
        {time(entry.createdAt)}
      </time>
      <div class="history-body">
        {entry.commandText && <p class="history-command">You asked: “{entry.commandText}”</p>}
        <p class="history-text serif selectable">{entry.output}</p>
        {open && changed && (
          <div class="history-original">
            <span class="history-original-label">{edit ? "Before the edit" : "Before polish"}</span>
            <p class="serif selectable">{entry.original}</p>
          </div>
        )}
        {comparing && entry.suggestion && <Suggestion kept={entry.output} suggestion={entry.suggestion} onCopy={() => void copyHistorySuggestion(entry.id)} />}
        <div class="history-meta">
          {failure && (
            <Chip tone="err" title={failure}>
              Not typed
            </Chip>
          )}
          {entry.app && <span>{appName(entry.app)}</span>}
          {mode && <span>{mode}</span>}
          {routeLabel(entry.route) && !skipped ? (
            <span class="history-route" title={routeLabel(entry.route) ?? undefined}>
              <Sparkles size={13} aria-hidden="true" />
              Polished
            </span>
          ) : (
            <span class="history-route">
              <Cpu size={13} aria-hidden="true" />
              On this {isWindows.value ? "PC" : "computer"}
            </span>
          )}
          {skipped && <Chip tone="warn">Polish skipped</Chip>}
          {!compact && changed && (
            <button type="button" class="link-button history-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
              {open ? "Hide original" : edit ? "Show original" : "Show before polish"}
              <ChevronDown size={14} aria-hidden="true" />
            </button>
          )}
          {!compact && entry.suggestion && (
            <button type="button" class="link-button history-toggle" aria-expanded={comparing} onClick={() => setComparing(!comparing)}>
              {comparing ? "Hide polished version" : "Show polished version"}
              <ChevronDown size={14} aria-hidden="true" />
            </button>
          )}
        </div>
      </div>
      <div class="history-actions">
        <IconButton label="Copy" icon={<Copy size={17} />} onClick={() => void copyHistoryEntry(entry.id)} />
        {!compact && <IconButton label="Delete" icon={<Trash2 size={17} />} onClick={() => void deleteHistoryEntry(entry.id)} />}
      </div>
    </li>
  );
}

export function History() {
  const value = settings.value;
  const [query, setQuery] = useState("");
  const [limit, setLimit] = useState(PAGE_SIZE);
  const [confirming, setConfirming] = useState(false);
  const [clearing, setClearing] = useState(false);
  const entries = history.value ?? [];
  const terms = query.trim().toLocaleLowerCase();
  const matches = useMemo(
    () =>
      terms
        ? entries.filter((entry) => `${entry.output}\n${entry.original}\n${entry.commandText ?? ""}\n${entry.app ? appName(entry.app) : ""}`.toLocaleLowerCase().includes(terms))
        : entries,
    [entries, terms],
  );
  const groups = useMemo(() => {
    const result: { label: string; items: HistoryEntry[] }[] = [];
    for (const entry of matches.slice(0, limit)) {
      const label = dayLabel(entry.createdAt);
      const last = result[result.length - 1];
      if (last?.label === label) last.items.push(entry);
      else result.push({ label, items: [entry] });
    }
    return result;
  }, [matches, limit]);
  if (!value) return null;
  const days = value.historyDays;
  const confirmClear = async () => {
    setClearing(true);
    const done = await clearHistory();
    setClearing(false);
    setConfirming(false);
    if (done) toast({ tone: "ok", title: "History cleared" }, 2400);
  };
  return (
    <Page
      title="History"
      lede={
        <>
          What you said, kept {retentionPhrase(days)} and encrypted on {thisDevice.value}.{" "}
          <button type="button" class="link-button" onClick={() => navigate("settings/privacy")}>
            Change
          </button>
        </>
      }
      actions={
        <Button variant="ghost" icon={<Trash2 size={16} />} disabled={!entries.length} onClick={() => setConfirming(true)}>
          Clear all
        </Button>
      }
    >
      {entries.length > 0 && (
        <div class="toolbar" data-geo="history-toolbar">
          <span class="muted tabular">{plural(entries.length, "entry", "entries")}</span>
          <label class="search-field">
            <Search size={16} aria-hidden="true" />
            <input
              type="search"
              placeholder="Search"
              aria-label="Search history"
              value={query}
              onInput={(event) => {
                setQuery((event.currentTarget as HTMLInputElement).value);
                setLimit(PAGE_SIZE);
              }}
            />
          </label>
        </div>
      )}

      {groups.length ? (
        <div class="history-days">
          {groups.map((group) => (
            <section class="history-day" key={group.label} aria-label={group.label}>
              <h2 class="history-day-label">{group.label}</h2>
              <ol class="history-list">
                {group.items.map((entry) => (
                  <HistoryItem key={entry.id} entry={entry} />
                ))}
              </ol>
            </section>
          ))}
          {matches.length > limit && (
            <div class="history-more">
              <Button onClick={() => setLimit(limit + PAGE_SIZE)}>Show more</Button>
            </div>
          )}
        </div>
      ) : terms ? (
        <EmptyState title={`Nothing matches “${query.trim()}”`}>Try a shorter search.</EmptyState>
      ) : (
        <EmptyState art={<HistoryIcon size={20} />} title="Nothing here yet">
          Everything you dictate lands here, so if it doesn't make it into your app you can copy it from here. Kept {retentionPhrase(days)}.
        </EmptyState>
      )}

      <Dialog
        open={confirming}
        title="Clear history?"
        description={`This deletes ${plural(entries.length, "entry", "entries")} from ${thisDevice.value}. You can't undo it.`}
        onClose={() => setConfirming(false)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
            <Button variant="danger" busy={clearing} onClick={() => void confirmClear()}>
              Clear history
            </Button>
          </>
        }
      >
        <p class="muted-note">
          <Lock size={14} aria-hidden="true" />
          Your dictionary, snippets and settings stay as they are.
        </p>
      </Dialog>
    </Page>
  );
}
