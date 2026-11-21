import { keyLabel, orderedKeys } from "./keys";

/** Use the same ordered names for the visible keycaps and their accessible text. */
export function describeShortcut(keys: readonly number[]) {
  const keycaps = orderedKeys(keys).map((key) => ({ key, label: keyLabel(key) }));
  return { keycaps, text: keycaps.map((cap) => cap.label).join(" + ") };
}
