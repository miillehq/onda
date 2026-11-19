import { randomUUID, createHash } from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
import { createReadStream } from "node:fs";
import { chmod, copyFile, mkdtemp, open, readFile, readdir, realpath, rename, rm, stat } from "node:fs/promises";
import { homedir } from "node:os";
import { basename, join, resolve, sep } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { DatabaseSync } from "node:sqlite";
import { expectedBundles } from "./release-files.mjs";

const APP_ID = "com.miille.onda";
const STARTUP_TIMEOUT_MS = 60_000;
const POLL_MS = 200;
const fixture = { id: randomUUID(), trigger: "installer upgrade fixture", expansion: "First line\nSecond line", enabled: true };

function run(program, args, options = {}) {
  const result = spawnSync(program, args, { stdio: "inherit", windowsHide: true, timeout: 180_000, ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${basename(program)} exited with ${result.status}.`);
}

async function findBundle(root, name) {
  const files = (await readdir(root, { recursive: true })).filter((file) => basename(file) === name);
  if (files.length !== 1) throw new Error(`Expected one ${name} in the verified artifact, found ${files.length}.`);
  return resolve(root, files[0]);
}

async function hashFile(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest("hex");
}

async function schemaVersion(checkout) {
  const names = await readdir(join(checkout, "crates/onda-core/migrations"));
  const versions = names.flatMap((name) => /^(\d+)_.*\.sql$/.exec(name)?.[1] ?? []).map(Number);
  if (!versions.length) throw new Error("The source checkout has no numbered profile migrations.");
  return Math.max(...versions);
}

function profileData(work) {
  if (process.platform === "linux") return join(work, "data", APP_ID);
  if (process.platform === "darwin") return join(homedir(), "Library", "Application Support", APP_ID);
  return join(process.env.LOCALAPPDATA, APP_ID);
}

function checkDatabase(data, expectedSchema, seed) {
  const db = new DatabaseSync(join(data, "onda.db"), { readOnly: !seed });
  try {
    if (db.prepare("PRAGMA user_version").get().user_version !== expectedSchema) throw new Error(`Expected profile schema ${expectedSchema}.`);
    if (seed) db.prepare("INSERT INTO snippets(id,normalized_trigger,payload) VALUES(?,?,?)")
      .run(fixture.id, fixture.trigger, JSON.stringify(fixture));
    const stored = db.prepare("SELECT payload FROM snippets WHERE id=?").get(fixture.id);
    if (stored?.payload !== JSON.stringify(fixture)) throw new Error("The upgrade lost or changed the seeded snippet.");
  } finally { db.close(); }
}

async function logSizes(directory) {
  let names;
  try { names = await readdir(directory); } catch (error) { if (error.code === "ENOENT") return new Map(); throw error; }
  return new Map(await Promise.all(names.map(async (name) => [name, (await stat(join(directory, name))).size])));
}

async function startedSince(directory, prior) {
  const sizes = await logSizes(directory);
  for (const [name, size] of sizes) {
    const offset = prior.get(name) ?? 0;
    if (size <= offset) continue;
    const handle = await open(join(directory, name), "r");
    try {
      const buffer = Buffer.alloc(size - offset);
      const { bytesRead } = await handle.read(buffer, 0, buffer.length, offset);
      for (const line of buffer.subarray(0, bytesRead).toString("utf8").split("\n")) {
        try { if (JSON.parse(line).fields?.message === "application started") return true; } catch { /* The log writer can be midway through its next line. */ }
      }
    } finally { await handle.close(); }
  }
  return false;
}

async function launch(executable, args, data, work, version, seed) {
  const logs = join(data, "logs");
  const prior = await logSizes(logs);
  const env = { ...process.env };
  if (process.platform === "linux") Object.assign(env, { XDG_DATA_HOME: join(work, "data"), XDG_CONFIG_HOME: join(work, "config"), XDG_CACHE_HOME: join(work, "cache") });
  const child = spawn(executable, [...args, "--background"], { env, windowsHide: true, detached: process.platform !== "win32", stdio: "ignore" });
  const exited = new Promise((settle) => { child.once("exit", settle); child.once("error", settle); });
  try {
    const end = Date.now() + STARTUP_TIMEOUT_MS;
    while (!(await startedSince(logs, prior))) {
      if (!child.pid || child.exitCode !== null || child.signalCode !== null || Date.now() >= end) throw new Error("The installed build did not emit its startup marker.");
      await delay(POLL_MS);
    }
    checkDatabase(data, version, seed);
  } finally {
    if (child.pid && child.exitCode === null && child.signalCode === null) {
      if (process.platform === "win32") child.kill(); else process.kill(-child.pid, "SIGTERM");
      const stopped = await Promise.race([exited.then(() => true), delay(5000, false, { ref: false })]);
      if (!stopped) {
        if (process.platform === "win32") child.kill("SIGKILL"); else process.kill(-child.pid, "SIGKILL");
        await exited;
      }
    }
  }
}

async function installMac(packagePath, work) {
  const mount = await mkdtemp(join(work, "mount-"));
  const destination = join(work, "Onda.app");
  run("hdiutil", ["attach", packagePath, "-nobrowse", "-readonly", "-mountpoint", mount]);
  try {
    const names = (await readdir(mount)).filter((name) => name.endsWith(".app"));
    if (names.length !== 1) throw new Error("Expected one app in the disk image.");
    if (await stat(destination).then(() => true, () => false)) await rename(destination, join(work, "previous.app"));
    run("ditto", [join(mount, names[0]), destination]);
  } finally { run("hdiutil", ["detach", mount]); }
  return [join(destination, "Contents/MacOS/onda"), []];
}

async function install(packagePath, work, format, first) {
  if (format === "dmg") return installMac(packagePath, work);
  if (format === "nsis") {
    const directory = join(work, "install");
    nsis("install", directory, packagePath);
    return [join(directory, "onda.exe"), []];
  }
  if (format === "deb") {
    if (first) run("sudo", ["env", "DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y", packagePath]);
    else run("sudo", ["dpkg", "-i", packagePath]);
    return ["/usr/bin/onda", []];
  }
  const installed = join(work, "Onda.AppImage");
  await copyFile(packagePath, installed);
  await chmod(installed, 0o755);
  return [installed, ["--appimage-extract-and-run"]];
}

function nsis(action, directory, installer) {
  const args = ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", "scripts/upgrade-nsis.ps1",
    "-Action", action, "-InstallDirectory", directory];
  if (installer) args.push("-InstallerPath", installer);
  run("powershell.exe", args);
}

async function upgradeFormat(previous, candidate, format, versions, runnerTemp) {
  const work = await mkdtemp(join(runnerTemp, "onda-upgrade-"));
  const data = profileData(work);
  let installed = false;
  let upgraded = false;
  try {
    let [executable, args] = await install(previous, work, format, true);
    installed = true;
    await launch(executable, args, data, work, versions.previous, true);
    const oldDigest = await hashFile(executable);
    [executable, args] = await install(candidate, work, format, false);
    if (await hashFile(executable) === oldDigest) throw new Error("The candidate did not replace the previously installed executable.");
    await launch(executable, args, data, work, versions.candidate, false);
    upgraded = true;
    console.log(`${format} retained its multiline snippet through profile schema ${versions.previous} → ${versions.candidate}.`);
  } finally {
    if (installed && format === "nsis") nsis("uninstall", join(work, "install"));
    if (installed && format === "deb") run("sudo", ["apt-get", "remove", "-y", "onda"]);
    if (upgraded && (format === "deb" || format === "nsis")) checkDatabase(data, versions.candidate, false);
    const owned = await realpath(work);
    if (!owned.startsWith(runnerTemp + sep) || !basename(owned).startsWith("onda-upgrade-")) throw new Error("Unexpected test-directory ownership.");
    await rm(owned, { recursive: true });
  }
}

if (import.meta.main) {
  if (process.env.DISPOSABLE_RUNNER !== "github-hosted" || process.env.GITHUB_ACTIONS !== "true") {
    throw new Error("This in-place check seeds the OS user's installed profile and requires a disposable hosted runner.");
  }
  const root = resolve(".");
  const runnerTemp = await realpath(process.env.RUNNER_TEMP);
  const platform = process.env.PLATFORM;
  const previousVersion = process.env.RELEASE_TAG.slice(1);
  const candidateVersion = JSON.parse(await readFile(join(root, "package.json"), "utf8")).version;
  const versions = { previous: await schemaVersion(join(root, "release-reference")), candidate: await schemaVersion(root) };
  const candidates = expectedBundles(candidateVersion, platform);
  for (const [index, old] of expectedBundles(previousVersion, platform).entries()) {
    await upgradeFormat(await findBundle(join(runnerTemp, "published"), old.name),
      await findBundle(join(runnerTemp, "candidate"), candidates[index].name), old.directory, versions, runnerTemp);
  }
}
