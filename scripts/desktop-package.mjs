import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

/** Resolve the repository's actual package or installer command for the host. */
export function desktopCommand(action, platform) {
  if (action !== "package" && action !== "installers") throw new Error("Expected package or installers.");
  if (platform === "win32") {
    const script = action === "package" ? "scripts/build.ps1" : "scripts/installer-smoke.ps1";
    return ["powershell.exe", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script];
  }
  if (platform !== "linux" && platform !== "darwin") throw new Error(`Unsupported desktop platform ${platform}.`);
  return action === "installers" ? ["bash", "scripts/installer-test.sh"]
    : ["bash", "scripts/build.sh", "--bundles", platform === "linux" ? "deb,appimage" : "app,dmg"];
}

if (import.meta.main) {
  const [program, ...args] = desktopCommand(process.argv[2], process.platform);
  const result = spawnSync(program, args, { cwd: fileURLToPath(new URL("../", import.meta.url)), stdio: "inherit" });
  if (result.error) throw result.error;
  process.exitCode = result.status ?? 1;
}
