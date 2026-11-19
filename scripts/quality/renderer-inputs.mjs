import { isAbsolute, join, relative, resolve, sep } from "node:path";
import { build } from "vite";

/** Read the production compiler's emitted modules after it folds DEV guards and removes dead imports. */
export async function rendererInputs(projectRoot, options = { configFile: join(projectRoot, "vite.config.ts") }) {
  const styles = new Set(), pages = new Set();
  const relativeToProject = (path) => relative(projectRoot, path).split(sep).join("/");
  await build({
    ...options,
    logLevel: "silent",
    build: { ...options.build, write: false },
    plugins: [...(options.plugins ?? []), {
      name: "onda-source-inputs",
      configResolved(config) {
        const inputs = config.build.rolldownOptions.input ?? join(config.root, "index.html");
        for (const input of typeof inputs === "string" ? [inputs] : Object.values(inputs)) {
          if (input.endsWith(".html")) pages.add(relativeToProject(isAbsolute(input) ? input : resolve(config.root, input)));
        }
      },
      generateBundle(_, bundle) {
        for (const output of Object.values(bundle)) {
          if (output.type !== "chunk") continue;
          for (const id of Object.keys(output.modules)) {
            const path = id.split("?", 1)[0];
            if (isAbsolute(path) && path.endsWith(".css")) styles.add(relativeToProject(path));
          }
        }
      },
    }],
  });
  return { pages: [...pages], styles };
}
