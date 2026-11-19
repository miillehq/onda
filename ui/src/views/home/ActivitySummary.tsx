import { useComputed } from "@preact/signals";
import { ArrowRight, AudioLines, BookOpen, Gauge } from "lucide-preact";
import { Chip } from "../../components/controls";
import { formatDuration, formatInteger } from "../../lib/format";
import { navigate } from "../../lib/router";
import { dictionary, usage } from "../../lib/store";

const TYPING_WPM = 40;

function dayKey(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

function useWeek() {
  return useComputed(() => {
    const report = usage.value;
    const byDay = new Map(report?.days.map((day) => [day.day, day]) ?? []);
    const [year, month, date] = (report?.today ?? dayKey(new Date())).split("-").map(Number);
    const today = new Date(year ?? 1970, (month ?? 1) - 1, date ?? 1);
    const monday = new Date(today);
    monday.setDate(today.getDate() - ((today.getDay() + 6) % 7));
    const week = Array.from({ length: 7 }, (_, index) => {
      const day = new Date(monday);
      day.setDate(monday.getDate() + index);
      const key = dayKey(day);
      return { key, label: day.toLocaleDateString(undefined, { weekday: "narrow" }), name: day.toLocaleDateString(undefined, { weekday: "long" }), words: byDay.get(key)?.words ?? 0, today: key === dayKey(today), future: day > today };
    });
    let rollingWords = 0;
    let rollingAudio = 0;
    for (let offset = 0; offset < 7; offset += 1) {
      const day = new Date(today);
      day.setDate(today.getDate() - offset);
      const entry = byDay.get(dayKey(day));
      rollingWords += entry?.words ?? 0;
      rollingAudio += entry?.audioMs ?? 0;
    }
    // Under a minute of speech the rate is mostly pauses, so wait for a fairer sample.
    const wpm = rollingAudio >= 60_000 && rollingWords > 0 ? rollingWords / (rollingAudio / 60_000) : null;
    const savedMs = wpm ? Math.max(0, (rollingWords / TYPING_WPM) * 60_000 - rollingAudio) : 0;
    return { week, weekWords: week.reduce((sum, day) => sum + day.words, 0), wpm, savedMs, loaded: report !== null };
  });
}

export function Stats() {
  const stats = useWeek().value;
  const max = Math.max(1, ...stats.week.map((day) => day.words));
  const seen = new Set<string>();
  const learned = [...dictionary.value]
    .filter((entry) => entry.enabled)
    .sort((a, b) => Number(b.source === "autoLearned") - Number(a.source === "autoLearned") || Number(b.starred) - Number(a.starred))
    .filter((entry) => {
      const label = (entry.replacement ?? entry.phrase).toLocaleLowerCase();
      if (seen.has(label)) return false;
      seen.add(label);
      return true;
    })
    .slice(0, 5);
  return (
    <div class="stats">
      <section class="tile" aria-labelledby="stat-week">
        <h3 class="tile-label" id="stat-week">
          <AudioLines size={16} aria-hidden="true" />
          Words this week
        </h3>
        <p class="tile-value tabular">{formatInteger(stats.weekWords)}</p>
        <div class="week-chart" role="img" aria-label={stats.week.map((day) => `${day.name}: ${day.words} words`).join(", ")}>
          {stats.week.map((day) => (
            <div class={`week-day${day.today ? " is-today" : ""}${day.future ? " is-future" : ""}`} key={day.key}>
              <i style={{ height: `${Math.max(6, (day.words / max) * 100)}%` }} />
              <span>{day.label}</span>
            </div>
          ))}
        </div>
      </section>
      <section class="tile" aria-labelledby="stat-speed">
        <h3 class="tile-label" id="stat-speed">
          <Gauge size={16} aria-hidden="true" />
          Your speaking speed
        </h3>
        {stats.wpm ? (
          <>
            <p class="tile-value tabular">
              {Math.round(stats.wpm)}
              <small>wpm</small>
            </p>
            <div class="versus" role="img" aria-label={`${Math.round(stats.wpm)} words per minute speaking, compared with about ${TYPING_WPM} typing`}>
              <div class="versus-row">
                <span>Speaking</span>
                <i style={{ width: `${(stats.wpm / Math.max(stats.wpm, TYPING_WPM)) * 100}%` }} />
              </div>
              <div class="versus-row is-muted">
                <span>Typing</span>
                <i style={{ width: `${(TYPING_WPM / Math.max(stats.wpm, TYPING_WPM)) * 100}%` }} />
              </div>
            </div>
            <p class="tile-foot">
              {stats.wpm >= TYPING_WPM * 1.1 ? (
                <>
                  <b>{(stats.wpm / TYPING_WPM).toFixed(1)}× faster</b> than typing{stats.savedMs >= 60_000 ? ` · ${formatDuration(stats.savedMs)} saved` : ""}
                </>
              ) : (
                <>Typical typing is about {TYPING_WPM} wpm. Speaking naturally, without long pauses, gets you past it.</>
              )}
            </p>
          </>
        ) : (
          <>
            <p class="tile-value is-empty">—</p>
            <p class="tile-foot">After a minute or so of dictation, see how your speed compares with typing.</p>
          </>
        )}
      </section>
      <section class="tile" aria-labelledby="stat-dictionary">
        <h3 class="tile-label" id="stat-dictionary">
          <BookOpen size={16} aria-hidden="true" />
          Your dictionary
        </h3>
        <p class="tile-value tabular">
          {formatInteger(dictionary.value.length)}
          <small>{dictionary.value.length === 1 ? "word" : "words"}</small>
        </p>
        {learned.length ? (
          <div class="chip-cloud">
            {learned.map((entry) => (
              <Chip key={entry.id} tone={entry.source === "autoLearned" ? "strong" : "neutral"} title={entry.source === "autoLearned" ? "Learned from your corrections" : undefined}>
                {entry.replacement ?? entry.phrase}
              </Chip>
            ))}
          </div>
        ) : (
          <button type="button" class="link-button" onClick={() => navigate("dictionary")}>
            Add names and jargon <ArrowRight size={14} />
          </button>
        )}
      </section>
    </div>
  );
}

/* ------------------------------------------------------------------ result */
