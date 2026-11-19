// @vitest-environment happy-dom
// @vitest-environment-options {"url":"http://127.0.0.1:1420/?mock=onboarding"}
//
// L2: the labelled development mock answers the shared contract examples exactly as Rust does.
// tests/e2e/00-contract.spec.ts runs the same list against the real app, so the mock cannot
// drift from the backend without a red on one side.
import { describe, expect, it } from "vitest";
import { examplesFor, type Example } from "../../contracts/examples";
import { malformedSettingsRequests } from "../../contracts/settings-inputs";
import { DEFAULT_SETTINGS } from "../../ui/src/generated/core";
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
  // The mock answers as Onda on Windows, whichever OS runs this test.
  for (const example of examplesFor(false)) {
    it(example.name, async () => {
      expect(await outcome(example)).toMatchObject(example.expect);
    });
  }

  it("refuses malformed replacements and keeps the user's preferences", async () => {
    const saved = { ...structuredClone(DEFAULT_SETTINGS), historyDays: 7 };
    await backend.call({ command: "updateSettings", args: { settings: saved } });
    for (const { name, request } of malformedSettingsRequests(saved)) {
      await expect(backend.call(request), name).rejects.toMatchObject({ code: "invalidInput" });
    }
    expect(await backend.call({ command: "getSettings" })).toEqual({ type: "settings", payload: saved });
  });
});
