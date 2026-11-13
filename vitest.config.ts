import { defineConfig } from "vitest/config";

// L1 and L2 for the interface: pure UI logic and the development mock held to the shared
// contract examples. Browser-like files opt into happy-dom per file.
export default defineConfig({
  test: {
    include: ["tests/unit/**/*.test.ts"],
    environment: "node",
  },
});
