import { posix } from "node:path";
import postcss from "postcss";

const linesOfCode = (text) => text.split(/\r?\n/).filter((line) => line.trim()).length;
const importPath = /^(?:"([^"]+)"|'([^']+)'|url\(\s*(?:"([^"]+)"|'([^']+)'|([^\s)]+))\s*\))/i;

/** Parse with Vite's CSS parser so modern selectors and comments inside strings retain their meaning. */
export function measureStyles(path, source) {
  const tree = postcss.parse(source, { from: path });
  const code = [], styles = [];
  let cursor = 0;
  tree.walkComments((comment) => {
    const start = comment.source.start.offset, end = comment.source.end.offset;
    code.push(source.slice(cursor, start), source.slice(start, end).replace(/[^\n]/g, ""));
    cursor = end;
  });
  code.push(source.slice(cursor));
  tree.walkAtRules(/^import$/i, (rule) => {
    const target = rule.params.match(importPath)?.slice(1).find(Boolean);
    if (!target) throw rule.error("Expected a stylesheet reference");
    styles.push(target);
  });
  return { path, lines: linesOfCode(code.join("")), entities: [], styles };
}

/** The two HTML entry points and SQL migrations have a file budget and no callable members. */
export function measureDocument(path, source) {
  const styles = [];
  if (path.endsWith(".html")) {
    for (const [tag] of source.replace(/<!--[\s\S]*?-->/g, "").matchAll(/<link\b[^>]*>/gi)) {
      const rel = tag.match(/\brel\s*=\s*["']([^"']+)["']/i)?.[1];
      const href = tag.match(/\bhref\s*=\s*["']([^"']+)["']/i)?.[1];
      if (href && rel?.toLowerCase().split(/\s+/).includes("stylesheet")) styles.push(href);
    }
  }
  return { path, lines: linesOfCode(source), entities: [], styles };
}

/** O(files + imports) time and space. Production entry modules and HTML link the style graph. */
export function orphanStyles(files) {
  const sheets = new Map(files.filter((file) => file.path.endsWith(".css")).map((file) => [file.path, file]));
  const reached = new Set(), pending = [];
  function enqueue(file, reference) {
    const path = reference.split(/[?#]/, 1)[0];
    if (!path || /^(?:[a-z][a-z\d+.-]*:|\/\/)/i.test(path)) return;
    const candidate = path.startsWith("/") ? posix.join("ui", path) : posix.join(posix.dirname(file), path);
    if (sheets.has(candidate)) pending.push(candidate);
    else if (path.startsWith(".") || path.startsWith("/")) throw new Error("Missing stylesheet " + reference + " imported by " + file);
  }
  for (const file of files) if (!sheets.has(file.path)) {
    for (const reference of file.styles ?? []) enqueue(file.path, reference);
  }
  while (pending.length) {
    const path = pending.pop();
    if (reached.has(path)) continue;
    reached.add(path);
    for (const reference of sheets.get(path).styles) enqueue(path, reference);
  }
  return [...sheets.keys()].filter((path) => !reached.has(path));
}

export function orphanMigrations(files) {
  const referenced = new Set();
  for (const file of files) {
    for (const path of file.migrations ?? []) referenced.add(posix.join(posix.dirname(file.path), path));
  }
  return files.filter((file) => file.path.endsWith(".sql") && !referenced.has(file.path)).map((file) => file.path);
}
