/** Select the renderer adapter before any application state is loaded. */
export function chooseBackend(hasBridge: boolean, development: boolean): "native" | "mock" | "unavailable" {
  if (hasBridge) return "native";
  return development ? "mock" : "unavailable";
}
