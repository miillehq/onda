import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { verifyArchives } from "../../scripts/verify-apt-archives.mjs";

test("APT can reuse the signed-index version and digest only when both match the restored archive", async () => {
  const directory = await mkdtemp(join(tmpdir(), "onda-deb-test-"));
  try {
    const expected = "signed fixture bytes";
    const sha = createHash("sha256").update(expected).digest("hex");
    const current = "demo_1%3a2.0-1_amd64.deb";
    await writeFile(join(directory, current), expected);
    await writeFile(join(directory, "other_2.0_all.deb"), "incomplete");
    await writeFile(join(directory, "demo_1.0_amd64.deb"), expected);
    const report = await verifyArchives(directory, (queries) => {
      assert.deepEqual(queries.sort(), ["demo=1.0", "demo=1:2.0-1", "other=2.0"]);
      return "Package: demo\nVersion: 1:2.0-1\nArchitecture: amd64\nSHA256: " + sha +
        "\n\nPackage: other\nVersion: 2.0\nArchitecture: all\nSHA256: " + sha + "\n";
    });
    assert.deepEqual(report, { kept: 1, discarded: 2 });
    assert.deepEqual(await readdir(directory), [current]);
    assert.equal(await readFile(join(directory, current), "utf8"), expected);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
