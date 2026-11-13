// Composes the README images in docs/images from real Onda screens (capture.mjs), the brand
// kit's mark and wordmark, the interface's own icons and fonts, and its light and dark palettes.
//   npm run readme:images
import { chromium } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { capture, screensDirectory } from "./capture.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const output = join(root, "docs", "images");
const studioDirectory = join(root, ".artifacts", "readme");
const url = (path) => pathToFileURL(path).href;

const brand = readFileSync(join(root, "ui", "src", "components", "Brand.tsx"), "utf8");
const MARK = /MARK_PATH =\s*"([^"]+)"/.exec(brand)[1];
const WORDMARK = /WORDMARK_PATH =\s*"([^"]+)"/.exec(brand)[1];
const mark = (height, color = "currentColor") =>
  `<svg viewBox="0 0 521.378 340" height="${height}" width="${(height * 521.378) / 340}"><path fill="${color}" d="${MARK}"/></svg>`;
const wordmark = (height, color = "currentColor") =>
  `<svg viewBox="0 0 727 217" height="${height}" width="${(height * 727) / 217}"><path fill="${color}" d="${WORDMARK}"/></svg>`;
const lockup = (height, color = "currentColor") => `<span class="lockup">${mark(height, color)}${wordmark(height * 0.86, color)}</span>`;

const icons = {};
for (const name of ["mic", "audio-lines", "cpu", "sparkles", "text-cursor-input", "lock", "shield-check", "key-round", "cloud-off", "message-square", "arrow-right", "rotate-ccw-clock", "book-open", "keyboard", "wifi-off", "trash"]) {
  const module = await import(url(join(root, "node_modules", "lucide-preact", "dist", "esm", "icons", `${name}.mjs`)));
  icons[name] = module.__iconData.node;
}
const icon = (name, size = 20, width = 2) =>
  `<svg viewBox="0 0 24 24" width="${size}" height="${size}" fill="none" stroke="currentColor" stroke-width="${width}" stroke-linecap="round" stroke-linejoin="round">${icons[name]
    .map(([tag, attributes]) => `<${tag} ${Object.entries(attributes).filter(([key]) => key !== "key").map(([key, value]) => `${key}="${value}"`).join(" ")}/>`)
    .join("")}</svg>`;
const winKey = `<svg viewBox="0 0 16 16" width="15" height="15"><path fill="currentColor" d="M0 0h7.5v7.5H0zM8.5 0H16v7.5H8.5zM0 8.5h7.5V16H0zM8.5 8.5H16V16H8.5z"/></svg>`;

const screen = (name) => url(join(screensDirectory, `${name}.png`));
const fonts = join(root, "node_modules", "@fontsource-variable");

const css = `
@font-face { font-family: "Inter"; src: url("${url(join(fonts, "inter", "files", "inter-latin-wght-normal.woff2"))}") format("woff2"); font-weight: 100 900; }
@font-face { font-family: "Newsreader"; src: url("${url(join(fonts, "newsreader", "files", "newsreader-latin-wght-normal.woff2"))}") format("woff2"); font-weight: 200 800; }
:root {
  --base: #F3F3F4; --panel: #FFFFFF; --sunken: #F5F5F6; --line: #E2E2E5; --line-strong: #CDCDD2;
  --fg: #0F0F10; --fg2: #45454B; --fg3: #66666D; --key: #FFFFFF; --key-edge: #BDBDC2;
  --canvas: radial-gradient(120% 100% at 50% 0%, #FFFFFF 0%, #F4F4F5 55%, #EBEBED 100%);
  --shadow: 0 28px 70px rgba(15, 15, 16, 0.16), 0 6px 18px rgba(15, 15, 16, 0.08);
  --tile: linear-gradient(180deg, #262628 0%, #0F0F10 55%, #040404 100%);
}
.dark {
  --base: #0B0B0C; --panel: #161618; --sunken: #1D1D20; --line: #2A2A2E; --line-strong: #3A3A40;
  --fg: #F2F2F3; --fg2: #B4B4BA; --fg3: #8E8E95; --key: #2A2A2E; --key-edge: #050506;
  --canvas: radial-gradient(120% 100% at 50% 0%, #1F1F22 0%, #111113 55%, #0B0B0C 100%);
  --shadow: 0 28px 70px rgba(0, 0, 0, 0.6), 0 6px 18px rgba(0, 0, 0, 0.4);
}
* { box-sizing: border-box; }
body { margin: 0; padding: 40px; font-family: "Inter", sans-serif; color: var(--fg); background: transparent; -webkit-font-smoothing: antialiased; font-feature-settings: "cv11", "ss01"; }
.shot { position: relative; overflow: hidden; border-radius: 22px; background: var(--canvas); margin-bottom: 40px; }
.lockup { display: inline-flex; align-items: center; gap: 0.32em; }
.window { position: absolute; border-radius: 14px; overflow: hidden; background: var(--panel); border: 1px solid var(--line); box-shadow: var(--shadow); }
.window > img { display: block; width: 100%; }
.titlebar { height: 34px; display: flex; align-items: center; gap: 8px; padding: 0 14px; background: var(--base); border-bottom: 1px solid var(--line); color: var(--fg2); font-size: 12.5px; font-weight: 600; letter-spacing: -0.01em; }
.titlebar .dots { margin-left: auto; display: flex; gap: 14px; color: var(--fg3); }
.titlebar .dots i { display: block; width: 10px; height: 1.5px; background: currentColor; border-radius: 1px; }
.pill { position: absolute; }
.pill img { display: block; height: 100%; }
.caret { display: inline-block; width: 2px; height: 1.05em; background: var(--fg); vertical-align: -0.15em; margin-left: 1px; }
.key { display: inline-flex; align-items: center; justify-content: center; gap: 6px; height: 38px; min-width: 44px; padding: 0 12px; border-radius: 10px; background: #FFFFFF; color: #0F0F10; font-weight: 650; font-size: 14px; box-shadow: 0 3px 0 #BDBDC2, 0 8px 16px rgba(0, 0, 0, 0.35); }
.plus { color: #8E8E95; font-weight: 600; margin: 0 5px; }

/* How it works */
.steps { display: grid; grid-template-columns: repeat(4, 1fr); gap: 34px; padding: 34px 36px 36px; position: relative; }
.step { position: relative; }
.step .next { position: absolute; left: -29px; top: 88px; color: var(--fg3); display: flex; }
.visual { height: 196px; border-radius: 16px; background: var(--panel); border: 1px solid var(--line); display: flex; align-items: center; justify-content: center; flex-direction: column; gap: 12px; overflow: hidden; position: relative; }
.visual.tile { background: var(--tile); border-color: #0F0F10; }
.number { display: inline-flex; width: 26px; height: 26px; border-radius: 13px; align-items: center; justify-content: center; background: var(--fg); color: var(--panel); font-size: 13px; font-weight: 700; margin: 18px 0 10px; }
.step h3 { margin: 0 0 6px; font-size: 18px; font-weight: 680; letter-spacing: -0.018em; }
.step p { margin: 0; font-size: 14px; line-height: 1.45; color: var(--fg2); }
.chip { display: inline-flex; align-items: center; gap: 7px; height: 30px; padding: 0 12px; border-radius: 15px; border: 1px solid var(--line-strong); color: var(--fg2); font-size: 13px; font-weight: 560; background: var(--panel); }
.chip.solid { background: var(--fg); color: var(--panel); border-color: var(--fg); }
.chip.dashed { border-style: dashed; }
.field { width: 86%; background: var(--panel); border: 1px solid var(--line-strong); border-radius: 12px; padding: 14px 16px; font-size: 15px; line-height: 1.45; color: var(--fg); box-shadow: 0 6px 20px rgba(15, 15, 16, 0.08); }
.field small { display: flex; align-items: center; gap: 6px; color: var(--fg3); font-size: 12px; font-weight: 560; margin-bottom: 8px; }

/* Privacy */
.privacy { padding: 40px 44px 38px; }
.device { border: 1.5px solid var(--line-strong); border-radius: 20px; padding: 46px 30px 30px; position: relative; background: color-mix(in srgb, var(--panel) 55%, transparent); }
.device-label { position: absolute; top: -15px; left: 26px; display: inline-flex; align-items: center; gap: 8px; height: 30px; padding: 0 14px; border-radius: 15px; background: var(--fg); color: var(--panel); font-size: 13.5px; font-weight: 650; }
.flow { display: grid; grid-template-columns: 1fr 34px 1fr 34px 1fr 34px 1fr; align-items: center; }
.node { background: var(--panel); border: 1px solid var(--line); border-radius: 16px; padding: 18px 18px 17px; min-height: 132px; box-shadow: 0 2px 0 rgba(15, 15, 16, 0.03); }
.node .glyph { width: 40px; height: 40px; border-radius: 12px; display: flex; align-items: center; justify-content: center; background: var(--tile); color: #FFFFFF; margin-bottom: 12px; }
.node b { display: block; font-size: 16px; font-weight: 680; letter-spacing: -0.015em; margin-bottom: 4px; }
.node span { display: block; font-size: 13px; line-height: 1.4; color: var(--fg2); }
.arrow { color: var(--fg3); display: flex; justify-content: center; }
.outside { display: grid; grid-template-columns: 1fr 1.25fr; gap: 26px; margin-top: 30px; align-items: stretch; }
.provider { border: 1.5px dashed var(--line-strong); border-radius: 18px; padding: 18px 20px; display: flex; gap: 16px; align-items: flex-start; }
.provider .glyph { flex: none; width: 40px; height: 40px; border-radius: 12px; display: flex; align-items: center; justify-content: center; border: 1px solid var(--line-strong); color: var(--fg); }
.provider b { display: block; font-size: 16px; font-weight: 680; margin-bottom: 4px; }
.provider span { display: block; font-size: 13px; line-height: 1.45; color: var(--fg2); }
.promises { display: grid; grid-template-columns: 1fr 1fr; gap: 10px 18px; align-content: center; }
.promise { display: flex; align-items: center; gap: 10px; font-size: 14px; font-weight: 560; color: var(--fg); }
.promise svg { color: var(--fg2); flex: none; }

/* Pills */
.pills { display: flex; justify-content: space-between; align-items: flex-start; padding: 22px 34px 28px; }
.pill-state { display: flex; flex-direction: column; align-items: center; gap: 4px; }
/* The captures carry transparent room for the shadow; let neighbours share it. */
.pill-state img { height: 92px; margin: 0 -30px -12px; }
.pill-state b { font-size: 15px; font-weight: 650; letter-spacing: -0.01em; }
.pill-state span { font-size: 12.5px; color: var(--fg3); text-align: center; max-width: 170px; line-height: 1.35; }

/* Gallery frames: the window alone, with room for its shadow */
.frame { position: relative; padding: 30px 34px 44px; margin-bottom: 40px; }
.frame .window { position: relative; }
`;

const messageWindow = (theme) => `
  <div class="window" style="left: 532px; top: 148px; width: 620px;">
    <div class="titlebar">${icon("message-square", 15)}<span>New message</span><span class="dots"><i></i><i></i><i></i></span></div>
    <div style="padding: 6px 22px 26px; font-size: 15px; color: var(--fg);">
      <div style="display: flex; gap: 14px; padding: 11px 0; border-bottom: 1px solid var(--line); color: var(--fg2);"><span style="width: 56px; color: var(--fg3);">To</span>Design team</div>
      <div style="display: flex; gap: 14px; padding: 11px 0; border-bottom: 1px solid var(--line); color: var(--fg2);"><span style="width: 56px; color: var(--fg3);">Subject</span>Thursday review</div>
      <div style="padding-top: 16px; line-height: 1.6;">Hi all,<br><br>Let's move the design review to Thursday at three and send the notes after.<span class="caret"></span><br><br><br></div>
    </div>
  </div>
  <div class="pill" style="left: ${532 + 310 - 98}px; top: 502px; height: 108px;"><img src="${screen(`pill-writing-${theme}`)}"></div>`;

const studio = (theme) => `<!doctype html><html class="${theme}"><head><meta charset="utf-8"><style>${css}</style></head><body>
<section id="hero" class="shot" style="width: 1200px; height: 680px;">
  <div class="window" style="left: 44px; top: 46px; width: 780px;">
    <div class="titlebar">${mark(12)}<span>Onda</span><span class="dots"><i></i><i></i><i></i></span></div>
    <img src="${screen(theme === "dark" ? "home-dark" : "home")}">
  </div>
  ${messageWindow(theme)}
</section>

<section id="how" class="shot" style="width: 1200px;">
  <div class="steps">
    <div class="step">
      <div class="visual tile">
        <div style="display: flex; align-items: center;"><span class="key">Ctrl</span><span class="plus">+</span><span class="key">${winKey}Win</span><span class="plus">+</span><span class="key" style="min-width: 64px;">Space</span></div>
        <div style="color: #B4B4BA; font-size: 12.5px; font-weight: 560;">Control + Shift + Space on a Mac</div>
      </div>
      <div class="number">1</div><h3>Hold your shortcut</h3><p>In any app, wherever your cursor is. Or tap it and talk hands-free.</p>
    </div>
    <div class="step">
      <div class="visual"><img src="${screen(`pill-listening-${theme}`)}" style="height: 118px;"></div>
      <span class="next">${icon("arrow-right", 22)}</span><div class="number">2</div><h3>Speak</h3><p>The pill shows Onda is listening. Esc cancels and keeps nothing.</p>
    </div>
    <div class="step">
      <div class="visual">
        <div style="width: 54px; height: 54px; border-radius: 16px; background: var(--tile); color: #FFFFFF; display: flex; align-items: center; justify-content: center;">${icon("cpu", 28, 1.8)}</div>
        <div style="display: flex; gap: 8px;"><span class="chip solid">${icon("wifi-off", 14)}On your computer</span></div>
        <span class="chip dashed">${icon("sparkles", 14)}AI polish, if you turn it on</span>
      </div>
      <span class="next">${icon("arrow-right", 22)}</span><div class="number">3</div><h3>Onda writes it out</h3><p>Speech recognition runs locally with Whisper. Your dictionary fixes names and jargon.</p>
    </div>
    <div class="step">
      <div class="visual"><div class="field"><small>${icon("text-cursor-input", 14)}Any app</small>Let's move the design review to Thursday at three.<span class="caret"></span></div></div>
      <span class="next">${icon("arrow-right", 22)}</span><div class="number">4</div><h3>Let go</h3><p>Your words are typed where your cursor is, and your clipboard is put back.</p>
    </div>
  </div>
</section>

<section id="privacy" class="shot privacy" style="width: 1200px;">
  <div class="device">
    <span class="device-label">${icon("lock", 15)}Stays on your computer</span>
    <div class="flow">
      <div class="node"><div class="glyph">${icon("mic", 20)}</div><b>Your voice</b><span>Recorded only while you hold the shortcut.</span></div>
      <div class="arrow">${icon("arrow-right", 22)}</div>
      <div class="node"><div class="glyph">${icon("audio-lines", 20)}</div><b>Speech recognition</b><span>Whisper models run on this computer. No internet needed.</span></div>
      <div class="arrow">${icon("arrow-right", 22)}</div>
      <div class="node"><div class="glyph">${icon("book-open", 20)}</div><b>Your words</b><span>Your dictionary and snippets are applied locally.</span></div>
      <div class="arrow">${icon("arrow-right", 22)}</div>
      <div class="node"><div class="glyph">${icon("text-cursor-input", 20)}</div><b>The app you're in</b><span>Typed where your cursor is. Never into password fields.</span></div>
    </div>
  </div>
  <div class="outside">
    <div class="provider"><div class="glyph">${icon("sparkles", 20)}</div><div><b>Your AI provider, only if you choose</b><span>Turn on AI polish or rewrites and only that text goes to the provider you pick: ChatGPT, an OpenAI-compatible server, Anthropic, or a model on your own computer. Never audio.</span></div></div>
    <div class="promises">
      <div class="promise">${icon("cloud-off", 19)}No audio uploads, no analytics</div>
      <div class="promise">${icon("rotate-ccw-clock", 19)}History kept 3 days, encrypted</div>
      <div class="promise">${icon("key-round", 19)}Keys in your system's secure store</div>
      <div class="promise">${icon("trash", 19)}Clear history any time</div>
    </div>
  </div>
</section>

<section id="pills" class="shot" style="width: 1200px;">
  <div class="pills">
    ${[
      ["listening", "Listening", "Hold the shortcut and talk."],
      ["handsfree", "Hands-free", "Tap to start; stop when you're done."],
      ["writing", "Writing it out", "The wave settles into the cursor."],
      ["polishing", "Polishing", "Only when AI polish is on."],
      ["done", "Done", "A quick confirmation, then it's gone."],
    ]
      .map(([state, title, text]) => `<div class="pill-state"><img src="${screen(`pill-${state}-${theme}`)}"><b>${title}</b><span>${text}</span></div>`)
      .join("")}
  </div>
</section>

${["history", "stats", "dictionary", "speech"]
  .map(
    (name) => `<section id="screen-${name}" class="frame" style="width: 1060px;">
  <div class="window" style="width: 100%;"><div class="titlebar">${mark(12)}<span>Onda</span><span class="dots"><i></i><i></i><i></i></span></div><img src="${screen(theme === "dark" ? `${name}-dark` : name)}"></div>
</section>`,
  )
  .join("\n")}
</body></html>`;

const social = `<!doctype html><html class="dark"><head><meta charset="utf-8"><style>${css}
body { padding: 0; }
#social { width: 1280px; height: 640px; position: relative; overflow: hidden; background: var(--tile); color: #FFFFFF; }
#social .copy { position: absolute; left: 84px; top: 92px; width: 640px; }
#social h1 { font-size: 58px; line-height: 1.04; letter-spacing: -0.03em; font-weight: 720; margin: 44px 0 22px; }
#social p { font-size: 24px; line-height: 1.4; color: #B4B4BA; margin: 0; font-weight: 450; }
#social .platforms { margin-top: 34px; display: flex; gap: 10px; }
#social .platforms span { height: 34px; padding: 0 15px; border-radius: 17px; border: 1px solid #3A3A40; display: inline-flex; align-items: center; font-size: 15px; font-weight: 600; color: #E4E4E7; }
</style></head><body>
<section id="social">
  <div class="copy">
    ${lockup(46, "#FFFFFF")}
    <h1>Your voice, typed<br>where your cursor is.</h1>
    <p>Offline voice dictation. Local Whisper speech recognition, optional AI polish with the provider you choose.</p>
    <div class="platforms"><span>Windows</span><span>macOS</span><span>Linux</span></div>
  </div>
  <div class="window" style="left: 770px; top: 170px; width: 430px; border-color: #2A2A2E;">
    <div class="titlebar">${icon("message-square", 15)}<span>New message</span></div>
    <div style="padding: 18px 22px 30px; font-size: 17px; line-height: 1.6; color: var(--fg);">Running ten minutes late, start without me.<span class="caret"></span><br><br></div>
  </div>
  <div class="pill" style="left: ${770 + 215 - 106}px; top: 330px; height: 124px;"><img src="${screen("pill-listening-dark")}"></div>
</section>
</body></html>`;

const lockupSvg = (color) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1300 340" width="260" height="68" role="img" aria-label="Onda"><title>Onda</title><path fill="${color}" d="${MARK}"/><g transform="translate(573 63) scale(1.2903)"><path fill="${color}" d="${WORDMARK}"/></g></svg>\n`;

export async function render({ recapture = true } = {}) {
  if (recapture) await capture();
  mkdirSync(output, { recursive: true });
  mkdirSync(studioDirectory, { recursive: true });
  writeFileSync(join(output, "onda-logo-light.svg"), lockupSvg("#0F0F10"));
  writeFileSync(join(output, "onda-logo-dark.svg"), lockupSvg("#F2F2F3"));
  const browser = await chromium.launch();
  try {
    for (const theme of ["light", "dark"]) {
      const file = join(studioDirectory, `studio-${theme}.html`);
      writeFileSync(file, studio(theme));
      const context = await browser.newContext({ viewport: { width: 1300, height: 900 }, deviceScaleFactor: 2, colorScheme: theme });
      const page = await context.newPage();
      await page.goto(url(file));
      await page.evaluate(async () => {
        await document.fonts.ready;
      });
      for (const id of ["hero", "how", "privacy", "pills", "screen-history", "screen-stats", "screen-dictionary", "screen-speech"]) {
        await page.locator(`#${id}`).screenshot({ path: join(output, `${id}-${theme}.png`), omitBackground: true });
      }
      await context.close();
    }
    const file = join(studioDirectory, "social.html");
    writeFileSync(file, social);
    const context = await browser.newContext({ viewport: { width: 1280, height: 640 }, deviceScaleFactor: 1 });
    const page = await context.newPage();
    await page.goto(url(file));
    await page.evaluate(async () => {
      await document.fonts.ready;
    });
    await page.locator("#social").screenshot({ path: join(output, "social-preview.png") });
    await context.close();
  } finally {
    await browser.close();
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) await render({ recapture: !process.argv.includes("--no-capture") });
