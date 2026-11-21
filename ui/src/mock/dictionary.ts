import { findNodeAtLocation, getNodeValue, parseTree, type Node, type ParseError } from "jsonc-parser";
import type { ApiRequest, ApiResponse } from "../generated/api";
import { LIMITS, type AppError, type DictionaryEntry } from "../generated/core";
import { DUPLICATE_TRIGGER, lookupKey, validateDictionaryEntry } from "./validate";

type DictionaryRequest = Extract<ApiRequest, { command: "getDictionary" | "saveDictionaryEntry" | "deleteDictionaryEntry" | "importDictionary" | "exportDictionary" }>;

function invalid(message: string): AppError {
  return { code: "invalidInput", message, retryable: false };
}

function lineAt(source: string, offset: number): number {
  let line = 1;
  for (let i = 0; i < offset; i++) if (source.charCodeAt(i) === 10) line++;
  return line;
}

const entryFields = new Set(["id", "phrase", "replacement", "enabled", "starred", "source", "kind"]);
const archiveFields = new Set(["version", "entries"]);

function hasKnownFields(node: Node, allowed: ReadonlySet<string>): boolean {
  if (node.type !== "object") return false;
  const seen = new Set<string>();
  for (const property of node.children ?? []) {
    const name: unknown = property.children?.[0]?.value;
    if (typeof name !== "string" || !allowed.has(name) || seen.has(name)) return false;
    seen.add(name);
  }
  return true;
}

function decodeEntry(node: Node): DictionaryEntry | undefined {
  if (!hasKnownFields(node, entryFields)) return undefined;
  const raw = getNodeValue(node) as Record<string, unknown>;
  const { id, phrase, kind, source, enabled, starred, replacement = null } = raw;
  if (typeof id !== "string" || typeof phrase !== "string" || typeof enabled !== "boolean" || typeof starred !== "boolean"
      || (kind !== "replacement" && kind !== "vocabulary") || (replacement !== null && typeof replacement !== "string")
      || (source !== "manual" && source !== "imported" && source !== "autoLearned")) return undefined;
  return { id, phrase, kind, source, enabled, starred, replacement };
}

/** Stage the entire archive before publishing an updated mock dictionary, as SQLite does. */
function importedEntries(source: string, current: DictionaryEntry[]): DictionaryEntry[] {
  if (new TextEncoder().encode(source).length > LIMITS.maxDictionaryArchiveBytes) {
    throw invalid("Dictionary archive exceeds the maximum backup size.");
  }
  if (!source.trimStart().startsWith("{")) throw invalid("Dictionary archive must be a versioned JSON object.");
  const errors: ParseError[] = [];
  const root = parseTree(source, errors, { disallowComments: true, allowTrailingComma: false });
  const records = root && findNodeAtLocation(root, ["entries"]);
  const version = root && findNodeAtLocation(root, ["version"]);
  if (!root || errors.length || !hasKnownFields(root, archiveFields) || records?.type !== "array" || version?.type !== "number") {
    throw invalid(`Dictionary archive is malformed at line ${lineAt(source, errors[0]?.offset ?? root?.offset ?? 0)}.`);
  }
  if (version.value !== 1) throw invalid("Unsupported dictionary archive version.");
  const rows = records.children ?? [];
  if (rows.length > LIMITS.maxDictionaryEntries) {
    throw invalid(`Dictionary import rejected all ${rows.length} entries. The limit is ${LIMITS.maxDictionaryEntries}.`);
  }
  const known = new Set(current.map((entry) => lookupKey(entry.phrase)));
  const additions: DictionaryEntry[] = [];
  for (const [index, row] of rows.entries()) {
    const entry = decodeEntry(row);
    if (!entry) throw invalid(`Dictionary entry ${index + 1} is malformed at line ${lineAt(source, row.offset)}.`);
    const refusal = validateDictionaryEntry(entry);
    if (refusal) throw invalid(`Dictionary entry ${index + 1} at line ${lineAt(source, row.offset)}. ${refusal.message}`);
    const key = lookupKey(entry.phrase);
    if (known.has(key)) continue;
    if (known.size === LIMITS.maxDictionaryEntries) throw invalid("Import would exceed the dictionary entry limit.");
    known.add(key);
    additions.push({ ...entry, id: crypto.randomUUID(), source: "imported" });
  }
  return additions;
}

/** The labelled mock's dictionary uses the desktop contract and owns its demo state. */
export function createDictionary(initial: DictionaryEntry[]): (request: DictionaryRequest) => ApiResponse {
  let entries = initial;
  return (request) => {
    switch (request.command) {
      case "getDictionary": return { type: "dictionary", payload: structuredClone(entries) };
      case "saveDictionaryEntry": {
        const saved = request.args.entry;
        const refusal = validateDictionaryEntry(saved);
        if (refusal) throw refusal;
        const key = lookupKey(saved.phrase);
        if (entries.some((entry) => entry.id !== saved.id && lookupKey(entry.phrase) === key)) throw DUPLICATE_TRIGGER;
        if (entries.length === LIMITS.maxDictionaryEntries && !entries.some((entry) => entry.id === saved.id)) {
          throw invalid("Dictionary entry limit reached.");
        }
        entries = [...entries.filter((entry) => entry.id !== saved.id), structuredClone(saved)];
        return { type: "dictionary", payload: structuredClone(entries) };
      }
      case "deleteDictionaryEntry":
        entries = entries.filter((entry) => entry.id !== request.args.id);
        return { type: "unit" };
      case "importDictionary": {
        const additions = importedEntries(request.args.json, entries);
        entries = entries.concat(additions);
        return { type: "count", payload: additions.length };
      }
      case "exportDictionary": return { type: "text", payload: JSON.stringify({ version: 1, entries }, null, 2) };
    }
  };
}
