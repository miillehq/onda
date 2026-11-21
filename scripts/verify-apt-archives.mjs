import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { createReadStream } from "node:fs";
import { readdir, unlink } from "node:fs/promises";
import { isAbsolute, join } from "node:path";
import { pathToFileURL } from "node:url";

const packageKey = (name, version, architecture) => [name, version, architecture].join("\n");

function archiveIdentity(file) {
  const parts = /^([a-z0-9][a-z0-9.+-]*)_([^_]+)_([a-z0-9-]+)\.deb$/.exec(file);
  if (!parts) return undefined;
  let version;
  try { version = decodeURIComponent(parts[2]); } catch { return undefined; }
  if (!/^[0-9][a-zA-Z0-9.+:~-]*$/.test(version)) return undefined;
  return { query: parts[1] + "=" + version, key: packageKey(parts[1], version, parts[3]) };
}

function knownDigests(records) {
  const digests = new Map();
  for (const paragraph of records.split(/\n\s*\n/)) {
    const fields = new Map();
    for (const line of paragraph.split(/\r?\n/)) {
      const match = /^(Package|Version|Architecture|SHA256):\s*(\S+)\s*$/.exec(line);
      if (match) fields.set(match[1], match[2]);
    }
    const sha = fields.get("SHA256");
    if (!/^[a-f\d]{64}$/i.test(sha ?? "")) continue;
    const key = packageKey(fields.get("Package"), fields.get("Version"), fields.get("Architecture"));
    if (!digests.has(key)) digests.set(key, new Set());
    digests.get(key).add(sha.toLowerCase());
  }
  return digests;
}

async function sha256(path) {
  const digest = createHash("sha256");
  for await (const bytes of createReadStream(path)) digest.update(bytes);
  return digest.digest("hex");
}

/** O(total restored bytes + index records) time, with one file stream in memory at a time. */
export async function verifyArchives(directory, lookup) {
  const files = (await readdir(directory, { withFileTypes: true })).filter((entry) => entry.name.endsWith(".deb") && !entry.isDirectory());
  const archives = files.map((entry) => ({ path: join(directory, entry.name), identity: entry.isFile() ? archiveIdentity(entry.name) : undefined }));
  const queries = [...new Set(archives.flatMap(({ identity }) => identity ? [identity.query] : []))];
  const digests = knownDigests(queries.length ? lookup(queries) : "");
  let kept = 0;
  for (const { path, identity } of archives) {
    const allowed = identity && digests.get(identity.key);
    if (allowed && allowed.has(await sha256(path))) kept++;
    else await unlink(path);
  }
  return { kept, discarded: archives.length - kept };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const temporary = process.env.RUNNER_TEMP;
  if (!temporary || !isAbsolute(temporary)) throw new Error("The Ubuntu package cache belongs under the runner's temporary directory.");
  const report = await verifyArchives(join(temporary, "onda-apt-archives"), (queries) => {
    // A removed package can make apt-cache return nonzero. Its absent digest triggers a fresh download.
    const result = spawnSync("apt-cache", ["show", ...queries], { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 });
    if (result.error) throw result.error;
    return result.stdout;
  });
  console.log("Verified " + report.kept + " cached Ubuntu packages and discarded " + report.discarded + ".");
}
