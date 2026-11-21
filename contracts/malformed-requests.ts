import type { ApiRequest } from "../ui/src/generated/api.js";

/** Inputs from an untyped caller. The mock and the native command must both reject them. */
export const malformedRequests = [
  null, 4, [], "2026-11-19T00:00:00Z",
  { command: "getSettings", args: [] },
  { command: "setProfileName", args: {} },
  { command: "setProfileName", args: [null] },
  { command: "getSettings", unexpected: true },
  { command: "saveSnippet", args: { snippet: { id: "test", trigger: "test", expansion: "test", enabled: true, extra: true } } },
  // serde's other spelling of a name. Both sides accept only the string.
  { command: { getSettings: null } },
  { command: "removeProviderKey", args: { provider: { anthropic: null } } },
] as unknown as readonly ApiRequest[];

export const shapeError = { code: "invalidInput", message: "The desktop request has an invalid shape." } as const;
