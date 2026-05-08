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
