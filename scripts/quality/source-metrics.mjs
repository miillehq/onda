import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import treeSitter from "@vscode/tree-sitter-wasm";
import { importsFrom, testOnly } from "./rust-scope.mjs";
export { aggregateTypes } from "./type-ownership.mjs";

const { Parser, Language } = treeSitter;
const wasmDirectory = dirname(createRequire(import.meta.url).resolve("@vscode/tree-sitter-wasm"));
const grammars = { ".rs": "rust", ".ts": "typescript", ".tsx": "tsx" };
const functions = new Set(["function_item", "closure_expression", "function_declaration", "function_expression", "arrow_function", "method_definition"]);
const comments = new Set(["line_comment", "block_comment", "comment"]);
const field = (node, name) => node.childForFieldName(name)?.text;

export async function createParsers() {
  await Parser.init({ locateFile: (file) => join(wasmDirectory, file) });
  const parsers = new Map();
  for (const [extension, name] of Object.entries(grammars)) {
    const parser = new Parser();
    parser.setLanguage(await Language.load(join(wasmDirectory, `tree-sitter-${name}.wasm`)));
    parsers.set(extension, parser);
  }
  return parsers;
}

function ignoredItems(root) {
  const excluded = new Set();
  function visit(parent) {
    let attributes = [];
    for (const child of parent.namedChildren) {
      if (child.type === "attribute_item") { attributes.push(child); continue; }
      if (comments.has(child.type)) continue;
      if (attributes.some(testOnly)) {
        excluded.add(child.id);
        for (const attribute of attributes) excluded.add(attribute.id);
      } else { visit(child); }
      attributes = [];
    }
  }
  visit(root);
  return excluded;
}

function functionName(node) {
  const identifier = field(node, "name") ?? field(node.parent ?? node, "name") ?? field(node.parent ?? node, "key");
  if (identifier) return identifier;
  for (let parent = node.parent; parent; parent = parent.parent) {
    if (functions.has(parent.type) && field(parent, "name")) return `${field(parent, "name")} callback`;
  }
  return "callback";
}

function methods(node, excluded) {
  const body = node.childForFieldName("body");
  if (node.type === "class_declaration" || node.type === "interface_declaration") {
    return (body?.namedChildren ?? []).filter((item) => ["method_definition", "public_field_definition", "property_signature", "method_signature", "abstract_method_signature"].includes(item.type))
      .filter((item) => !item.namedChildren.some((child) => child.type === "accessibility_modifier" && child.text !== "public"))
      .map((item) => field(item, "name")).filter((name) => name && !name.startsWith("#"));
  }
  return (body?.namedChildren ?? []).filter((item) => !excluded.has(item.id) && (item.type === "function_item" || item.type === "field_declaration"))
    .filter((item) => item.namedChildren.some((child) => child.type === "visibility_modifier" && child.text === "pub")
      || (item.type === "function_item" && Boolean(node.childForFieldName("trait"))))
    .map((item) => field(item, "name")).filter(Boolean);
}

/** O(syntax nodes + source lines), with O(source lines + declarations) working storage per file. */
export function measureSource(path, source, parser) {
  const tree = parser.parse(source);
  if (!tree || tree.rootNode.hasError) throw new Error(`Could not parse production source ${path}.`);
  try {
    const lines = source.split(/\r?\n/);
    const code = new Uint8Array(lines.length);
    const logic = new Uint8Array(lines.length);
    const markup = new Uint8Array(lines.length);
    const excluded = path.endsWith(".rs") ? ignoredItems(tree.rootNode) : new Set();
    const entities = [];
    const imports = [];
    const styles = [];
    const migrations = [];
    function visit(node, owner = "", inMarkup = false, module = []) {
      if (excluded.has(node.id) || comments.has(node.type)) return;
      const start = node.startPosition.row;
      const end = node.endPosition.row - Number(node.endPosition.column === 0 && node.endPosition.row > start);
      let nextOwner = owner;
      let childModule = module;
      if (node.type === "mod_item" && node.childForFieldName("body")) childModule = [...module, field(node, "name")];
      const importedFile = node.type === "import_statement" ? node.childForFieldName("source")
        : node.type === "call_expression" && node.childForFieldName("function")?.text === "import"
          ? node.childForFieldName("arguments")?.namedChildren[0] : undefined;
      if (importedFile?.type === "string" && /\.css(?:[?#][^'"]*)?['"]$/i.test(importedFile.text)) styles.push(importedFile.text.slice(1, -1));
      if (node.type === "macro_invocation" && node.childForFieldName("macro")?.text === "include_str") {
        const literal = node.namedChildren.find((child) => child.type === "token_tree")?.namedChildren[0];
        const path = literal?.namedChildren.find((child) => child.type === "string_content")?.text;
        if (path?.endsWith(".sql")) migrations.push(path);
      }
      if (node.type === "use_declaration") {
        for (const imported of importsFrom(node.childForFieldName("argument"))) imports.push({ ...imported, module });
      } else if (node.type === "impl_item") {
        const reference = (field(node, "type") ?? "").split("<")[0].replace(/\s/g, "").split("::");
        nextOwner = reference.at(-1);
        entities.push({ kind: "impl", name: nextOwner, reference, module, start, end, members: methods(node, excluded) });
      } else if (["struct_item", "class_declaration", "interface_declaration"].includes(node.type)) {
        entities.push({ kind: "type", name: field(node, "name"), module, start, end, members: methods(node, excluded) });
      } else if (functions.has(node.type)) {
        const name = functionName(node);
        const entity = { kind: "function", name: owner ? `${owner}::${name}` : name, start, end };
        if (entity.name === "Runtime::dispatch") {
          const arms = node.descendantsOfType("match_arm");
          entity.routes = arms.length;
          entity.routeBranches = arms.filter((arm) => arm.descendantsOfType(["if_expression", "match_expression", "while_expression", "for_expression", "loop_expression"]).length > 0).length;
        }
        entities.push(entity);
      }
      const jsx = node.type.startsWith("jsx_") ? node.type !== "jsx_expression" : inMarkup;
      if (node.childCount) {
        for (const child of node.children) visit(child, nextOwner, jsx, childModule);
      } else {
        for (let row = start; row <= end; row++) {
          if (!lines[row]?.trim()) continue;
          code[row] = 1;
          (jsx ? markup : logic)[row] = 1;
        }
      }
    }
    visit(tree.rootNode);
    function totals(rows) {
      const sum = new Uint32Array(rows.length + 1);
      for (let i = 0; i < rows.length; i++) sum[i + 1] = sum[i] + rows[i];
      return sum;
    }
    const prefix = totals(code), executable = totals(logic), jsx = totals(markup);
    return {
      path, lines: prefix.at(-1), imports, styles, migrations,
      entities: entities.map(({ start, end, ...entity }) => ({ ...entity, line: start + 1, lines: prefix[end + 1] - prefix[start],
        logic: executable[end + 1] - executable[start], markup: jsx[end + 1] - jsx[start] })),
    };
  } finally { tree.delete(); }
}
