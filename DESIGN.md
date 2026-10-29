# DESIGN.md: Onda

This is the living record of design decisions. Read it before any UI work, and update it when a
decision changes. Keep it short: decisions and their reasons, not essays. The approved concept
board is `docs/design/onda-concept.html` (image: `onda-concept.png`).

## Product
- Audience and primary job: people who write a lot on Windows and want to speak instead of type,
  anywhere their cursor is. The job happens in other apps; Onda's window is for setup, review and
  teaching it words. Optimise for the hundredth dictation, not the first launch.
- Surfaces: Windows desktop (Tauri 2, WebView2), and Linux (WebKitGTK) with fewer features; see
  docs/linux.md. Two webviews: the main window (`ui/index.html`) and the dictation pill
  (`ui/pill.html`).
- Register per surface: product. First-run setup borrows a little brand warmth.
- Desktop fidelity: branded-native. Custom identity in the content; native title bar (tinted),
  caption buttons, Snap, tray, keyboard and accessibility at the edges.

## Concept
"Your voice, typed where your cursor is." A quiet, monochrome instrument where the brand's wave
and cursor become the interface: they show when Onda is listening, writing, or needs you.
Graphics explain things wherever a paragraph would otherwise go.

## Typography
- Families and roles: Inter Variable for all interface text (bundled, matches the brand guide).
  Newsreader Variable only for the user's own words (transcripts, spoken phrases, diffs), so it is
  always clear whose words are on screen. Cascadia Mono for paths and executable names.
- Scale: 13 (smallest anywhere) / 14 (buttons, chips, secondary) / 15 (body) / 16 (row titles) /
  20 (section) / 30 (page title) / 22 serif for transcripts. Hero headline 30, setup title 34.
- Numerals, case, tracking rules: sentence case everywhere. Tabular numerals for counts, sizes and
  timers. Headings track tight (-0.018em); body text normal. `text-wrap: pretty` on short text.

## Color and surfaces
- Canvas, surfaces, text levels, borders: brand greys only. Base `#F3F3F4` (sidebar and title bar),
  panel `#FFFFFF`, sunken `#F5F5F6`, lines `#E2E2E5`/`#CDCDD2`. Text `#0F0F10`, `#45454B`
  (9.5:1), `#66666D` (5.7:1, the faintest allowed for meaningful text).
- Accent, and what it is reserved for: none. The brand has no colour; ink (`--fg`) fills primary
  buttons, selection and focus. The graphite tile gradient (`#262628 → #0F0F10 → #040404`) marks
  brand moments: the hero, the pill, setup's art panel, empty-state and learning icons.
- Semantic colors: green ok, amber warn, red error. Status only, never decoration.
- Dark mode and high-contrast notes: dark is its own palette (base `#0B0B0C`, panel `#161618`),
  not an inversion, with the same contrast targets. Theme follows Windows by default; the
  setting drives the native window theme, which drives the webview's `prefers-color-scheme`.
  `forced-colors` adds outlines and borders.

## Geometry
- Radii, borders, shadows: controls 10, cards 16, hero 22, pill fully round, dialogs 18. 1px
  hairlines instead of shadows for in-flow surfaces; shadows only on floating layers and keycaps.
- Control heights, spacing rhythm, content widths: controls 38 (small 32), rows at least 72,
  4px spacing grid, sidebar 252, page content max 920 with 44px side padding.

## Layout and navigation
- Layout philosophy: sidebar plus one scrolling page. Home first answers "how do I dictate?",
  then "what did I just say?" and "what has Onda learned?". Stats, second in the sidebar, holds
  the long view: lifetime totals, streaks and a year of activity. Settings are grouped the way people
  think (General, Shortcuts, Microphone, Speech and language, AI polish, App styles, Privacy,
  Updates and help), with common choices first and advanced options last, behind disclosures.
- Navigation model; breakpoints or window size classes and what collapses when: settings swap the
  sidebar for their own section list with a Back control. Below 1040px the hero drops its
  illustration; below 960px grids stack and setup hides its art panel; below 900px the sidebar
  collapses to icons. Minimum window 760×560.

## Interaction and keyboard model
- Primary interactions: dictation happens through global shortcuts in other apps. In the app,
  edits save immediately (optimistic, rolled back with a message if native validation refuses).
  Deletions are immediate with Undo in a toast; nothing reversible asks for confirmation.
- Command palette and shortcuts: Ctrl+K searches pages, settings and actions; Ctrl+, opens
  settings; Esc closes dialogs. Esc stays reserved for cancelling dictation globally. Shortcuts are
  shown as keycaps wherever they are mentioned. Dev builds only: Ctrl+Shift+F9 toggles dictation
  through IPC, because the shortcut helper ignores injected keys by design.
- Context menus, drag and drop, multi-select: the browser context menu is suppressed outside
  text fields in production. No drag and drop or multi-select yet.

## Motion vocabulary
- Easing and durations: `cubic-bezier(0.2, 0.8, 0.2, 1)`; 120ms hover, 180ms state, 260ms
  entrances. Transform and opacity only.
- What moves, and why: the pill's wave follows the live microphone level (the only continuous
  motion, and only while recording). The pill enters with a small rise, settles into the cursor
  while writing out, and fades before the native window hides. The Home scene types its sample
  sentence once per visit. Nothing animates while idle.
- Reduced-motion behavior: all durations drop to 0; waves render still; carets stop blinking.

## Signature moments
- The pill: the Onda mark come alive, a graphite capsule above the taskbar on the monitor under
  the cursor. It never takes focus and is click-through, except the error card, which offers Copy
  when the text wasn't typed and one fix-it action that opens the right page.
- The hero: the user's real shortcut drawn as physical keys next to a preview of the result.
- Flow diagrams in Speech and AI polish that show where audio and text go.

## Deliberately quiet, and don'ts
- No accent colour, gradients, glows or glass in app chrome; the graphite tile is the only
  "material". No dashboards of filler: stats are three tiles of real, content-free counts.
- Never show provisional ChatGPT text as final; it is styled as in progress until validated.
- Never persist user text in WebView storage, and never render text as HTML.
- Don't recolour or redraw the logo (`ui/src/components/Brand.tsx` uses the master paths).
  The live wave is a UI element in the mark's stroke language, not the logo.

## Tokens
- Where they live in code: `ui/src/styles/tokens.css` (all colour, type, space, radius and motion
  tokens), `base.css` (resets, typography), `controls.css` (primitives), `capsule.css` (pill
  surface, shared by both webviews), `shell.css`, `pages.css`, `pill.css`. Native chrome colours
  mirror `--base` in `src-tauri/src/lib.rs` (`LIGHT_CHROME`, `DARK_CHROME`).

## Decision log
- 2026-10-23: Product renamed Onda; identifier `com.miille.onda`. Data from `org.eagle.dictation`
  is moved on first launch, or used in place if the old folder is still in use.
- 2026-10-23: Direction A ("paper and ink") adopted on the brand: monochrome, higher contrast
  (13px minimum text, 5.7:1 minimum text contrast), because the brand has no colour and the
  first draft read too small and faint.
- 2026-10-23: Preact + signals, hand-written CSS, bundled fonts, Lucide icons, because the
  renderer must stay small and fast inside WebView2 and the CSP forbids remote assets.
- 2026-10-23: Native title bar kept and tinted through DWM rather than a custom one, because it
  keeps Snap Layouts, caption buttons and accessibility for free.
- 2026-10-23: The pill is shown and hidden with non-activating Win32 calls, because tao's
  `show()` activates the window after creation and stole focus from the target field.
- 2026-10-23: Hands-free shows the finish shortcut instead of Stop/Cancel buttons, because a
  clickable pill would take focus from the app being dictated into.
- 2026-10-23: History page, an "Earlier" list on Home, and a Copy button on the pill's error card,
  because a failed insertion used to block the next dictation until the user cleared it.
  Retention lives in Privacy (default 3 days); entries are encrypted on disk (ADR 005).
- 2026-10-23: AI polish fixed at the protocol layer (plan streams have no Content-Type and an
  empty completion `output`) and the preservation check relaxed so number formatting and
  abbreviations may change, because every polish, rewrite and voice edit was falling back.
- 2026-10-24: "Say it to teach Onda" on dictionary words: three short takes, and whatever the
  local model writes instead becomes an opt-out list of replacements, because speech models take
  spelling hints rather than pronunciations (Dragon's spoken-form training has no Whisper
  equivalent).
- 2026-10-24: The main window is created when opened and destroyed when closed (the pill stays
  loaded), and the Home caret stops blinking after the sample is typed, because a hidden window
  kept ~100 MB of WebView renderer and an idle open window kept repainting.
- 2026-10-24: The main panel starts 6 px below the title bar, level with the sidebar's first
  control, so its rounded top edge reads as a panel rather than a clipped edge.
- 2026-10-24: Signed-out ChatGPT accounts show Remove instead of Sign out.
- 2026-10-24: A focused control re-created in place (same window, process, type and position)
  still counts as the same field, because Chromium-based apps rebuild it during polish and
  every polished dictation into them failed with "You switched fields".
- 2026-10-24: Settings saves are coalesced and sent one at a time, and the shortcut helper
  restarts only when shortcuts change, because rapid quality switching flickered the cards
  and the sidebar status.
- 2026-10-25: The pill's error card moves Dismiss to a corner × and keeps at most two
  actions, because "Copy ChatGPT's version", "Open History" and Dismiss overflowed its 340 px
  and hid Dismiss. When Onda cannot type, the words go on the clipboard and the card says
  so, because Ctrl+V otherwise pasted whatever was copied before. ChatGPT's version is
  offered only when its polish is what failed.
- 2026-10-25: Onboarding's Try it art loops a held shortcut (keys go down one by one, stay
  down while a bar fills and the pill listens, then release on "Let go to type"), and a tap
  shorter than 0.6 s gets its own "Hold to talk" message, because a tester tapped the keys and
  got "Didn't hear anything".
- 2026-10-25: The ChatGPT model picker names what "Recommended" means ("Recommended (GPT-6
  Luna)"), onboarding asks for the model once connected, and Luna counts as a light model,
  because the old hints (fast, mini, nano) matched none of the GPT-6 names and fell back to
  the first, possibly heaviest, model.
- 2026-10-25: When ChatGPT's text fails the preservation check, its version is kept beside the
  original: Copy ChatGPT's version on the pill and Home, and Show ChatGPT's version in History,
  marked word by word against the kept text. History still leads with the kept text because
  that is what is in the document, and the marks show whether the flagged change was harmless
  (GPT6 → GPT-6) or real (1,200 → 1,500).
- 2026-10-25: Dismiss on the pill or Home acknowledges the error, so the sidebar and tray read
  Ready again; it used to hide the banner only, and "Needs attention" stayed until the next
  dictation.
- 2026-10-25: Dropdowns are capped at 320 px with an ellipsis, and a wide one moves under its
  description in a narrow window, because a long microphone name squeezed the description
  into a one-word column on a 125% laptop.
- 2026-10-29: Stats page with lifetime words, speaking pace, current and longest streaks, active
  days and a 53-week activity grid, monochrome like the week chart (ink at four opacities). Every
  number is defined on the page, and nothing is estimated for days before Onda counted.
- 2026-10-29: A local profile (name and picture) greets by first name and can go on the shareable
  image. The image is a 1200×630 graphite card with the master lockup paths and Inter; name and
  picture are off until switched on in the share dialog, because sharing should only ever show
  what the user chose.
- 2026-10-29: Tray icon choice (Automatic, White, Black) in General, previewed on fixed light and
  dark swatches. Automatic follows the taskbar and now updates on theme changes even with the main
  window closed. Linux cannot report its panel colour, so there Automatic is white.
- 2026-10-29: AI polish chooses a provider from four cards: ChatGPT, OpenAI-compatible, Anthropic,
  On this computer. The "where your text goes" diagram names the real destination, and a local
  model gets the dashed "stays here" style. Saved keys are never shown again, only "Saved" with
  Remove.
- 2026-10-29: "Chinese" is labelled "Chinese (Mandarin)", with a note on Whisper's single Chinese
  setting, its scripts and Cantonese, because the old label suggested Cantonese might work.
- 2026-10-29: Updates and help reports Onda's total memory and CPU, because Task Manager files the
  WebView2 processes under "WebView2 Manager" and makes Onda look smaller than it is.
- 2026-10-29: The installer uses the brand: the black lockup in the page header, and the dark app
  icon with the white wordmark on the graphite tile for the Welcome and Finish pages.
- 2026-10-29: On Linux the hero shows the `onda --dictate` command instead of keycaps, and a
  finished dictation reads "Copied · press Ctrl+V", because Onda cannot type there yet.
- 2026-10-23: Quality cards say "In use" only for the model actually running; a card picked
  but downgraded for low memory says "Selected" and the footnote names the model in use.
