/** Generate attribution from the exact locked source tree, without fetching third-party prose. */
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, readdirSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
const metadata = JSON.parse(execFileSync("cargo", ["metadata", "--locked", "--format-version", "1"], { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 }));
const sections = ["Onda third-party notices\nGenerated from Cargo.lock and package-lock.json.\nAll-target dependency inventory includes build-only and optional libraries; inclusion here does not imply every library is distributed.\n"];
const inventory = [];
for (const dependency of metadata.packages.filter((dependency) => dependency.source !== null).sort((left, right) => left.name.localeCompare(right.name) || left.version.localeCompare(right.version))) {
  const directory = dirname(dependency.manifest_path);
  const files = readdirSync(directory, { withFileTypes: true }).filter((file) => file.isFile() && /^(LICENSE|LICENCE|COPYING|NOTICE)([._-]|$)/i.test(file.name));
  if (dependency.license_file && existsSync(join(directory, dependency.license_file))) files.push({ name: dependency.license_file });
  const uniqueFiles = [...new Set(files.map((file) => file.name))];
  if (dependency.name === "whisper-rs-sys") {
    const collectNativeNotices = (relative) => {
      const nativePath = join(directory, relative);
      for (const entry of readdirSync(nativePath, { withFileTypes: true })) {
        const child = join(relative, entry.name);
        if (entry.isDirectory()) collectNativeNotices(child);
        else if (entry.isFile() && /^(LICENSE|LICENCE|COPYING|NOTICE)([._-]|$)/i.test(entry.name)) uniqueFiles.push(child);
      }
    };
    collectNativeNotices("whisper.cpp");
  }
  inventory.push({ ecosystem: "cargo", name: dependency.name, version: dependency.version, license: dependency.license, source: dependency.source, noticeFiles: uniqueFiles });
  sections.push(`\n${"=".repeat(72)}\n${dependency.name} ${dependency.version}\nLicense: ${dependency.license ?? "see license file"}\nSource: ${dependency.source}\n`);
  for (const filename of uniqueFiles) sections.push(`\n--- ${filename} ---\n${readFileSync(join(directory, filename), "utf8")}\n`);
}
const nodeLock = JSON.parse(readFileSync("package-lock.json", "utf8"));
for (const [path, dependency] of Object.entries(nodeLock.packages).filter(([path]) => path.startsWith("node_modules/")).sort(([left], [right]) => left.localeCompare(right))) {
  if (!existsSync(path)) continue;
  const name = dependency.name ?? path.replace(/^node_modules\//, "");
  const files = readdirSync(path, { withFileTypes: true }).filter((file) => file.isFile() && /^(LICENSE|LICENCE|COPYING|NOTICE)([._-]|$)/i.test(file.name));
  inventory.push({ ecosystem: "npm", name, version: dependency.version, license: dependency.license, noticeFiles: files.map((file) => file.name) });
  sections.push(`\n${"=".repeat(72)}\n${name} ${dependency.version}\nLicense: ${dependency.license ?? "see license file"}\n`);
  for (const file of files) sections.push(`\n--- ${file.name} ---\n${readFileSync(join(path, file.name), "utf8")}\n`);
}
writeFileSync("THIRD_PARTY_NOTICES.txt", sections.join(""));
writeFileSync("docs/dependency-inventory.json", JSON.stringify(inventory, null, 2) + "\n");
console.log(`Collected notices for ${inventory.length} locked dependencies.`);
