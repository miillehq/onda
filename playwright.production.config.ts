import { defineConfig } from "@playwright/test";
import { fileURLToPath } from "node:url";
import { browserTestPort } from "./tests/ui/server";

const checkout = fileURLToPath(new URL(".", import.meta.url));
const port = browserTestPort(`${checkout}/built-ui`);
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: "tests/production",
  outputDir: "artifacts/production-test-results",
  reporter: "list",
  retries: 0,
  use: { baseURL },
  projects: [{ name: "chromium", use: { browserName: "chromium" } }, { name: "webkit", use: { browserName: "webkit" } }],
  webServer: {
    command: `npm exec -- vite preview --host 127.0.0.1 --strictPort --port ${port} --outDir ../artifacts/production-ui`,
    url: baseURL,
    reuseExistingServer: false,
  },
});
