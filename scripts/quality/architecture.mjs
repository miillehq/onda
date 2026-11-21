import { readFile, readdir } from "node:fs/promises";
import { extname, join, relative, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { aggregateTypes, createParsers, measureSource } from "./source-metrics.mjs";
import { orphanRust } from "./rust-modules.mjs";

const root = fileURLToPath(new URL("../../", import.meta.url));
const limits = { file: 400, function: 80, type: 400, members: 25 };
// ADR 008 names the wire and storage interfaces whose callers need stable, flat contracts.
const declaredLimits = new Map([
  ["members crates/onda-core/src/contracts.rs Settings", 40],
  ["members crates/onda-core/src/persistence.rs Store", 32],
  ["function src-tauri/src/runtime/commands.rs Runtime::dispatch", 180],
]);
const presentationLines = 160;
const tooling = new Set(["crates/onda-asr/src/bin/benchmark.rs", "crates/onda-core/src/bin/quality.rs",
  "crates/onda-core/src/bin/contracts.rs", "crates/onda-platform/src/debug_fixture.rs", "ui/src/vite-env.d.ts"]);

async function filesUnder(directory, parsers) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const child = join(directory, entry.name);
    const path = relative(root, child).split(sep).join("/");
    if (entry.isDirectory()) {
      if (!["tests", "fixtures", "generated", "mock"].includes(entry.name)) files.push(...await filesUnder(child, parsers));
    } else if (parsers.has(extname(child)) && !/(?:^|_)tests\.rs$/.test(entry.name) && !tooling.has(path)) {
      files.push(measureSource(path, await readFile(child, "utf8"), parsers.get(extname(child))));
    }
  }
  return files;
}

export function violations(files) {
  const findings = [];
  function inspect(kind, path, line, name, actual, defaultLimit = limits[kind]) {
    const limit = declaredLimits.get(`${kind} ${path} ${name}`) ?? defaultLimit;
    if (actual > limit) findings.push({ kind, path, line, name, actual, limit });
  }
  for (const file of files) {
    inspect("file", file.path, 1, "", file.lines);
    for (const entity of file.entities.filter((item) => item.kind === "function")) {
      const isView = file.path.endsWith(".tsx") && /^[A-Z]\w*$/.test(entity.name) && entity.markup > 0;
      inspect("function", file.path, entity.line, entity.name, entity.lines, isView ? presentationLines : limits.function);
      if (isView) inspect("view logic", file.path, entity.line, entity.name, entity.logic, limits.function);
      if (entity.routeBranches !== undefined) inspect("branches inside dispatch arms", file.path, entity.line, entity.name, entity.routeBranches, 0);
    }
  }
  for (const type of aggregateTypes(files)) {
    inspect("type", type.path, type.line, type.name, type.lines);
    inspect("members", type.path, type.line, type.name, type.members);
  }
  return findings.sort((a, b) => a.path.localeCompare(b.path) || a.line - b.line || a.kind.localeCompare(b.kind));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const parsers = await createParsers();
  const crates = (await readdir(join(root, "crates"), { withFileTypes: true })).filter((entry) => entry.isDirectory());
  const roots = ["ui/src", "src-tauri/src", ...crates.map(({ name }) => `crates/${name}/src`)];
  const files = [];
  for (const directory of roots) files.push(...await filesUnder(join(root, directory), parsers));
  const unreachable = await orphanRust(root, files, parsers.get(".rs"), tooling);
  const findings = violations(files);
  for (const path of unreachable) {
    findings.push({ kind: "unreachable Rust module", path, line: 1, name: "", actual: 1, limit: 0 });
  }
  if (process.argv.includes("--stats")) {
    const types = aggregateTypes(files);
    const largest = files.toSorted((a, b) => b.lines - a.lines)[0];
    console.log(JSON.stringify({ files: files.length, largest: { path: largest.path, lines: largest.lines },
      types: types.filter((item) => ["Runtime", "Settings", "Store", "ChatGptService", "MicrophoneSession", "RewriteEngine"].includes(item.name)),
      findings }, null, 2));
  } else if (process.argv.includes("--json")) console.log(JSON.stringify(findings));
  else {
    for (const item of findings) console.log(`${item.path}:${item.line} ${item.kind} ${item.name} ${item.actual}/${item.limit}`);
    console.log(`${files.length} production files, ${findings.length} source limits exceeded.`);
  }
  for (const parser of parsers.values()) parser.delete();
  process.exitCode = Number(findings.length > 0);
}
