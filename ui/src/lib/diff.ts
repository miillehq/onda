import type { DiffSegment } from "../generated/core";

/** Above this many word pairs the comparison shows a whole replacement instead. */
const MAX_CELLS = 1_000_000;

/** Word-level changes from `before` to `after`; whitespace runs count as words. */
export function wordDiff(before: string, after: string): DiffSegment[] {
  const a = before.split(/(\s+)/).filter(Boolean);
  const b = after.split(/(\s+)/).filter(Boolean);
  const segments: DiffSegment[] = [];
  const push = (kind: DiffSegment["kind"], text: string) => {
    const last = segments[segments.length - 1];
    if (last?.kind === kind) last.text += text;
    else segments.push({ kind, text });
  };
  if (a.length * b.length > MAX_CELLS) {
    if (before) push("removed", before);
    if (after) push("added", after);
    return segments;
  }
  // Longest common subsequence, filled from the end so the walk below goes forward.
  const width = b.length + 1;
  const common = new Uint32Array((a.length + 1) * width);
  const at = (i: number, j: number) => common[i * width + j] ?? 0;
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      common[i * width + j] = a[i] === b[j] ? at(i + 1, j + 1) + 1 : Math.max(at(i + 1, j), at(i, j + 1));
    }
  }
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      push("equal", a[i++] ?? "");
      j++;
    } else if (at(i + 1, j) >= at(i, j + 1)) {
      push("removed", a[i++] ?? "");
    } else {
      push("added", b[j++] ?? "");
    }
  }
  while (i < a.length) push("removed", a[i++] ?? "");
  while (j < b.length) push("added", b[j++] ?? "");
  return segments;
}
