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
