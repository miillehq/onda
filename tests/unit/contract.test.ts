// @vitest-environment happy-dom
// @vitest-environment-options {"url":"http://127.0.0.1:1420/?mock=onboarding"}
//
// L2: the labelled development mock answers the shared contract examples exactly as Rust does.
// tests/e2e/00-contract.spec.ts runs the same list against the real app, so the mock cannot
// drift from the backend without a red on one side.
import { describe, expect, it } from "vitest";
import { allExamples, type Example } from "../../contracts/examples";
import { createMockBackend } from "../../ui/src/mock/backend";

const backend = createMockBackend();

async function outcome(example: Example): Promise<unknown> {
  try {
    return { response: await backend.call(example.request) };
  } catch (error) {
    return { error };
  }
}

describe("the development mock on a fresh install", () => {
  for (const example of allExamples) {
    it(example.name, async () => {
      expect(await outcome(example)).toMatchObject(example.expect);
    });
  }
});
