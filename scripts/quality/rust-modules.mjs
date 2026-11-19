import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { basename, dirname, join, relative, resolve, sep } from "node:path";
import { testOnly } from "./rust-scope.mjs";

function declarations(node, inline = [], output = []) {
  let explicit;
  let excluded = false;
  for (const child of node.namedChildren) {
    if (child.type === "attribute_item") {
      excluded ||= testOnly(child);
      const path = child.text.match(/^#\[\s*path\s*=\s*"([^"]+)"\s*\]$/);
      if (path) explicit = path[1];
      continue;
    }
    if (child.type.endsWith("comment")) continue;
    if (!excluded && child.type === "mod_item") {
      const name = child.childForFieldName("name").text;
      const body = child.childForFieldName("body");
      if (body) declarations(body, [...inline, explicit ?? name], output);
      else output.push({ inline, explicit, name });
    }
    explicit = undefined;
    excluded = false;
  }
  return output;
}

/** Follow every conditional module from Cargo targets, including modules for other OSes. */
export async function reachableRust(roots, parser) {
  const reached = new Map();
  const queue = roots.map((root) => ({ path: resolve(root), base: dirname(resolve(root)), crate: resolve(root), module: [], ancestors: [] }));
  while (queue.length) {
    const { path, base, crate, module, ancestors } = queue.pop();
    if (ancestors.includes(path)) throw new Error("Cyclic Rust module path " + path);
    const scopes = reached.get(path) ?? [];
    if (scopes.some((scope) => scope.crate === crate && scope.module.join("::") === module.join("::"))) continue;
    scopes.push({ crate, module });
    reached.set(path, scopes);
    const tree = parser.parse(await readFile(path, "utf8"));
    try {
      for (const { inline, explicit, name } of declarations(tree.rootNode)) {
        const directory = join(base, ...inline);
        const named = explicit
          ? [resolve(inline.length ? directory : dirname(path), explicit)]
          : [join(directory, name + ".rs"), join(directory, name, "mod.rs")];
        const file = named.find(existsSync);
        if (file) queue.push({ path: file, base: basename(file) === "mod.rs" ? dirname(file) : join(dirname(file), basename(file, ".rs")),
          crate, module: [...module, ...inline, name], ancestors: [...ancestors, path] });
      }
    } finally { tree.delete(); }
  }
  return reached;
}

export function productionRoots(packages) {
  const shipped = new Set(["lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro", "bin"]);
  return packages.flatMap((pkg) => pkg.targets.filter((target) => target.kind.some((kind) => shipped.has(kind))).map((target) => target.src_path));
}

/** Follow the module declarations of shipped builds. Development tools have separate checks. */
export async function orphanRust(root, files, parser, tooling = new Set()) {
  const packages = JSON.parse(execFileSync("cargo", ["metadata", "--locked", "--no-deps", "--format-version", "1"], { cwd: root, encoding: "utf8" })).packages;
  const roots = productionRoots(packages).filter((path) => !tooling.has(relative(root, path).split(sep).join("/")));
  const reached = await reachableRust(roots, parser);
  for (const file of files) file.scopes = reached.get(resolve(root, file.path));
  return files.filter((file) => file.path.endsWith(".rs") && !reached.has(resolve(root, file.path)))
    .map((file) => relative(root, resolve(root, file.path)).split(sep).join("/"));
}
