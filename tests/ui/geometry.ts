// Tier 2 of the visual check: layout geometry. Every element marked with a
// data-geo attribute reports its rounded bounding box. The snapshot is one
// JSON file shared by all OSes and both engines, compared with a tolerance,
// so it catches "the form collapsed on Linux" without failing on the one-pixel
// font differences that per-OS pixel baselines are there to absorb.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import type { Page } from "@playwright/test";

export type Box = { x: number; y: number; w: number; h: number };
export type Geometry = Record<string, Box>;

export async function captureGeometry(page: Page): Promise<Geometry> {
  return page.evaluate(() => {
    const out: Record<string, { x: number; y: number; w: number; h: number }> = {};
    for (const el of document.querySelectorAll<HTMLElement>("[data-geo]")) {
      const r = el.getBoundingClientRect();
      out[el.dataset.geo!] = {
        x: Math.round(r.x),
        y: Math.round(r.y),
        w: Math.round(r.width),
        h: Math.round(r.height),
      };
    }
    return out;
  });
}

export type GeometryDiff = { key: string; field: keyof Box; expected: number; actual: number }[];

export function compareGeometry(expected: Geometry, actual: Geometry, tolerance: number): GeometryDiff {
  const diffs: GeometryDiff = [];
  for (const key of Object.keys(expected)) {
    const e = expected[key];
    const a = actual[key];
    if (!a) {
      diffs.push({ key, field: "x", expected: e.x, actual: Number.NaN });
      continue;
    }
    for (const field of ["x", "y", "w", "h"] as const) {
      if (Math.abs(e[field] - a[field]) > tolerance) {
        diffs.push({ key, field, expected: e[field], actual: a[field] });
      }
    }
  }
  for (const key of Object.keys(actual)) {
    if (!expected[key]) diffs.push({ key, field: "x", expected: Number.NaN, actual: actual[key].x });
  }
  return diffs;
}

/**
 * Compares against the committed snapshot. A missing snapshot is written and
 * reported, never silently accepted: the test fails with a message telling
 * the author to commit it. With UPDATE_SNAPSHOTS=1 the file is rewritten.
 */
export function assertGeometry(file: string, actual: Geometry, tolerance = 8): void {
  const update = process.env.UPDATE_SNAPSHOTS === "1";
  if (!existsSync(file) || update) {
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, JSON.stringify(actual, null, 2) + "\n");
    if (!update) throw new Error(`Geometry snapshot was missing and has been written: ${file}. Review and commit it.`);
    return;
  }
  const expected = JSON.parse(readFileSync(file, "utf8")) as Geometry;
  const diffs = compareGeometry(expected, actual, tolerance);
  if (diffs.length) {
    const lines = diffs.map((d) => `  ${d.key}.${d.field}: expected ${d.expected}, got ${d.actual}`);
    throw new Error(`Layout geometry drifted beyond ${tolerance}px:\n${lines.join("\n")}`);
  }
}
