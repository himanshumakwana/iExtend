# iExtend Pencil-Feel Test Protocol

**Version:** 1.0  
**Applies to:** iExtend v1.0 and later  
**Reviewers:** 10 (minimum 8 to constitute a valid run)  
**Estimated time:** 45–60 minutes per reviewer including setup and survey

---

## 1. Purpose and scope

This protocol measures the subjective quality of the Apple Pencil experience
when drawing on an iPad that is connected to an iExtend host. We compare three
apps side by side:

| App | Platform | Role |
|---|---|---|
| **Adobe Photoshop** (desktop) | Windows/Linux host (via iExtend) | Production creative workload |
| **Procreate** | iPad native | Gold standard / control condition |
| **Krita** (desktop) | Windows/Linux host (via iExtend) | Open-source alternative |

The outcome determines whether iExtend meets the subjective Pencil-feel bar for
v1 release.

**Pass bar:**
- Per-app mean score ≥ 4.0 on every Likert axis (5-point scale).
- No individual app below 3.5 mean on any axis.
- No individual reviewer score below 2 on any axis without an actionable
  comment explaining the specific failure.

---

## 2. Recruitment

**Target:** 10 reviewers from three user groups:

| Group | Count | Recruiting notes |
|---|---|---|
| Digital illustrators (professional or semi-pro) | 4 | Freelancers, concept artists, UX designers who use Procreate or Photoshop daily |
| Note-takers / students | 3 | GoodNotes or Notability power users; mixed pressure/tilt usage |
| CAD / technical users | 3 | Engineers using technical drawing apps; fine detail, straight lines |

**Honorarium:** $80–$120 per reviewer (payable via bank transfer or gift card
within 14 days of the session). Disclose the amount before sign-up.

**Recruitment timeline:** Open calls 3 weeks before the planned review date.
Confirm headcount by 1 week out; reschedule if fewer than 8 confirmed.

**Exclusion criteria:**
- Employees of iExtend or its parent company.
- Reviewers who have provided feedback in a previous round (to avoid anchoring).
- Reviewers who have never used an Apple Pencil before.

---

## 3. Apparatus

### Hardware

- **Host:** A fully provisioned iExtend box matching one of the four
  hardware-CI cluster boxes (see `bench/cluster/`). Record the box hostname,
  GPU model, and `iextendd --version` output in the session log.
- **iPad:** iPad Pro M2 11" with Apple Pencil Pro.
- **Connections:** Wi-Fi 6E run first; USB-C run second (for each reviewer).
  Use the same physical AP as the hardware-CI cluster.
- **Monitor:** The host's primary display must be visible to the reviewer so
  they can see what is "on the remote desktop."

### Software

- iExtend host daemon at the candidate release commit (record git SHA).
- Adobe Photoshop (latest stable) — configured for Wintab stylus input.
- Krita (latest stable) — configured for libinput stylus passthrough.
- Procreate (latest App Store version) — native; no iExtend involved.

### Calibration check before each session

1. `iextendd ctl status --json` → confirm a single paired iPad is shown.
2. Run the synthetic latency test locally: `cargo test -p iextendd --test synthetic_latency --release`.
   Confirm p99 < 30 ms; if not, do not proceed — the box may be overloaded.
3. Verify the Apple Pencil Pro tip is < 2 months old; replace if worn.
4. Confirm the iPad screen is clean (no fingerprint smudges on the glass near
   the drawing area).

---

## 4. Session structure

Each reviewer completes the following sequence independently, with a proctor
present to answer setup questions (but not to comment on feel or quality).

### 4.1 Briefing (5 min)

1. Explain the purpose: "We're measuring how natural the Apple Pencil feels
   when drawing on an app running on a remote computer via Wi-Fi and USB-C."
2. Explain the Likert scale: 1 = very bad, 5 = excellent.
3. Remind the reviewer that there are no right or wrong answers; their honest
   perception is what matters.
4. Obtain signed consent (see `consent-form.md`).

### 4.2 Warm-up (5 min)

Free drawing in Procreate (native, no iExtend). Goal: let the reviewer calibrate
their sense of "normal" and get comfortable with the Pencil.

### 4.3 App rotations (30 min total, ~10 min per app)

The app order is randomised per reviewer (Latin square across 10 reviewers) to
control for order effects. The proctor records the order.

For each app, the reviewer completes four tasks:

| Task | Description | Duration |
|---|---|---|
| a. Freehand sketch | Continuous gesture sketch of an object or scene of their choice | 3 min |
| b. Careful inking | Deliberate thin-line inking pass over their sketch; assess line smoothness | 3 min |
| c. Pressure-sensitive shading | Circular gradients and hatching; assess pressure response | 2 min |
| d. Hatch + stipple | Fine detail: hatching, stippling, cross-contour lines | 2 min |

Between Wi-Fi 6E and USB-C runs, give the reviewer a 5-minute break.

### 4.4 Survey (10 min)

After both runs (Wi-Fi 6E + USB-C) for each app, the reviewer fills in the
survey (Likert scores + open text). The proctor does not hover.

The survey is administered via the Google Form whose template is in
`google-form.template.json`. Export responses to CSV before the session ends.

### 4.5 Debrief (5 min)

Thank the reviewer. Ask one open question: "Is there anything else about the
Pencil feel that you wanted to mention that the form didn't capture?" Note the
answer in the session log (anonymised).

---

## 5. Likert axes (5-point scale)

| Axis | 1 | 3 | 5 |
|---|---|---|---|
| **Stroke smoothness** | Jittery, staircase artefacts | Some wobble in slow strokes | Perfectly smooth at all speeds |
| **Perceived latency** | Obvious cursor lag | Slightly behind the tip | Tip and cursor coincide |
| **Pressure response** | Pressure changes ignored or abrupt | Somewhat accurate | Linear, responsive, fine gradients |
| **Tilt response** | Tilt ignored or wrong direction | Mostly correct | Accurate, nuanced shading |
| **Overall feel** | Unusable for creative work | Acceptable but noticeable | Indistinguishable from native |

Reviewers assign one score per axis per app (Wi-Fi 6E) and one per axis per app
(USB-C). The form collects these as separate items.

---

## 6. Pass/fail rules and escalation

### 6.1 Automatic pass

All per-app means ≥ 4.0 across all axes, for both Wi-Fi 6E and USB-C runs.

### 6.2 Soft fail

Any per-app mean between 3.5 and 3.99. Action: investigate the axis and
determine if it is a systematic regression or a single outlier reviewer.
If a single reviewer drove the score below 4.0 due to an unexplained
personal preference (e.g. "I always score pressure response 3 everywhere"),
that reviewer may be excluded from the aggregate with lead-engineer sign-off
and documented justification.

### 6.3 Hard fail

Any per-app mean below 3.5, or any individual reviewer score below 2 on any
axis without a comment. Action: **hold the release.** File a bug, investigate
the root cause (encoder latency? Wintab passthrough? Apple Pencil Pro palm
rejection?), fix, and re-run the full protocol.

### 6.4 Escalation matrix

| Condition | Action | Owner |
|---|---|---|
| p99 > 30 ms on the synthetic test | Do not start the review session | Eng |
| Any soft fail | Document, get lead-eng sign-off, proceed if justified | Lead eng |
| Any hard fail | Block release, file bug, re-run after fix | Team |
| Fewer than 8 reviewers complete the session | Reschedule; do not extrapolate from < 8 | PM |

---

## 7. Data handling and anonymisation

- Reviewer IDs are assigned sequentially: `R01`, `R02`, …, `R10`.
- Real names are never written into the results CSV.
- The consent forms (signed, with real names) are stored separately from the
  results CSVs, in a physical folder labelled with the release tag.
- Results CSVs are committed to `bench/pencil-feel/results/<release-tag>.csv`.
  They contain only `reviewer_id`, `app`, `connection`, and Likert scores plus
  anonymised open-text comments.
- Audio or video recording of the session requires additional explicit consent
  (separate checkbox on the consent form). If not granted, do not record.

---

## 8. Analysis

Run `python analyze.py results/<release-tag>.csv` to get the per-app mean table
and pass/fail determination.  The script also writes a
`results/<release-tag>.summary.md` for inclusion in the release checklist.

---

## 9. Appendix — app configuration checklist

### Adobe Photoshop (Windows host)

- Preferences → Technology Previews → **Activate Stylus Input**: On
- Preferences → Cursors → **Painting Cursors**: Normal Brush Tip
- Wintab driver installed (bundled with Photoshop; verify in `C:\Program Files\Tablet Driver\`)

### Krita (Windows/Linux host)

- Settings → Configure Krita → Tablet → **Tablet Input API**: WinTab (Windows) or libinput (Linux)
- Settings → Configure Krita → Tablet → **Use mouse button events when stylus is not pressed**: Off

### Procreate (iPad native)

- No special configuration; use default settings to ensure the reviewer's
  experience matches what any Procreate user would get.
