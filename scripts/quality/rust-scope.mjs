const syntax = (text) => text.replace(/\s+/g, "");

/** Evaluate test and debug predicates for a shipped target. OS and architecture stay open. */
function releasePredicate(input) {
  if (["test", "debug_assertions", 'feature="e2e"'].includes(input)) return false;
  const group = /^(all|any|not)\((.*)\)$/.exec(input);
  if (!group) return undefined;
  let level = 0;
  let start = 0;
  const values = [];
  for (let i = 0; i <= group[2].length; i++) {
    const token = group[2][i];
    if ((token === "," && level === 0) || token === undefined) {
      if (i > start) values.push(releasePredicate(group[2].slice(start, i)));
      start = i + 1;
    } else if (token === "(") level++;
    else if (token === ")") level--;
  }
  if (group[1] === "not") return values[0] === undefined ? undefined : !values[0];
  const decisive = group[1] === "any";
  if (values.includes(decisive)) return decisive;
  return values.includes(undefined) ? undefined : !decisive;
}

export function testOnly(attribute) {
  const text = syntax(attribute.text);
  if (/^#\[(?:[a-z_]+::)?test(?:\(|\])/.test(text)) return true;
  const condition = /^#\[cfg\((.*)\)\]$/.exec(text);
  return Boolean(condition && releasePredicate(condition[1]) === false);
}

export function physicalModule(path) {
  const parts = path.split("/src/").at(-1).replace(/\.rs$/, "").split("/");
  if (["lib", "main", "mod"].includes(parts.at(-1))) parts.pop();
  return parts;
}

/** Collect the local aliases and glob imports from a Rust use tree, including nested groups. */
export function importsFrom(node, prefix = [], output = []) {
  if (!node) return output;
  const parts = (part) => part ? syntax(part.text).split("::").filter(Boolean) : [];
  if (node.type === "scoped_use_list") {
    return importsFrom(node.childForFieldName("list"), [...prefix, ...parts(node.childForFieldName("path"))], output);
  }
  if (node.type === "use_list") {
    for (const part of node.namedChildren) importsFrom(part, prefix, output);
    return output;
  }
  const wildcard = node.type === "use_wildcard";
  const alias = node.childForFieldName("alias")?.text;
  const target = node.type === "use_as_clause" ? node.childForFieldName("path") : wildcard ? node.namedChildren[0] : node;
  const imported = [...prefix, ...parts(target)];
  if (imported.at(-1) === "self" && imported.length > 1) imported.pop();
  output.push({ alias: wildcard ? "*" : alias ?? imported.at(-1), target: imported });
  return output;
}

export function absoluteModule(context, reference) {
  const parts = [...reference];
  const scope = parts[0] === "crate" ? [] : [...context];
  if (parts[0] === "crate" || parts[0] === "self") parts.shift();
  while (parts[0] === "super") { scope.pop(); parts.shift(); }
  return [...scope, ...parts].join("::");
}
