import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const workerDirectory = resolve(root, process.argv[2] ?? "target/debug");
const modelDirectory = resolve(root, ".artifacts/ci-models");

/** Run the same verified model installation and native worker tests on every supported runner. */
function cargo(args, options) {
  const result = spawnSync("cargo", args, { cwd: root, encoding: "utf8", ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
  return result.stdout;
}

const installation = JSON.parse(cargo(
  ["run", "--locked", "-p", "onda-asr", "--bin", "onda-asr-benchmark", "--", "download", modelDirectory, "whisper-tiny-en"],
  { stdio: ["ignore", "pipe", "inherit"] },
));
if (typeof installation.path !== "string" || installation.path.length === 0) throw new Error("The verified model has no path.");
cargo(["test", "--locked", "-p", "onda-asr", "--test", "golden", "--", "--ignored", "--test-threads=1", "--nocapture"], {
  stdio: "inherit",
  env: { ...process.env, ONDA_ASR_TEST_MODEL: installation.path, ONDA_ASR_WORKER_DIR: workerDirectory },
});
