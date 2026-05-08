# iExtend.html Visual Deliverable Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a single self-contained `iExtend.html` at the repo root that renders the full Figma-style design canvas (15 artboards, 4 sections, live Tweaks panel) with pan/zoom/drag-reorder/fullscreen-focus, suitable for sharing as a static design artifact with no build step.

**Architecture:** Inline all 7 React + Babel source files from the Claude Design bundle (`design-canvas.jsx`, `tweaks-panel.jsx`, `icons.jsx`, `frames.jsx`, `scenes-ipad.jsx`, `scenes-pc.jsx`, `app.jsx`) into a single HTML document as `<script type="text/babel">` blocks, in dependency order. Babel Standalone transpiles them in the browser; React 18 + ReactDOM mount the App. No bundler, no build step, no separate `.jsx` fetches. A Playwright smoke + interaction test suite verifies the result.

**Tech Stack:**
- HTML/CSS (verbatim from design bundle)
- React 18.3.1 + ReactDOM 18.3.1 (UMD from unpkg, SRI-pinned)
- Babel Standalone 7.29.0 (UMD from unpkg, SRI-pinned)
- Playwright 1.48+ (test runner) — Node 20+

**Plan scope:** This is **Plan 1 of 10** for the iExtend project. It produces only the visual deliverable. The host daemon, iPad app, drivers, pairing flow, and installer are out of scope for this plan and tracked in separate plans.

**Source bundle location:** A pre-extracted copy of the Claude Design bundle is at `/tmp/iextend-design/iextend/`. Task 1 copies it into the repo as `design-source/` so it's stable.

---

## File Structure

This plan creates a small, focused tree:

```
iExtend/
├── iExtend.html                          # the deliverable (built incrementally tasks 5–11)
├── README.md                             # how to view + run tests
├── package.json                          # one dev dep: @playwright/test
├── .gitignore
├── playwright.config.ts
├── tests/
│   └── iextend.spec.ts                   # smoke + interaction tests
└── design-source/                        # frozen copy of the Claude Design bundle
    ├── README.md
    ├── chats/
    │   └── chat1.md
    └── project/
        ├── iExtend.html                  # original multi-file shell (reference only)
        ├── design-canvas.jsx
        ├── ios-frame.jsx
        ├── tweaks-panel.jsx
        └── src/
            ├── app.jsx
            ├── frames.jsx
            ├── icons.jsx
            ├── scenes-ipad.jsx
            └── scenes-pc.jsx
```

The deliverable is `iExtend.html`. Everything else exists to verify it works and to make rebuilding it reproducible.

**Inlining order is fixed by the original document** (see `design-source/project/iExtend.html` lines 255–261):

1. `design-canvas.jsx`
2. `tweaks-panel.jsx`
3. `src/icons.jsx`
4. `src/frames.jsx`
5. `src/scenes-ipad.jsx`
6. `src/scenes-pc.jsx`
7. `src/app.jsx`

Do not change this order. Later files reference globals (`window.IPad`, `window.I`, etc.) registered by earlier ones.

---

### Task 1: Initialize the repository

**Files:**
- Create: `/home/tops/Projects/iExtend/.gitignore`
- Create: `/home/tops/Projects/iExtend/package.json`
- Create: `/home/tops/Projects/iExtend/README.md`
- Copy: `/tmp/iextend-design/iextend/` → `/home/tops/Projects/iExtend/design-source/`

- [ ] **Step 1: `git init`**

```bash
cd /home/tops/Projects/iExtend
git init -b main
```

Expected: `Initialized empty Git repository in /home/tops/Projects/iExtend/.git/`

- [ ] **Step 2: Write `.gitignore`**

Create `/home/tops/Projects/iExtend/.gitignore`:

```
node_modules/
.superpowers/
test-results/
playwright-report/
/blob-report/
.playwright/
*.log
.DS_Store
```

- [ ] **Step 3: Write `package.json`**

Create `/home/tops/Projects/iExtend/package.json`:

```json
{
  "name": "iextend",
  "version": "0.0.1",
  "private": true,
  "description": "iExtend — iPad as a wireless second screen for Windows/Linux",
  "scripts": {
    "test": "playwright test",
    "test:headed": "playwright test --headed",
    "view": "python3 -m http.server 8080"
  },
  "devDependencies": {
    "@playwright/test": "^1.48.0"
  }
}
```

- [ ] **Step 4: Write `README.md`**

Create `/home/tops/Projects/iExtend/README.md`:

```markdown
# iExtend

iExtend turns an iPad into a wireless second screen for Windows and Linux laptops.

## What's in this repo right now

This is **Plan 1 of 10**: only the visual design deliverable. The runtime application
(Rust host daemon, iPad app, drivers) is tracked in subsequent plans — see
`docs/superpowers/plans/`.

- `iExtend.html` — single-file Figma-style design canvas (15 artboards, 4 sections,
  live tweaks panel). Open in any modern browser.
- `design-source/` — frozen copy of the Claude Design bundle this was built from.
- `docs/superpowers/specs/` — engineering design spec.
- `tests/` — Playwright tests verifying iExtend.html renders and is interactive.

## Viewing iExtend.html

```bash
npm run view
# then open http://localhost:8080/iExtend.html
```

Opening the file directly with `file://` works in Firefox but Chrome blocks
cross-origin font loads under `file://`. Always use the local server.

## Running the tests

```bash
npm install
npx playwright install chromium
npm test
```
```

- [ ] **Step 5: Copy the design-source bundle**

```bash
cp -r /tmp/iextend-design/iextend /home/tops/Projects/iExtend/design-source
ls /home/tops/Projects/iExtend/design-source/project/src/
```

Expected output: `app.jsx  frames.jsx  icons.jsx  scenes-ipad.jsx  scenes-pc.jsx`

- [ ] **Step 6: Install Playwright**

```bash
cd /home/tops/Projects/iExtend
npm install
npx playwright install chromium
```

Expected: `node_modules/` populated; chromium binary installed; no errors. On CI or fresh sandboxes that lack system libraries, append `--with-deps` (requires sudo).

- [ ] **Step 7: Commit**

```bash
cd /home/tops/Projects/iExtend
git add .gitignore package.json package-lock.json README.md design-source
git commit -m "chore: initialize iextend repo with design-source bundle"
```

---

### Task 2: Write the failing smoke test

**Files:**
- Create: `/home/tops/Projects/iExtend/playwright.config.ts`
- Create: `/home/tops/Projects/iExtend/tests/iextend.spec.ts`

- [ ] **Step 1: Write `playwright.config.ts`**

Create `/home/tops/Projects/iExtend/playwright.config.ts`:

```ts
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  timeout: 30_000,
  expect: { timeout: 10_000 },
  fullyParallel: true,
  reporter: [['list']],
  use: {
    baseURL: 'http://127.0.0.1:8080',
    trace: 'retain-on-failure',
    viewport: { width: 1440, height: 900 },
  },
  webServer: {
    command: 'python3 -m http.server 8080',
    port: 8080,
    reuseExistingServer: !process.env.CI,
    timeout: 5_000,
  },
  projects: [
    {
      name: 'chromium',
      // Desktop Chrome's default viewport is 1280x720, which would override the 1440x900
      // top-level `use.viewport`. Re-apply it explicitly so coordinate-sensitive tests
      // (the pan test drags at y=850) hit the right pixels.
      use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 900 } },
    },
  ],
});
```

- [ ] **Step 2: Write the smoke test**

Create `/home/tops/Projects/iExtend/tests/iextend.spec.ts`:

```ts
import { test, expect } from '@playwright/test';

test.describe('iExtend.html', () => {
  test('loads without JS errors', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (e) => errors.push(e.message));
    page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
    await page.goto('/iExtend.html');
    await page.waitForLoadState('networkidle');
    expect(errors, `console/page errors: ${errors.join(' | ')}`).toEqual([]);
  });

  test('renders 15 artboards across 4 sections', async ({ page }) => {
    await page.goto('/iExtend.html');
    await page.waitForSelector('[data-dc-section]', { timeout: 10_000 });
    const sections = await page.locator('[data-dc-section]').count();
    expect(sections).toBe(4);
    const artboards = await page.locator('[data-dc-slot]').count();
    expect(artboards).toBe(15);
  });

  test('canvas pans on background drag', async ({ page }) => {
    await page.goto('/iExtend.html');
    await page.waitForSelector('[data-dc-section]');
    const world = page.locator('.design-canvas > div').first();
    const before = await world.evaluate((el) => (el as HTMLElement).style.transform);
    // (40, 850) is below the artboard row at the 1440x900 viewport — empty canvas
    // background. The DCViewport pan handler triggers on primary-button drag started
    // outside any [data-dc-slot]. Drag right by 100px → world should translate by ~100px.
    await page.mouse.move(40, 850);
    await page.mouse.down();
    await page.mouse.move(140, 850, { steps: 5 });
    await page.mouse.up();
    const after = await world.evaluate((el) => (el as HTMLElement).style.transform);
    expect(after).not.toBe(before);
    const xBefore = Number(before.match(/translate3d\((-?[\d.]+)px/)?.[1] ?? '0');
    const xAfter  = Number(after .match(/translate3d\((-?[\d.]+)px/)?.[1] ?? '0');
    expect(xAfter - xBefore).toBeGreaterThan(95);
    expect(xAfter - xBefore).toBeLessThan(105);
  });

  test('canvas zooms on ctrl+wheel', async ({ page }) => {
    await page.goto('/iExtend.html');
    await page.waitForSelector('[data-dc-section]');
    const world = page.locator('.design-canvas > div').first();
    const before = await world.evaluate((el) => (el as HTMLElement).style.transform);
    // (720, 450) is the viewport center; ctrl+wheel zooms toward the cursor.
    // Notched-mouse-wheel branch in DCViewport applies a fixed exp(0.18) ≈ 1.197x step.
    await page.mouse.move(720, 450);
    await page.keyboard.down('Control');
    await page.mouse.wheel(0, -120);
    await page.keyboard.up('Control');
    // Allow the synchronous transform write + style-recalc to land before reading.
    await page.waitForTimeout(150);
    const after = await world.evaluate((el) => (el as HTMLElement).style.transform);
    expect(after).not.toBe(before);
    const sBefore = Number(before.match(/scale\(([\d.]+)\)/)?.[1] ?? '1');
    const sAfter  = Number(after .match(/scale\(([\d.]+)\)/)?.[1] ?? '1');
    expect(sAfter / sBefore).toBeGreaterThan(1.15);
    expect(sAfter / sBefore).toBeLessThan(1.25);
  });

  test('artboard labels are visible', async ({ page }) => {
    await page.goto('/iExtend.html');
    await page.waitForSelector('[data-dc-slot]');
    await expect(page.getByText('iPad · Welcome').first()).toBeVisible();
    await expect(page.getByText('iPad · Live (extended)').first()).toBeVisible();
    await expect(page.getByText('PC · Arrangement').first()).toBeVisible();
  });

  test('section titles match the spec', async ({ page }) => {
    await page.goto('/iExtend.html');
    await page.waitForSelector('[data-dc-section]');
    for (const t of ['Onboarding', 'Connected', 'Settings & errors', 'Floating toolbar variants']) {
      await expect(page.getByText(t).first()).toBeVisible();
    }
  });
});
```

- [ ] **Step 3: Run the test, verify it fails**

```bash
cd /home/tops/Projects/iExtend
npx playwright test
```

Expected: every test fails. The first one with `Error: page.goto: net::ERR_FILE_NOT_FOUND` or a 404, because `iExtend.html` does not exist yet. This is the desired starting state.

- [ ] **Step 4: Commit**

```bash
git add playwright.config.ts tests/
git commit -m "test: add playwright smoke + interaction suite (failing)"
```

---

### Task 3: Build the iExtend.html shell

**Files:**
- Create: `/home/tops/Projects/iExtend/iExtend.html`

The shell is the head + style + React/Babel UMD scripts, with **no inlined JSX yet**. The file should load in a browser without errors but render an empty `<div id="root">`.

- [ ] **Step 1: Read the original shell to copy verbatim**

Read `/home/tops/Projects/iExtend/design-source/project/iExtend.html`. The file is 264 lines; lines 1–249 are head + body shell (everything up to and including `<div id="root"></div>` and the React/ReactDOM/Babel `<script>` tags). Lines 255–261 are the seven `<script type="text/babel" src="...">` lines you will *replace* in the next tasks.

- [ ] **Step 2: Create iExtend.html with the shell**

Create `/home/tops/Projects/iExtend/iExtend.html`. Copy lines 1–253 of `design-source/project/iExtend.html` verbatim — that is everything from `<!doctype html>` through the closing `</script>` of the `babel.min.js` UMD include. Then append:

```html

<!-- inlined sources are appended below in dependency order -->

</body>
</html>
```

The file should now end with `</body></html>` and contain no `<script type="text/babel" src=...>` tags. The expected line count is around 256 lines.

Verify:

```bash
cd /home/tops/Projects/iExtend
grep -c 'script type="text/babel"' iExtend.html
```

Expected: `0` (no babel scripts yet).

```bash
grep '<script src=' iExtend.html | wc -l
```

Expected: `3` (React, ReactDOM, Babel UMD).

- [ ] **Step 3: Smoke-load the shell manually**

```bash
cd /home/tops/Projects/iExtend
python3 -m http.server 8080 &
SERVER_PID=$!
sleep 1
curl -sI http://127.0.0.1:8080/iExtend.html | head -1
kill $SERVER_PID
```

Expected: `HTTP/1.0 200 OK`.

- [ ] **Step 4: Commit**

```bash
git add iExtend.html
git commit -m "feat: add iExtend.html shell (head + react/babel UMD, empty root)"
```

---

### Task 4: Inline `design-canvas.jsx`

**Files:**
- Modify: `/home/tops/Projects/iExtend/iExtend.html`

- [ ] **Step 1: Append design-canvas.jsx as an inline babel script**

Open `iExtend.html`. Find the comment line `<!-- inlined sources are appended below in dependency order -->`. Immediately after it, insert:

```html
<script type="text/babel" data-source="design-canvas.jsx">
/* === BEGIN design-canvas.jsx === */
[paste the entire contents of design-source/project/design-canvas.jsx here verbatim, all 937 lines]
/* === END design-canvas.jsx === */
</script>
```

The exact bytes between the BEGIN and END markers must match `design-source/project/design-canvas.jsx` byte-for-byte. Do not edit, reformat, or "improve" the source.

- [ ] **Step 2: Verify the inlining**

```bash
cd /home/tops/Projects/iExtend
diff <(sed -n '/=== BEGIN design-canvas.jsx ===/,/=== END design-canvas.jsx ===/p' iExtend.html | sed '1d;$d') design-source/project/design-canvas.jsx
```

Expected: no output (files match).

- [ ] **Step 3: Verify no JS errors loading the page**

```bash
npx playwright test --grep "loads without JS errors"
```

Expected: `1 passed`. (The other tests still fail — that's correct; nothing renders yet because there's no `<App/>` root.)

- [ ] **Step 4: Commit**

```bash
git add iExtend.html
git commit -m "feat: inline design-canvas.jsx into iExtend.html"
```

---

### Task 5: Inline `tweaks-panel.jsx`

**Files:**
- Modify: `/home/tops/Projects/iExtend/iExtend.html`

- [ ] **Step 1: Append the second babel script**

After the `=== END design-canvas.jsx ===` script's closing `</script>` tag, append:

```html
<script type="text/babel" data-source="tweaks-panel.jsx">
/* === BEGIN tweaks-panel.jsx === */
[paste the entire contents of design-source/project/tweaks-panel.jsx here verbatim, all 569 lines]
/* === END tweaks-panel.jsx === */
</script>
```

- [ ] **Step 2: Verify**

```bash
diff <(sed -n '/=== BEGIN tweaks-panel.jsx ===/,/=== END tweaks-panel.jsx ===/p' iExtend.html | sed '1d;$d') design-source/project/tweaks-panel.jsx
```

Expected: no output.

- [ ] **Step 3: Smoke test still passes**

```bash
npx playwright test --grep "loads without JS errors"
```

Expected: `1 passed`.

- [ ] **Step 4: Commit**

```bash
git add iExtend.html
git commit -m "feat: inline tweaks-panel.jsx into iExtend.html"
```

---

### Task 6: Inline `icons.jsx`

**Files:**
- Modify: `/home/tops/Projects/iExtend/iExtend.html`

- [ ] **Step 1: Append**

After the previous script, append:

```html
<script type="text/babel" data-source="src/icons.jsx">
/* === BEGIN icons.jsx === */
[paste the entire contents of design-source/project/src/icons.jsx here verbatim, all 179 lines]
/* === END icons.jsx === */
</script>
```

- [ ] **Step 2: Verify**

```bash
diff <(sed -n '/=== BEGIN icons.jsx ===/,/=== END icons.jsx ===/p' iExtend.html | sed '1d;$d') design-source/project/src/icons.jsx
```

Expected: no output.

- [ ] **Step 3: Commit**

```bash
git add iExtend.html
git commit -m "feat: inline icons.jsx into iExtend.html"
```

---

### Task 7: Inline `frames.jsx`

**Files:**
- Modify: `/home/tops/Projects/iExtend/iExtend.html`

- [ ] **Step 1: Append**

After the previous script, append:

```html
<script type="text/babel" data-source="src/frames.jsx">
/* === BEGIN frames.jsx === */
[paste the entire contents of design-source/project/src/frames.jsx here verbatim, all 164 lines]
/* === END frames.jsx === */
</script>
```

- [ ] **Step 2: Verify**

```bash
diff <(sed -n '/=== BEGIN frames.jsx ===/,/=== END frames.jsx ===/p' iExtend.html | sed '1d;$d') design-source/project/src/frames.jsx
```

Expected: no output.

- [ ] **Step 3: Commit**

```bash
git add iExtend.html
git commit -m "feat: inline frames.jsx into iExtend.html"
```

---

### Task 8: Inline `scenes-ipad.jsx`

**Files:**
- Modify: `/home/tops/Projects/iExtend/iExtend.html`

- [ ] **Step 1: Append**

After the previous script, append:

```html
<script type="text/babel" data-source="src/scenes-ipad.jsx">
/* === BEGIN scenes-ipad.jsx === */
[paste the entire contents of design-source/project/src/scenes-ipad.jsx here verbatim, all 776 lines]
/* === END scenes-ipad.jsx === */
</script>
```

- [ ] **Step 2: Verify**

```bash
diff <(sed -n '/=== BEGIN scenes-ipad.jsx ===/,/=== END scenes-ipad.jsx ===/p' iExtend.html | sed '1d;$d') design-source/project/src/scenes-ipad.jsx
```

Expected: no output.

- [ ] **Step 3: Commit**

```bash
git add iExtend.html
git commit -m "feat: inline scenes-ipad.jsx into iExtend.html"
```

---

### Task 9: Inline `scenes-pc.jsx`

**Files:**
- Modify: `/home/tops/Projects/iExtend/iExtend.html`

- [ ] **Step 1: Append**

After the previous script, append:

```html
<script type="text/babel" data-source="src/scenes-pc.jsx">
/* === BEGIN scenes-pc.jsx === */
[paste the entire contents of design-source/project/src/scenes-pc.jsx here verbatim, all 268 lines]
/* === END scenes-pc.jsx === */
</script>
```

- [ ] **Step 2: Verify**

```bash
diff <(sed -n '/=== BEGIN scenes-pc.jsx ===/,/=== END scenes-pc.jsx ===/p' iExtend.html | sed '1d;$d') design-source/project/src/scenes-pc.jsx
```

Expected: no output.

- [ ] **Step 3: Commit**

```bash
git add iExtend.html
git commit -m "feat: inline scenes-pc.jsx into iExtend.html"
```

---

### Task 10: Inline `app.jsx` (entry point — page now renders)

**Files:**
- Modify: `/home/tops/Projects/iExtend/iExtend.html`

This is the last script and the one that calls `ReactDOM.createRoot(...).render(<App/>)`. After this task, the page renders the full design canvas.

- [ ] **Step 1: Append**

After the previous script, append:

```html
<script type="text/babel" data-source="src/app.jsx">
/* === BEGIN app.jsx === */
[paste the entire contents of design-source/project/src/app.jsx here verbatim, all 94 lines]
/* === END app.jsx === */
</script>
```

- [ ] **Step 2: Verify**

```bash
diff <(sed -n '/=== BEGIN app.jsx ===/,/=== END app.jsx ===/p' iExtend.html | sed '1d;$d') design-source/project/src/app.jsx
```

Expected: no output.

- [ ] **Step 3: Verify all 7 inlined scripts are present**

```bash
grep -c 'script type="text/babel"' /home/tops/Projects/iExtend/iExtend.html
```

Expected: `7`.

- [ ] **Step 4: Run the full test suite**

```bash
cd /home/tops/Projects/iExtend
npx playwright test
```

Expected: all 6 tests pass.

If any fail, do not "fix" by editing the inlined source. The most likely cause is a typo in the BEGIN/END markers, a missing newline, or a duplicate `<script>` tag. Re-run the verification diffs from each previous task; whichever diff produces output is the file with the bug.

- [ ] **Step 4.5: Address two test-infrastructure gaps surfaced by the integration**

These two fixes are necessary to make the suite pass and were not anticipated in earlier tasks:

1. **Create empty `.design-canvas.state.json` sidecar at the repo root.**
   `design-canvas.jsx` unconditionally fetches `./.design-canvas.state.json` on mount to restore saved order/labels/hidden-artboards. With no file present, the 404 surfaces as a console error (even though the JS handles `r.ok ? r.json() : null` gracefully) — which the "loads without JS errors" test treats as a failure. Create:

   ```bash
   echo '{}' > /home/tops/Projects/iExtend/.design-canvas.state.json
   ```

2. **Fix viewport override in `playwright.config.ts`.**
   `...devices['Desktop Chrome']` sets `viewport: { width: 1280, height: 720 }`, which overrides the top-level `use.viewport: { width: 1440, height: 900 }`. The pan test drags at y=850 — below the 720px viewport. Re-apply the 1440×900 viewport inside the project entry. The plan's playwright.config.ts code block in Task 2 has already been updated to reflect this; ensure the on-disk file matches.

- [ ] **Step 5: Commit**

```bash
cd /home/tops/Projects/iExtend
git add iExtend.html .design-canvas.state.json playwright.config.ts
git commit -m "feat: inline app.jsx into iExtend.html — full canvas now renders"
```

---

### Task 11: Manual visual smoke check

**Files:** none (manual verification step).

Automated tests verify structure, count, and basic interaction. This step verifies the result actually looks right.

- [ ] **Step 1: Start the local server**

```bash
cd /home/tops/Projects/iExtend
python3 -m http.server 8080
```

- [ ] **Step 2: Open in a browser**

Visit `http://127.0.0.1:8080/iExtend.html`. Verify visually:

1. Warm-gray grid background, four section titles ("Onboarding", "Connected", "Settings & errors", "Floating toolbar variants").
2. **Onboarding** section: 6 artboards — iPad Welcome, PC Searching, iPad Discover, PC Devices, iPad Pair, PC Show code.
3. **Connected** section: 3 artboards — iPad Connecting, iPad Live (extended), PC Arrangement.
4. **Settings & errors** section: 3 artboards — iPad Settings, iPad Disconnected, PC Connection lost.
5. **Floating toolbar variants** section: 3 artboards — Bottom · regular, Top · compact, Left · comfy.
6. Two-finger trackpad scroll (or click-drag on background) pans the canvas.
7. Trackpad pinch (or ctrl + wheel) zooms toward the cursor.
8. Hovering an artboard reveals a `⋯` menu and a focus button (top-right).
9. Clicking the focus button opens a fullscreen overlay with ←/→ arrows; Esc closes it.
10. Clicking the artboard header label puts it in inline-edit mode.

- [ ] **Step 3: If anything looks wrong, do not "fix" by editing iExtend.html**

Re-run the verification diffs from Tasks 4–10. If they all pass, the issue is in the source bundle, not the inlining. Stop and report.

- [ ] **Step 4: No commit needed (no file changes).**

---

### Task 12: README polish + final review

**Files:**
- Modify: `/home/tops/Projects/iExtend/README.md`

- [ ] **Step 1: Add the rebuild instructions**

Append to `/home/tops/Projects/iExtend/README.md`:

```markdown

## Rebuilding iExtend.html from design-source

`iExtend.html` is the inlined product of the seven `.jsx` files in
`design-source/project/`. If the design ever changes, follow
`docs/superpowers/plans/2026-05-08-iextend-html-visual-deliverable.md`
to rebuild — Tasks 3–10 are the mechanical inlining steps.

The inlining order is fixed by inter-file globals; do not change it:

1. `design-canvas.jsx`
2. `tweaks-panel.jsx`
3. `src/icons.jsx`
4. `src/frames.jsx`
5. `src/scenes-ipad.jsx`
6. `src/scenes-pc.jsx`
7. `src/app.jsx`

## Plan status

- [x] Plan 1: `iExtend.html` visual deliverable (this plan)
- [ ] Plan 2: Rust host workspace bootstrap
- [ ] Plan 3: Windows IddCx signed kernel driver
- [ ] Plan 4: Linux evdi integration + capture pipeline
- [ ] Plan 5: WebRTC transport + codec selection
- [ ] Plan 6: iPad Swift app shell + decode/render
- [ ] Plan 7: SPAKE2 pairing flow (host + iPad)
- [ ] Plan 8: Input forwarding + cursor reprojection
- [ ] Plan 9: Installers + codesigning
- [ ] Plan 10: Bench rig + CI hardware cluster
```

- [ ] **Step 2: Final test run**

```bash
cd /home/tops/Projects/iExtend
npx playwright test
```

Expected: all 6 tests pass. Total run time well under 30 seconds.

- [ ] **Step 3: Verify the deliverable is truly self-contained**

```bash
cd /home/tops/Projects/iExtend
# iExtend.html should reference no local .jsx files
grep -E 'src="[^h][^t][^t][^p]' iExtend.html | grep -v unpkg.com || echo "OK: no local src refs"
```

Expected: `OK: no local src refs`.

```bash
# Size sanity check — should be a single file under ~250 KB
wc -c iExtend.html
```

Expected: between 80,000 and 250,000 bytes.

- [ ] **Step 4: Final commit**

```bash
git add README.md
git commit -m "docs: add rebuild instructions and plan-status to README"
```

- [ ] **Step 5: Tag the milestone**

```bash
git tag -a plan-1-complete -m "Plan 1 of 10 complete: iExtend.html visual deliverable shipped"
```

---

## Done criteria

All of the following must be true to consider this plan complete:

1. `iExtend.html` exists at the repo root and renders the full design canvas in Chromium and Firefox.
2. `npx playwright test` reports 6/6 passing.
3. The file references zero local resources (only unpkg CDN URLs are external).
4. Each of the 7 inlined script blocks byte-matches its `design-source/` original.
5. The README documents how to view, test, and rebuild.
6. Git history has one commit per task; tag `plan-1-complete` is on the final commit.

## Out of scope (handled by later plans)

- Any Rust code (host daemon, tray, codec, WebRTC, mDNS, drivers)
- Any Swift code (iPad app, Kit, UI, Input modules)
- Windows IddCx kernel driver and EV codesigning
- Linux evdi DKMS package
- Pairing flow runtime
- Input forwarding runtime
- Installers
- Bench rig / CI hardware cluster

If a later plan needs to modify the visual design, it does so by editing
`design-source/` and re-running this plan's Tasks 3–10 to rebuild iExtend.html.
