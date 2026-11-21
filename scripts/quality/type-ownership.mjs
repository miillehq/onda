import { absoluteModule, physicalModule } from "./rust-scope.mjs";

function scopes(file) {
  return file.scopes ?? [{ crate: file.path.split("/src/")[0], module: physicalModule(file.path) }];
}

function addPart(owner, entity) {
  owner.lines += entity.lines;
  for (const name of entity.members) owner.members.add(name);
}

function ownerOf(owners, byName, scope, entity, imports) {
  const reference = entity.reference ?? [entity.name];
  const module = [...scope.module, ...entity.module];
  const inCrate = (path) => owners.get(scope.crate + "::" + path);
  const direct = inCrate(absoluteModule(module, reference));
  if (direct) return direct;
  if (["crate", "self", "super"].includes(reference[0])) return undefined;
  const candidates = new Set();
  for (const item of imports) {
    if (item.module.join("::") !== entity.module.join("::")) continue;
    if (item.alias !== "*" && item.alias !== reference[0]) continue;
    const suffix = item.alias === "*" ? reference : reference.slice(1);
    const imported = [...item.target, ...suffix];
    const owner = inCrate(absoluteModule(module, imported)) ?? inCrate(absoluteModule([], imported));
    if (owner) candidates.add(owner);
  }
  if (!candidates.size && reference.length === 1) {
    for (const owner of byName.get(scope.crate + "::" + entity.name) ?? []) candidates.add(owner);
  }
  if (candidates.size > 1) throw new Error("Ambiguous implementation of " + reference.join("::") + " in " + module.join("::"));
  return candidates.values().next().value ?? inCrate(absoluteModule([], reference));
}

/** Index declarations by crate and logical module before assigning implementations from any file. */
export function aggregateTypes(files) {
  const owners = new Map(), byName = new Map();
  for (const file of files) {
    for (const scope of scopes(file)) {
      for (const entity of file.entities.filter((item) => item.kind === "type")) {
        const path = [...scope.module, ...entity.module, entity.name].join("::");
        const key = scope.crate + "::" + path;
        const owner = owners.get(key) ?? { path: file.path, name: entity.name, line: entity.line, lines: 0, members: new Set() };
        addPart(owner, entity);
        owners.set(key, owner);
        const name = scope.crate + "::" + entity.name;
        if (!byName.has(name)) byName.set(name, new Set());
        byName.get(name).add(owner);
      }
    }
  }
  for (const file of files) {
    for (const scope of scopes(file)) {
      for (const entity of file.entities.filter((item) => item.kind === "impl")) {
        const owner = ownerOf(owners, byName, scope, entity, file.imports ?? []);
        if (owner) addPart(owner, entity);
      }
    }
  }
  return [...owners.values()].map((owner) => ({ ...owner, members: owner.members.size }));
}
