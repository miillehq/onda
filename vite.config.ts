import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";

const page = (name: string) => fileURLToPath(new URL(`./ui/${name}.html`, import.meta.url));

export default defineConfig({
  root: "ui",
  clearScreen: false,
  oxc: { jsx: { runtime: "automatic", importSource: "preact" } },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "chrome120",
    // The CSP allows only same-origin assets, so fonts and images are never inlined as data URIs.
    assetsInlineLimit: 0,
    rolldownOptions: { input: { main: page("index"), pill: page("pill") } },
  },
  server: { host: "127.0.0.1", port: 1420, strictPort: true },
});
