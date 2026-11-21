import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { basename, dirname, join } from "node:path";

const [input = "artifacts/platforms", output = "artifacts/contact-sheet.html"] = process.argv.slice(2);
const manifest = JSON.parse(await readFile(new URL("../keel.json", import.meta.url), "utf8"));
const escape = (text) => text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;");
const sections = [];
const missing = [];
const journeys = ["settings-dark", "dictionary-after-restart"];
let images = 0;
for (const platform of [...manifest.platforms.tier1, ...manifest.platforms.tier2]) {
  const root = join(input, `e2e-${platform}`);
  const files = await readdir(root, { recursive: true }).catch((error) => { if (error.code === "ENOENT") return []; throw error; });
  const cards = [];
  for (const journey of journeys) {
    if (!files.some((file) => basename(file) === `${journey}-${platform}.png`)) missing.push(`${platform}/${journey}`);
  }
  for (const path of files.filter((file) => file.endsWith(".png")).sort()) {
    const caption = escape(basename(path));
    const source = `data:image/png;base64,${(await readFile(join(root, path))).toString("base64")}`;
    cards.push(`<figure><img src="${source}" alt="${caption}"><figcaption>${caption}</figcaption></figure>`);
    images++;
  }
  sections.push(`<section><h2>${escape(platform)}</h2><div>${cards.join("") || "<p>No screenshot was uploaded for this platform.</p>"}</div></section>`);
}
if (!images) throw new Error("The real-shell jobs uploaded no screenshots.");
await mkdir(dirname(output), { recursive: true });
await writeFile(output, `<!doctype html><html lang="en"><meta charset="utf-8"><title>Onda real-shell screenshots</title>
<style>body{font:16px system-ui;margin:2rem;background:#f5f5f5;color:#181818}section>div{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(30rem,100%),1fr));gap:1rem}figure{margin:0}img{width:100%;height:auto}figcaption{padding:.4rem 0;overflow-wrap:anywhere}</style>
<h1>Onda real-shell screenshots</h1>${sections.join("")}</html>`);
console.log(`${images} real-shell screenshots in ${output}`);
if (missing.length) throw new Error(`Missing native journey screenshots: ${missing.join(", ")}`);
