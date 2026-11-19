import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { violations } from "../../scripts/quality/architecture.mjs";
import { productionRoots, reachableRust } from "../../scripts/quality/rust-modules.mjs";
import { aggregateTypes, createParsers, measureSource } from "../../scripts/quality/source-metrics.mjs";

let parsers;
before(async () => { parsers = await createParsers(); });
after(() => { for (const parser of parsers.values()) parser.delete(); });

test("inherent implementations share a type budget across files and OS conditions", () => {
  const measure = (file, body) => measureSource("crates/sample/src/" + file, body, parsers.get(".rs"));
  const methods = (prefix) => Array.from({ length: 70 }, (_, i) => `pub fn ${prefix}${i}(&self) {\nlet _ = self;\n}`).join("\n");
  const findings = violations([
    measure("lib.rs", "pub struct Owner;"),
    measure("desktop.rs", `#[cfg(windows)]\nimpl Owner {\n${methods("desktop")}\n}`),
    measure("model.rs", `impl Owner {\n${methods("model")}\n}`),
  ]);
  assert.deepEqual(findings.map(({ kind }) => kind).sort(), ["members", "type"]);
  assert.equal(findings.find(({ kind }) => kind === "members").actual, 140);
});

test("JSX markup has its own bound and imperative lines retain the ordinary function limit", () => {
  const measure = (body) => violations([measureSource("ui/src/View.tsx", body, parsers.get(".tsx"))]);
  const markup = Array(100).fill("<li>row</li>").join("\n");
  assert.deepEqual(measure(`function View() { return <ul>\n${markup}\n</ul>; }`), []);
  const code = Array(81).fill("consume();").join("\n");
  assert.ok(measure(`function View() {\n${code}\nreturn <main/>; }`).some((item) => item.kind === "view logic"));
  assert.ok(measure(`function View() { return <ul>\n${markup}\n${markup}\n</ul>; }`).some((item) => item.kind === "function"));
});

test("the Rust module graph follows an off-platform module and its explicit file path", async () => {
  const root = await mkdtemp(join(tmpdir(), "onda-source-test-"));
  try {
    await mkdir(join(root, "port"));
    const path = (name) => join(root, name);
    await writeFile(path("lib.rs"), "#[cfg(target_os=\"macos\")]\nmod port;\n#[cfg(all(windows,test))]\nmod lost;");
    await writeFile(path("port.rs"), "mod worker;\n#[path=\"identity.rs\"] mod named;");
    await writeFile(path("port/worker.rs"), "pub struct Worker;");
    await writeFile(path("identity.rs"), "pub struct Identity;");
    await writeFile(path("lost.rs"), "pub struct Lost;");
    const reached = await reachableRust([path("lib.rs")], parsers.get(".rs"));
    for (const name of ["port.rs", "identity.rs", "port/worker.rs"]) assert.ok(reached.has(path(name)));
    assert.equal(reached.has(path("lost.rs")), false);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test("Cargo test, build, example and benchmark targets cannot make unused production code reachable", () => {
  const kinds = ["lib", "bin", "test", "example", "bench", "custom-build"];
  const targets = kinds.map((kind) => ({ kind: [kind], src_path: kind + ".rs" }));
  assert.deepEqual(productionRoots([{ targets }]), ["lib.rs", "bin.rs"]);
});

test("qualified implementations attach to the right same-named type and exclude test-only members", () => {
  const measure = (file, source) => measureSource("crates/example/src/" + file, source, parsers.get(".rs"));
  const methods = (count) => Array.from({ length: count }, (_, i) => `pub fn f${i}(&self) {}`).join("\n");
  const files = [
    measure("a.rs", "pub struct Counter;"),
    measure("b.rs", "pub struct Counter;"),
    measure("extra.rs", `impl crate::a::Counter {\n${methods(26)}\n}\nimpl crate::b::Counter {\n${methods(25)}\n#[cfg(test)] pub fn fixture(&self) {}\n}`),
  ];
  const owners = aggregateTypes(files);
  assert.equal(owners.find((item) => item.path.endsWith("/a.rs")).members, 26);
  assert.equal(owners.find((item) => item.path.endsWith("/b.rs")).members, 25);
  assert.deepEqual(violations(files).map((item) => item.path), ["crates/example/src/a.rs"]);
});
