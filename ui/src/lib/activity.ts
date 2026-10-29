import type { DailyUsage } from "../generated/core";

/** One day in the activity grid. `level` is 0 (no dictation) to 4 (the busiest days). */
export interface ActivityDay {
  key: string;
  date: Date;
  words: number;
  dictations: number;
  level: 0 | 1 | 2 | 3 | 4;
  future: boolean;
}

export interface ActivityGrid {
  /** Columns of seven days, Monday first, oldest week first; the last column holds today. */
  weeks: ActivityDay[][];
  /** Column index and short name of each month's first column. */
  months: { column: number; label: string }[];
  activeDays: number;
}

export function dayKey(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

/** Parses a native `YYYY-MM-DD` day as a local calendar date, never as UTC. */
export function parseDay(key: string): Date {
  const [year, month, day] = key.split("-").map(Number);
  return new Date(year ?? 1970, (month ?? 1) - 1, day ?? 1);
}

/**
 * The last `weekCount` weeks ending with `today` (the native local day, so the grid never guesses
 * a time zone). Intensity is relative to the busiest day shown, in quarters, like GitHub's grid.
 */
export function activityGrid(days: DailyUsage[], today: string, weekCount: number): ActivityGrid {
  const byDay = new Map(days.map((day) => [day.day, day]));
  const end = parseDay(today);
  const monday = new Date(end);
  monday.setDate(end.getDate() - ((end.getDay() + 6) % 7));
  const start = new Date(monday);
  start.setDate(monday.getDate() - (weekCount - 1) * 7);
  const cells: ActivityDay[] = [];
  for (let index = 0; index < weekCount * 7; index += 1) {
    const date = new Date(start);
    date.setDate(start.getDate() + index);
    const key = dayKey(date);
    const entry = byDay.get(key);
    cells.push({ key, date, words: entry?.words ?? 0, dictations: entry?.dictations ?? 0, level: 0, future: date > end });
  }
  const busiest = Math.max(0, ...cells.map((cell) => cell.words));
  for (const cell of cells) {
    if (cell.dictations > 0 && busiest > 0) cell.level = Math.min(4, Math.max(1, Math.ceil((cell.words / busiest) * 4))) as ActivityDay["level"];
  }
  const weeks = Array.from({ length: weekCount }, (_, week) => cells.slice(week * 7, week * 7 + 7));
  const months: ActivityGrid["months"] = [];
  weeks.forEach((week, column) => {
    const first = week.find((cell) => cell.date.getDate() === 1);
    if (first || column === 0) months.push({ column, label: (first ?? week[0]!).date.toLocaleDateString(undefined, { month: "short" }) });
  });
  // A leading label squeezed against the next month's would overlap it.
  if (months.length > 1 && months[1]!.column - months[0]!.column < 2) months.shift();
  return { weeks, months, activeDays: cells.filter((cell) => cell.dictations > 0 && !cell.future).length };
}
