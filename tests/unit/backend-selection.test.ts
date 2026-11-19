import { expect, it } from "vitest";
import { chooseBackend } from "../../ui/src/api";

it("uses the native bridge whenever present and the mock only on a development server", () => {
  expect(chooseBackend(true, false)).toBe("native");
  expect(chooseBackend(true, true)).toBe("native");
  expect(chooseBackend(false, true)).toBe("mock");
  expect(chooseBackend(false, false)).toBe("unavailable");
});
