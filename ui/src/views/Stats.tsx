import { useComputed } from "@preact/signals";
import { ChartNoAxesColumn, Flame, Gauge, ImageDown, Share2, Type } from "lucide-preact";
import type { ComponentChildren } from "preact";
import { useEffect, useRef, useState } from "preact/hooks";
import { Avatar, pictureUrl } from "../components/Avatar";
import { Button, Switch } from "../components/controls";
import { Card, Dialog, EmptyState, Page, Section } from "../components/layout";
import { activityGrid, parseDay } from "../lib/activity";
import { formatInteger, plural } from "../lib/format";
import { navigate } from "../lib/router";
import { isWindows, loadUsage, profile, saveStatsImage, toast, toastError, usage } from "../lib/store";
import { canvasBase64, renderStatsImage } from "../lib/statsImage";

const WEEKS = 53;

function longDate(key: string): string {
  return parseDay(key).toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric" });
}

function Tile({ icon, label, value, unit, foot }: { icon: ComponentChildren; label: string; value: string; unit?: string | undefined; foot?: ComponentChildren }) {
  return (
    <section class="tile" aria-label={label}>
      <h3 class="tile-label">
        {icon}
        {label}
      </h3>
      <p class={`tile-value tabular${value === "—" ? " is-empty" : ""}`}>
        {value}
        {unit && <small>{unit}</small>}
      </p>
      {foot && <p class="tile-foot">{foot}</p>}
    </section>
  );
}

function ActivityGrid() {
  const grid = useComputed(() => {
    const report = usage.value;
    return report ? activityGrid(report.days, report.today, WEEKS) : null;
  }).value;
  if (!grid) return null;
  return (
    <Card class="activity">
      {/* Opens on the most recent weeks when the window is too narrow for the whole year. */}
      <div
        class="activity-scroll"
        ref={(node) => {
          if (node) node.scrollLeft = node.scrollWidth;
        }}
      >
        <div class="activity-months" aria-hidden="true" style={{ gridTemplateColumns: `repeat(${WEEKS}, minmax(0, 1fr))` }}>
          {grid.months.map((month) => (
            <span key={month.column} style={{ gridColumn: `${month.column + 1} / span 4` }}>
              {month.label}
            </span>
          ))}
        </div>
        <div class="activity-grid" role="grid" aria-label={`Dictation activity for the last ${WEEKS} weeks, ${plural(grid.activeDays, "active day")}`}>
          {grid.weeks.map((week, column) => (
            <div class="activity-week" role="row" key={column}>
              {week.map((day) => (
                <span
                  key={day.key}
                  role="gridcell"
                  class={`activity-day level-${day.level}${day.future ? " is-future" : ""}`}
                  title={day.future ? undefined : `${day.date.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" })}: ${day.dictations ? `${plural(day.words, "word")} in ${plural(day.dictations, "dictation")}` : "no dictation"}`}
                  aria-label={day.future ? undefined : `${longDate(day.key)}: ${day.dictations ? `${plural(day.words, "word")}, ${plural(day.dictations, "dictation")}` : "no dictation"}`}
                />
              ))}
            </div>
          ))}
        </div>
      </div>
      <div class="activity-legend" aria-hidden="true">
        <span>Less</span>
        {[0, 1, 2, 3, 4].map((level) => (
          <i key={level} class={`activity-day level-${level}`} />
        ))}
        <span>More</span>
      </div>
    </Card>
  );
}

function ShareDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const name = profile.value?.name ?? null;
  const picture = profile.value?.picture ?? null;
  // Off until chosen: only what the user sees and switches on goes into the image.
  const [withName, setWithName] = useState(false);
  const [withPicture, setWithPicture] = useState(false);
  const [saving, setSaving] = useState(false);
  const report = usage.value;
  useEffect(() => {
    if (!open || !report || !canvas.current) return;
    void renderStatsImage(
      { summary: report.summary, days: report.days, today: report.today, name: withName ? name : null, picture: withPicture ? pictureUrl(picture) : null },
      canvas.current,
    );
  }, [open, report, withName, withPicture, name, picture]);
  const save = async () => {
    if (!canvas.current) return;
    setSaving(true);
    try {
      const path = await saveStatsImage(canvasBase64(canvas.current));
      if (path) {
        toast({ tone: "ok", title: "Image saved", body: path });
        onClose();
      }
    } catch (error) {
      toastError(error, "Couldn't save the image");
    } finally {
      setSaving(false);
    }
  };
  return (
    <Dialog
      open={open}
      onClose={onClose}
      size="lg"
      title="Share your stats"
      description="Numbers and your activity grid only, never what you said."
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" icon={<ImageDown size={16} />} busy={saving} onClick={() => void save()} data-autofocus>
            Save image…
          </Button>
        </>
      }
    >
      <canvas ref={canvas} class="share-preview" role="img" aria-label="Preview of the image that will be saved" />
      <div class="share-options">
        <label class="share-option">
          <Switch checked={withName && !!name} disabled={!name} label="Include my name" onChange={setWithName} />
          <span>
            Include my name
            {!name && <small> · add one in Settings › General</small>}
          </span>
        </label>
        <label class="share-option">
          <Switch checked={withPicture && !!picture} disabled={!picture} label="Include my picture" onChange={setWithPicture} />
          <span>
            Include my picture
            {!picture && <small> · add one in Settings › General</small>}
          </span>
        </label>
      </div>
    </Dialog>
  );
}

export function Stats() {
  const [sharing, setSharing] = useState(false);
  useEffect(() => {
    void loadUsage().catch(() => undefined);
  }, []);
  const report = usage.value;
  const summary = report?.summary;
  const person = profile.value;
  // Who these numbers belong to, not another greeting: Home already says good afternoon.
  const header = (
    <div class="stats-person">
      <Avatar profile={person} size={56} />
      <div>
        <p class="stats-greeting">{person?.name || "Your dictation"}</p>
        <p class="stats-since">
          {summary?.since ? `Since ${longDate(summary.since)}` : "Nothing counted yet"} ·{" "}
          <a
            href="#settings/general"
            onClick={(event) => {
              event.preventDefault();
              navigate("settings/general");
            }}
          >
            {person?.name || person?.picture ? "Edit profile" : "Add your name and picture"}
          </a>
        </p>
      </div>
    </div>
  );
  return (
    <Page
      title="Stats"
      lede={`Counted on this ${isWindows.value ? "PC" : "computer"}. Only numbers are kept, never what you said.`}
      actions={
        summary && summary.dictations > 0 ? (
          <Button icon={<Share2 size={16} />} onClick={() => setSharing(true)}>
            Share stats
          </Button>
        ) : undefined
      }
    >
      {header}
      {!summary || summary.dictations === 0 ? (
        <EmptyState art={<ChartNoAxesColumn size={20} />} title="Your stats start with your first dictation">
          Words, pace and streaks appear here once Onda has typed something for you.
        </EmptyState>
      ) : (
        <>
          <div class="stats stats-four">
            <Tile icon={<Type size={16} />} label="Words dictated" value={formatInteger(summary.words)} foot={plural(summary.dictations, "dictation")} />
            <Tile
              icon={<Gauge size={16} />}
              label="Speaking pace"
              value={summary.wordsPerMinute ? String(Math.round(summary.wordsPerMinute)) : "—"}
              unit={summary.wordsPerMinute ? "wpm" : undefined}
              foot={summary.wordsPerMinute ? "Across all your recordings" : "Shown after a minute of recording"}
            />
            <Tile icon={<Flame size={16} />} label="Current streak" value={formatInteger(summary.currentStreak)} unit={summary.currentStreak === 1 ? "day" : "days"} foot={`Longest ${plural(summary.longestStreak, "day")}`} />
            <Tile icon={<ChartNoAxesColumn size={16} />} label="Active days" value={formatInteger(summary.activeDays)} foot="Days you dictated" />
          </div>
          <Section title="Activity" description="The past year. Darker days had more words.">
            <ActivityGrid />
          </Section>
        </>
      )}
      {/* The definitions are there for anyone who asks, not in the way of the numbers. */}
      <details class="disclosure stats-help">
        <summary>How these are counted</summary>
        <dl class="stats-rules">
          <div>
            <dt>What counts</dt>
            <dd>Dictations Onda typed{isWindows.value ? "" : " or put on the clipboard"}. Rewrites, previews and failed dictations don't. Words are counted before AI polish.</dd>
          </div>
          <div>
            <dt>Streak</dt>
            <dd>Days in a row with a dictation, up to today, or yesterday if you haven't dictated yet today.</dd>
          </div>
          <div>
            <dt>Speaking pace</dt>
            <dd>Words per minute of recording. Pauses count, so it reads lower than reading aloud.</dd>
          </div>
          <div>
            <dt>Days</dt>
            <dd>Each dictation counts on this computer's date at the time. Dictations from before Onda kept these numbers aren't included.</dd>
          </div>
        </dl>
      </details>
      <ShareDialog open={sharing} onClose={() => setSharing(false)} />
    </Page>
  );
}
