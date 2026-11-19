// L2 in the real app: the shared contract examples that tests/unit runs against the development
// mock, run here against Rust on a fresh profile. Together they keep the mock from drifting.
import { $, browser, expect } from "@wdio/globals";
import { examplesFor } from "../../contracts/examples.js";
import { malformedSettingsRequests } from "../../contracts/settings-inputs.js";
import { malformedRequests, shapeError } from "../../contracts/malformed-requests.js";
import { outcome, saveSettings, snapshot } from "./app.js";

describe("the real backend on a fresh profile", () => {
  it("keeps a failed rewrite from switching its destination on retry", async () => {
    const original = (await snapshot()).settings;
    try {
      // The synthetic destination has no model. Validation ends the attempt before any network I/O.
      await saveSettings({ aiProvider: "local", localBaseUrl: "http://127.0.0.1:12419/v1", localModel: null });
      expect(await outcome({ command: "previewTransform", args: { source: "Fixture sentence.", transformId: "polish" } })).toMatchObject({ error: { code: "invalidInput" } });
      await saveSettings({ localBaseUrl: "http://127.0.0.1:12420/v1" });
      expect(await outcome({ command: "retryTransformation" })).toMatchObject({ error: { code: "invalidInput", message: "The rewrite destination changed. Start a new rewrite to use it." } });
    } finally {
      await saveSettings(original);
    }
  });
  it("rejects malformed command envelopes through the real bridge", async () => {
    for (const request of malformedRequests) expect(await outcome(request)).toMatchObject({ error: shapeError });
  });
  it("is not the labelled development mock", async () => {
    await expect($('[data-testid="backend-badge"]')).not.toExist();
  });

  it("enforces the bundled image policy inside the real webview", async () => {
    const observed = await browser.executeAsync((finish: (outcome: string) => void) => {
      const image = new Image();
      const end = (outcome: string) => {
        document.removeEventListener("securitypolicyviolation", blocked);
        window.clearTimeout(timer);
        finish(outcome);
      };
      const blocked = (event: SecurityPolicyViolationEvent) => {
        if (event.blockedURI === "data") end(event.effectiveDirective);
      };
      const timer = window.setTimeout(() => end("no policy event"), 3000);
      document.addEventListener("securitypolicyviolation", blocked);
      image.onload = () => end("image loaded");
      // A one-pixel GIF performs no script execution or network access.
      image.src = "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
    });
    expect(observed).toBe("img-src");
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
