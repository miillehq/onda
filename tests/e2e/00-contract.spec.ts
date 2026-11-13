// L2 in the real app: the shared contract examples that tests/unit runs against the development
// mock, run here against Rust on a fresh profile. Together they keep the mock from drifting.
import { $, expect } from "@wdio/globals";
import { allExamples } from "../../contracts/examples.js";
import { outcome } from "./app.js";

describe("the real backend on a fresh profile", () => {
  it("is not the labelled development mock", async () => {
    await expect($('[data-testid="backend-badge"]')).not.toExist();
  });

  for (const example of allExamples) {
    it(example.name, async () => {
      expect(await outcome(example.request)).toMatchObject(example.expect);
    });
  }
});
