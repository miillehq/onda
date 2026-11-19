import { parse } from "parse5";

const SQL_QUOTES = new Set(["'", '"', "`", "["]);

/** Preserve line positions while excluding parser-reported comments and keeping all other tokens. */
export function countOutsideComments(source, ranges) {
  let cursor = 0;
  const code = [];
  for (const [start, end] of ranges.sort((a, b) => a[0] - b[0])) {
    code.push(source.slice(cursor, start), source.slice(start, end).replace(/[^\n]/g, ""));
    cursor = end;
  }
  code.push(source.slice(cursor));
  return code.join("").split(/\r?\n/).filter((line) => line.trim()).length;
}

/** O(source bytes) time and constant auxiliary space. SQLite quotes protect comment delimiters. */
function sqlLines(source) {
  let quote = "", block = false, lineComment = false, code = false, lines = 0;
  for (let i = 0; i < source.length; i++) {
    const char = source[i], next = source[i + 1];
    if (char === "\n") { lines += Number(code); code = false; lineComment = false; continue; }
    if (lineComment) continue;
    if (block) {
      if (char === "*" && next === "/") { block = false; i++; }
      continue;
    }
    if (quote) {
      if (char.trim()) code = true;
      if (char === quote) {
        if (quote !== "]" && next === quote) i++;
        else quote = "";
      }
      continue;
    }
    if (char === "-" && next === "-") { lineComment = true; i++; continue; }
    if (char === "/" && next === "*") { block = true; i++; continue; }
    if (SQL_QUOTES.has(char)) quote = char === "[" ? "]" : char;
    if (char.trim()) code = true;
  }
  return lines + Number(code);
}

export function measureDocument(path, source) {
  if (path.endsWith(".sql")) return { path, lines: sqlLines(source), entities: [], styles: [] };
  const styles = [], comments = [], pending = [parse(source, { sourceCodeLocationInfo: true })];
  while (pending.length) {
    const node = pending.pop();
    if (node.nodeName === "#comment") {
      comments.push([node.sourceCodeLocation.startOffset, node.sourceCodeLocation.endOffset]);
    } else if (node.tagName === "link") {
      const attributes = new Map(node.attrs.map(({ name, value }) => [name, value]));
      if (attributes.has("href") && attributes.get("rel")?.toLowerCase().split(/\s+/).includes("stylesheet")) styles.push(attributes.get("href"));
    }
    if (node.content) pending.push(node.content);
    for (const child of node.childNodes ?? []) pending.push(child);
  }
  return { path, lines: countOutsideComments(source, comments), entities: [], styles };
}
