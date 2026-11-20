import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

test("the Windows installer observes only complete markers written by its launch", { skip: process.platform !== "win32" }, async () => {
  const directory = await mkdtemp(join(tmpdir(), "onda-install-log-"));
  try {
    const fixture = fileURLToPath(new URL("./installed-log.fixture.ps1", import.meta.url));
    for (const shell of ["powershell.exe", "pwsh"]) {
      const sample = await mkdtemp(join(directory, "shell-"));
      const result = spawnSync(shell, ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", fixture, "-Directory", sample], { encoding: "utf8", windowsHide: true });
      // Windows always has powershell.exe. PowerShell 7 is checked wherever it is installed.
      if (shell === "pwsh" && result.error?.code === "ENOENT" && !process.env.CI) continue;
      if (result.error) throw result.error;
      assert.equal(result.status, 0, result.stderr || result.stdout);
    }
  } finally { await rm(directory, { recursive: true, force: true }); }
});
