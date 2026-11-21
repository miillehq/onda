import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { constants, createReadStream } from "node:fs";
import { access, copyFile, mkdir, readFile, readdir } from "node:fs/promises";
import { basename, join, resolve } from "node:path";

const formats = {
  "windows-x64": [["nsis", "x64-setup.exe"]], "windows-arm64": [["nsis", "arm64-setup.exe"]],
  "linux-x64": [["deb", "amd64.deb"], ["appimage", "amd64.AppImage"]],
  "linux-arm64": [["deb", "arm64.deb"], ["appimage", "aarch64.AppImage"]],
  "macos-x64": [["dmg", "x64.dmg"]], "macos-arm64": [["dmg", "aarch64.dmg"]],
};

export function expectedBundles(version, platform) {
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version) || !Object.hasOwn(formats, platform)) {
    throw new Error("Expected a release version and a supported platform.");
  }
  return formats[platform].map(([directory, suffix]) => ({ directory, name: `Onda_${version}_${suffix}` }));
}

/** Match each platform's exact bundle set, including duplicate basenames in separate directories. */
export function verifyNames(version, platform, files) {
  const expected = expectedBundles(version, platform).map(({ name }) => name).sort();
  const actual = files.map((file) => basename(file)).sort();
  if (actual.length !== expected.length || actual.some((name, i) => name !== expected[i])) {
    throw new Error(`Unexpected ${platform} packages. Expected ${expected.join(", ")}. Found ${actual.join(", ")}.`);
  }
}

async function collect(version, downloads, assets) {
  await mkdir(assets, { recursive: true });
  for (const platform of Object.keys(formats)) {
    const root = join(downloads, platform);
    const files = (await readdir(root, { recursive: true })).filter((path) => /(?:-setup\.exe|\.deb|\.AppImage|\.dmg)$/.test(path));
    verifyNames(version, platform, files);
    for (const file of files) await copyFile(join(root, file), join(assets, basename(file)), constants.COPYFILE_EXCL);
  }
}

function gh(args) {
  const repo = process.env.GITHUB_REPOSITORY;
  const result = spawnSync("gh", [...args, ...(repo ? ["--repo", repo] : [])], { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`GitHub release command failed with exit ${result.status}.`);
  return result.stdout;
}

async function sha256(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest("hex");
}

async function download(tag, asset, directory) {
  const path = join(directory, asset.name);
  await mkdir(directory, { recursive: true });
  if (!(await access(path).then(() => true, () => false))) {
    gh(["release", "download", tag, "--pattern", asset.name, "--dir", directory]);
  }
  const digest = await sha256(path);
  if (asset.digest !== `sha256:${digest}`) throw new Error(`GitHub digest mismatch for ${asset.name}.`);
  return { path, digest };
}

/** Downloads are read-only GitHub operations. Verify both published digests before any installer runs. */
async function fetchPublished(tag, platform, root) {
  if (!tag?.startsWith("v")) throw new Error("Expected a release tag.");
  const bundles = expectedBundles(tag.slice(1), platform);
  const release = JSON.parse(gh(["release", "view", tag, "--json", "tagName,isDraft,assets"]));
  if (release.isDraft || release.tagName !== tag) throw new Error("Expected the published release for this tag.");
  const asset = (name) => {
    const match = release.assets.find((item) => item.name === name);
    if (!match || !/^sha256:[a-f0-9]{64}$/.test(match.digest ?? "")) throw new Error(`No published digest for ${name}.`);
    return match;
  };
  const sums = await download(tag, asset("SHA256SUMS.txt"), root);
  const checksums = new Map((await readFile(sums.path, "utf8")).split(/\r?\n/).flatMap((line) => {
    const match = /^([a-f0-9]{64}) [ *](.+)$/.exec(line);
    return match ? [[match[2], match[1]]] : [];
  }));
  for (const { name, directory } of bundles) {
    const item = await download(tag, asset(name), join(root, directory));
    if (checksums.get(name) !== item.digest) throw new Error(`Published checksum mismatch for ${name}.`);
    console.log(`Verified ${name} ${item.digest}`);
  }
}

if (import.meta.main) {
  const [action, first, second, third] = process.argv.slice(2);
  if (action === "collect") await collect(first, resolve(second), resolve(third));
  else if (action === "fetch") await fetchPublished(first, second, resolve(third));
  else throw new Error("Choose collect or fetch.");
}
