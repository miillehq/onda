import { execFileSync } from "node:child_process";
import { stripVTControlCharacters } from "node:util";

const gh = (...args) => execFileSync("gh", args, { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 });
let id = process.argv[2];
if (!id) {
  const branch = execFileSync("git", ["branch", "--show-current"], { encoding: "utf8" }).trim();
  id = gh("run", "list", "--branch", branch, "--workflow", "ci.yml", "--limit", "1", "--json", "databaseId", "-q", ".[0].databaseId").trim();
}
if (!/^\d+$/.test(id)) throw new Error("Supply a GitHub Actions run ID for this repository.");
const run = JSON.parse(gh("run", "view", id, "--json", "workflowName,headSha,status,conclusion,jobs,url"));
console.log(`${run.workflowName} ${run.headSha.slice(0, 12)}  ${run.conclusion || run.status}  ${run.url}`);
for (const job of run.jobs) {
  console.log(`${job.name}  ${job.conclusion || job.status}`);
  for (const step of job.steps.filter((item) => ["failure", "cancelled", "timed_out"].includes(item.conclusion))) {
    console.log(`  ${step.name}  ${step.conclusion}  ${job.url}`);
  }
  if (job.conclusion !== "failure" || run.status !== "completed") continue;
  const log = stripVTControlCharacters(gh("run", "view", "--job", String(job.databaseId), "--log-failed"));
  for (const line of log.split(/\r?\n/).filter((entry) => /##\[error\]|error\[E\d+\]|panicked at|assertion failed|Error:/.test(entry)).slice(0, 3)) {
    console.log(`  ${line.slice(0, 260)}`);
  }
}
