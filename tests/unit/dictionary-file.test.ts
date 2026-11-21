import { expect, it, vi } from "vitest";
import { LIMITS } from "../../ui/src/generated/core";
import { readDictionaryArchive } from "../../ui/src/lib/dictionaryFile";

it("refuses an oversized backup before allocating space for its contents", async () => {
  const arrayBuffer = vi.fn(async () => new ArrayBuffer(0));
  await expect(readDictionaryArchive({ size: LIMITS.maxDictionaryArchiveBytes + 1, arrayBuffer }))
    .rejects.toMatchObject({ code: "invalidInput" });
  expect(arrayBuffer).not.toHaveBeenCalled();
});
