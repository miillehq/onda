import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const installerSteps = {
  windows: "Install, probe the workers, launch, observe and uninstall (L7)",
  linux: "Install, launch and remove the deb; launch the AppImage (L7)",
  macos: "Mount, copy, launch and remove the app (L7)",
};

function requireSteps(run, name, steps) {
  const job = run.jobs.find((candidate) => candidate.name === name);
  const passed = new Set(job?.steps.filter((step) => step.conclusion === "success").map((step) => step.name));
  if (job?.conclusion !== "success" || !steps.every((step) => passed.has(step))) {
    throw new Error(`${name} did not pass its required steps in run ${run.databaseId}.`);
  }
}

/** Check the named native step as well as its job, including advisory jobs and fresh attempts. */
export function requireReleaseEvidence(kind, sha, run, platforms) {
  if (kind !== "ci" && kind !== "installers") throw new Error("Unknown release evidence type.");
  if (run.attempt !== 1) throw new Error(`Release evidence must come from a complete first attempt of the ${kind} workflow.`);
  if (run.headSha !== sha || run.status !== "completed" || run.conclusion !== "success") {
    throw new Error(`Expected a completed, successful ${kind} run for ${sha}.`);
  }
  for (const tier of ["tier1", "tier2"]) {
    for (const platform of platforms[tier]) {
      const jobName = `${kind === "ci" ? tier : "installers"} ${platform}`;
      const stepName = kind === "installers" ? installerSteps[platform.split("-")[0]]
        : tier === "tier1" ? "L6 real-shell journeys" : "Real-shell journeys (L6)";
      requireSteps(run, jobName, [stepName]);
    }
  }
  if (kind === "ci" && platforms.tier2.includes("macos-x64")) {
    requireSteps(run, "debug workers macos-x64", ["Build the native workers"]);
    requireSteps(run, "native libraries macos-x64", ["Crates without the app shell (L1, L3)", "Synthetic account and key storage round trips (L3)"]);
    requireSteps(run, "app tests macos-x64", ["App shell (L1)"]);
  }
  return run.databaseId;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [kind, sha] = process.argv.slice(2);
  const manifest = JSON.parse(readFileSync(new URL("../keel.json", import.meta.url), "utf8"));
  console.log(requireReleaseEvidence(kind, sha, JSON.parse(readFileSync(0, "utf8")), manifest.platforms));
}
