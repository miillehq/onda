// L2 in the real app: the shared contract examples that tests/unit runs against the development
// mock, run here against Rust on a fresh profile. Together they keep the mock from drifting.
import { $, expect } from "@wdio/globals";
import { examplesFor } from "../../contracts/examples.js";
import { malformedSettingsRequests } from "../../contracts/settings-inputs.js";
import { malformedRequests, shapeError } from "../../contracts/malformed-requests.js";
import { outcome, saveSettings, snapshot } from "./app.js";

describe("the real backend on a fresh profile", () => {
  it("rejects malformed command envelopes through the real bridge", async () => {
    for (const request of malformedRequests) expect(await outcome(request)).toMatchObject({ error: shapeError });
  });
  it("is not the labelled development mock", async () => {
    await expect($('[data-testid="backend-badge"]')).not.toExist();
  });

  // The app runs on the machine that runs this suite.
  for (const example of examplesFor(process.platform === "darwin")) {
    it(example.name, async () => {
      expect(await outcome(example.request)).toMatchObject(example.expect);
    });
  }

  it("refuses malformed replacements through IPC and keeps the user's preferences", async () => {
    const original = (await snapshot()).settings;
    try {
      const saved = await saveSettings({ historyDays: 7 });
      for (const { name, request } of malformedSettingsRequests(saved)) {
        expect({ name, result: await outcome(request) }).toMatchObject({ name, result: { error: expect.anything() } });
      }
      expect((await snapshot()).settings).toEqual(saved);
    } finally {
      await saveSettings(original);
    }
  });
});
