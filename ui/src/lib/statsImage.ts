import { MARK_PATH, MARK_SIZE, WORDMARK_PATH, WORDMARK_SIZE } from "../components/Brand";
import type { DailyUsage, UsageSummary } from "../generated/core";
import { activityGrid, parseDay } from "./activity";
import { formatInteger } from "./format";

/** What the shareable card shows. Only content-free counts; name and picture only if chosen. */
export interface StatsCard {
  summary: UsageSummary;
  days: DailyUsage[];
  today: string;
  name: string | null;
  /** A blob: URL of the stored profile PNG. */
  picture: string | null;
}

export const CARD_WIDTH = 1200;
export const CARD_HEIGHT = 630;
const SCALE = 2;
const PADDING = 64;
const FONT = '"Inter Variable", "Segoe UI", system-ui, sans-serif';
// Brand graphite tile and the tile's text tiers (DESIGN.md, tokens.css).
const TILE_STOPS: [number, string][] = [
  [0, "#262628"],
  [0.55, "#0f0f10"],
  [1, "#040404"],
];
const TEXT = "#ffffff";
const TEXT_2 = "#cacad0";
const TEXT_3 = "#a3a3aa";
const CELL_ALPHA = [0.09, 0.3, 0.52, 0.76, 1] as const;

async function loadFonts(): Promise<void> {
  if (!document.fonts) return;
  await Promise.all([400, 600, 720].map((weight) => document.fonts.load(`${weight} 40px "Inter Variable"`).catch(() => undefined)));
}

function loadImage(url: string): Promise<HTMLImageElement | null> {
  return new Promise((resolve) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => resolve(null);
    image.src = url;
  });
}

function drawLockup(context: CanvasRenderingContext2D, x: number, y: number, height: number): void {
  const mark = new Path2D(MARK_PATH);
  const word = new Path2D(WORDMARK_PATH);
  context.save();
  context.fillStyle = TEXT;
  context.translate(x, y);
  context.scale(height / MARK_SIZE.height, height / MARK_SIZE.height);
  context.fill(mark);
  context.restore();
  const wordHeight = height * 0.86;
  const markWidth = (height * MARK_SIZE.width) / MARK_SIZE.height;
  context.save();
  context.fillStyle = TEXT;
  context.translate(x + markWidth + height * 0.34, y + (height - wordHeight) / 2);
  context.scale(wordHeight / WORDMARK_SIZE.height, wordHeight / WORDMARK_SIZE.height);
  context.fill(word);
  context.restore();
}

function formatDate(key: string): string {
  return parseDay(key).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}

function plural(value: number, word: string): string {
  return `${formatInteger(value)} ${word}${value === 1 ? "" : "s"}`;
}

/** Draws the card into `canvas` (created if absent) at 2× for sharp text when shared. */
export async function renderStatsImage(card: StatsCard, canvas = document.createElement("canvas")): Promise<HTMLCanvasElement> {
  await loadFonts();
  const picture = card.picture ? await loadImage(card.picture) : null;
  canvas.width = CARD_WIDTH * SCALE;
  canvas.height = CARD_HEIGHT * SCALE;
  const context = canvas.getContext("2d");
  if (!context) return canvas;
  context.scale(SCALE, SCALE);
  const background = context.createLinearGradient(0, 0, 0, CARD_HEIGHT);
  for (const [stop, color] of TILE_STOPS) background.addColorStop(stop, color);
  context.fillStyle = background;
  context.fillRect(0, 0, CARD_WIDTH, CARD_HEIGHT);

  drawLockup(context, PADDING, PADDING - 4, 30);
  context.textBaseline = "alphabetic";
  context.textAlign = "right";
  context.fillStyle = TEXT_3;
  context.font = `500 17px ${FONT}`;
  context.fillText(`As of ${formatDate(card.today)}`, CARD_WIDTH - PADDING, PADDING + 20);

  // Heading: the chosen name and picture, or an anonymous title.
  context.textAlign = "left";
  let textX = PADDING;
  const headingY = 168;
  if (picture) {
    const size = 64;
    context.save();
    context.beginPath();
    context.arc(PADDING + size / 2, headingY - 22, size / 2, 0, Math.PI * 2);
    context.clip();
    context.drawImage(picture, PADDING, headingY - 22 - size / 2, size, size);
    context.restore();
    textX = PADDING + size + 20;
  }
  context.fillStyle = TEXT;
  context.font = `720 38px ${FONT}`;
  context.fillText(card.name ? card.name : "My dictation", textX, headingY - 14);
  context.fillStyle = TEXT_2;
  context.font = `500 19px ${FONT}`;
  context.fillText(card.name ? "Dictating with Onda" : "with Onda", textX, headingY + 16);

  // Four headline numbers.
  const { summary } = card;
  const stats: [string, string, string][] = [
    ["Words dictated", formatInteger(summary.words), ""],
    ["Speaking pace", summary.wordsPerMinute ? String(Math.round(summary.wordsPerMinute)) : "—", summary.wordsPerMinute ? "wpm" : ""],
    ["Current streak", formatInteger(summary.currentStreak), summary.currentStreak === 1 ? "day" : "days"],
    ["Longest streak", formatInteger(summary.longestStreak), summary.longestStreak === 1 ? "day" : "days"],
  ];
  const column = (CARD_WIDTH - PADDING * 2) / stats.length;
  stats.forEach(([label, value, unit], index) => {
    const x = PADDING + index * column;
    context.fillStyle = TEXT_3;
    context.font = `560 17px ${FONT}`;
    context.fillText(label, x, 262);
    context.fillStyle = TEXT;
    context.font = `720 52px ${FONT}`;
    context.fillText(value, x, 318);
    if (unit) {
      const width = context.measureText(value).width;
      context.fillStyle = TEXT_2;
      context.font = `600 21px ${FONT}`;
      context.fillText(unit, x + width + 8, 318);
    }
  });

  // A year of activity, one square per day, Monday at the top.
  const grid = activityGrid(card.days, card.today, 52);
  const gap = 4;
  const cell = (CARD_WIDTH - PADDING * 2 - gap * (grid.weeks.length - 1)) / grid.weeks.length;
  const top = 380;
  context.font = `500 14px ${FONT}`;
  context.fillStyle = TEXT_3;
  for (const month of grid.months) context.fillText(month.label, PADDING + month.column * (cell + gap), top - 10);
  grid.weeks.forEach((week, column) => {
    week.forEach((day, row) => {
      if (day.future) return;
      context.fillStyle = `rgba(255, 255, 255, ${CELL_ALPHA[day.level]})`;
      context.beginPath();
      context.roundRect(PADDING + column * (cell + gap), top + row * (cell + gap), cell, cell, 3);
      context.fill();
    });
  });

  const footerY = CARD_HEIGHT - PADDING + 10;
  context.fillStyle = TEXT_3;
  context.font = `500 16px ${FONT}`;
  const since = summary.since ? `Since ${formatDate(summary.since)} · ` : "";
  context.fillText(`${since}${plural(summary.activeDays, "active day")} · ${plural(summary.dictations, "dictation")}`, PADDING, footerY);
  context.textAlign = "right";
  context.fillText("Onda by miille", CARD_WIDTH - PADDING, footerY);
  return canvas;
}

/** Base64 PNG for native code to save; the data-URL prefix is removed. */
export function canvasBase64(canvas: HTMLCanvasElement): string {
  return canvas.toDataURL("image/png").replace(/^data:image\/png;base64,/, "");
}
