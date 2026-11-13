// L2 in the real app: the shared contract examples that tests/unit runs against the development
// mock, run here against Rust on a fresh profile. Together they keep the mock from drifting.
import { $, expect } from "@wdio/globals";
import { examplesFor } from "../../contracts/examples.js";
import { outcome } from "./app.js";

describe("the real backend on a fresh profile", () => {
  it("is not the labelled development mock", async () => {
    await expect($('[data-testid="backend-badge"]')).not.toExist();
  });

  // The app runs on the machine that runs this suite.
  for (const example of examplesFor(process.platform === "darwin")) {
    it(example.name, async () => {
      expect(await outcome(example.request)).toMatchObject(example.expect);
    });
  }
});
