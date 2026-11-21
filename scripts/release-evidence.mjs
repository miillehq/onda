import { readFileSync } from "node:fs";

const installerSteps = {
  windows: "Install, probe the workers, launch, observe and uninstall (L7)",
  linux: "Install, launch and remove the deb; launch the AppImage (L7)",
  macos: "Mount, copy, launch and remove the app (L7)",
};

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
      const job = run.jobs.find((candidate) => candidate.name === jobName);
      if (job?.conclusion !== "success" || !job.steps.some((step) => step.name === stepName && step.conclusion === "success")) {
        throw new Error(`${jobName} did not pass its native ${kind === "ci" ? "L6" : "L7"} step in run ${run.databaseId}.`);
      }
    }
  }
  return run.databaseId;
}

if (import.meta.main) {
  const [kind, sha] = process.argv.slice(2);
  const manifest = JSON.parse(readFileSync(new URL("../keel.json", import.meta.url), "utf8"));
  console.log(requireReleaseEvidence(kind, sha, JSON.parse(readFileSync(0, "utf8")), manifest.platforms));
}
