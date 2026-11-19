import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { basename, dirname, join, relative, resolve, sep } from "node:path";

function declarations(node, inline = [], output = []) {
  let explicit;
  for (const child of node.namedChildren) {
    if (child.type === "attribute_item") {
      const path = child.text.match(/^#\[\s*path\s*=\s*"([^"]+)"\s*\]$/);
      if (path) explicit = path[1];
      continue;
    }
    if (child.type.endsWith("comment")) continue;
    if (child.type === "mod_item") {
      const name = child.childForFieldName("name").text;
      const body = child.childForFieldName("body");
      if (body) declarations(body, [...inline, explicit ?? name], output);
      else output.push({ inline, explicit, name });
    }
    explicit = undefined;
  }
  return output;
}

/** Follow every conditional module from Cargo targets, including modules for other OSes. */
export async function reachableRust(roots, parser) {
  const reached = new Set();
  const queue = roots.map((path) => ({ path: resolve(path), base: dirname(resolve(path)) }));
  while (queue.length) {
    const { path, base } = queue.pop();
    if (reached.has(path)) continue;
    reached.add(path);
    const tree = parser.parse(await readFile(path, "utf8"));
    try {
      for (const { inline, explicit, name } of declarations(tree.rootNode)) {
        const directory = join(base, ...inline);
        const named = explicit
          ? [resolve(inline.length ? directory : dirname(path), explicit)]
          : [join(directory, name + ".rs"), join(directory, name, "mod.rs")];
        const file = named.find(existsSync);
        if (file) queue.push({ path: file, base: basename(file) === "mod.rs" ? dirname(file) : join(dirname(file), basename(file, ".rs")) });
      }
    } finally { tree.delete(); }
  }
  return reached;
}

/** O(source bytes + declarations) time. Each reachable source file enters the set once. */
export async function orphanRust(root, files, parser) {
  const packages = JSON.parse(execFileSync("cargo", ["metadata", "--locked", "--no-deps", "--format-version", "1"], { cwd: root, encoding: "utf8" })).packages;
  const roots = packages.flatMap((pkg) => pkg.targets.map((target) => target.src_path));
  const reached = await reachableRust(roots, parser);
  return files.filter((file) => file.path.endsWith(".rs") && !reached.has(resolve(root, file.path)))
    .map((file) => relative(root, resolve(root, file.path)).split(sep).join("/"));
}
