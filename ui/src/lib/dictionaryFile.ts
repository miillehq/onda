import { LIMITS, type AppError } from "../generated/core";

/** Refuse oversized or damaged files before sending a backup to the desktop importer. */
export async function readDictionaryArchive(file: Pick<Blob, "size" | "arrayBuffer">): Promise<string> {
  if (file.size > LIMITS.maxDictionaryArchiveBytes) {
    throw { code: "invalidInput", message: "Dictionary archive exceeds the maximum backup size.", retryable: false } satisfies AppError;
  }
  const bytes = await file.arrayBuffer();
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw { code: "invalidInput", message: "Choose a dictionary backup saved as valid UTF-8.", retryable: false } satisfies AppError;
  }
}
