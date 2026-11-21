import assert from "node:assert/strict";
import test from "node:test";
import { desktopCommand } from "../../scripts/desktop-package.mjs";
import { requireReleaseEvidence } from "../../scripts/release-evidence.mjs";
import { expectedBundles, verifyNames } from "../../scripts/release-files.mjs";

const platforms = { tier1: ["windows-x64"], tier2: ["macos-arm64"] };
const job = (name, step) => ({ name, conclusion: "success", steps: [{ name: step, conclusion: "success" }] });
const report = (jobs) => ({ attempt: 1, headSha: "fixture-sha", status: "completed", conclusion: "success", databaseId: 12, jobs });

test("a release needs successful shell and installer steps on the tagged revision", () => {
  const ci = report([job("tier1 windows-x64", "L6 real-shell journeys"), job("tier2 macos-arm64", "Real-shell journeys (L6)")]);
  assert.equal(requireReleaseEvidence("ci", "fixture-sha", ci, platforms), 12);
  assert.throws(() => requireReleaseEvidence("ci", "other-sha", ci, platforms), /other-sha/);
  for (const attempt of [undefined, 2]) {
    assert.throws(() => requireReleaseEvidence("ci", "fixture-sha", { ...ci, attempt }, platforms), /first attempt/);
  }
  ci.jobs[1].steps[0].conclusion = "skipped";
  assert.throws(() => requireReleaseEvidence("ci", "fixture-sha", ci, platforms), /tier2 macos-arm64/);
  const installers = report([job("installers windows-x64", "Install, probe the workers, launch, observe and uninstall (L7)")]);
  assert.throws(() => requireReleaseEvidence("installers", "fixture-sha", installers, platforms), /installers macos-arm64/);
});

test("the manifest can package and verify each desktop OS with its native scripts", () => {
  for (const [platform, bundles] of [["linux", "deb,appimage"], ["darwin", "app,dmg"]]) {
    assert.deepEqual(desktopCommand("package", platform), ["bash", "scripts/build.sh", "--bundles", bundles]);
    assert.deepEqual(desktopCommand("installers", platform), ["bash", "scripts/installer-test.sh"]);
  }
  assert.equal(desktopCommand("package", "win32").at(-1), "scripts/build.ps1");
  assert.equal(desktopCommand("installers", "win32").at(-1), "scripts/installer-smoke.ps1");
});

test("each published format belongs to the intended architecture", () => {
  const names = expectedBundles("0.3.0", "linux-arm64").map(({ name }) => name);
  verifyNames("0.3.0", "linux-arm64", names);
  assert.throws(() => verifyNames("0.3.0", "linux-x64", names), /Unexpected linux-x64 packages/);
  assert.throws(() => verifyNames("0.3.0", "linux-arm64", [names[0], names[0]]), /Unexpected linux-arm64 packages/);
});
