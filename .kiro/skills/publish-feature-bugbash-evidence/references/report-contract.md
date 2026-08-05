# Evidence Report Contract

Produce one self-contained HTML document a reviewer can inspect without the
checkout, test harness, or network-loaded assets.

## Required Metadata

Show near the top:

- report title and generation timestamp;
- repository and feature name;
- named visual direction plus the scene/reference brief and declared color,
  type, layout, motion, and voice choices;
- branch and full commit SHA;
- PR URL and linked issue URLs;
- evidence mode: deterministic UX, live LLM, or hybrid;
- mocked or injected boundary, if any;
- feature flags and launch path;
- engine, surface, OS, terminal size, and browser viewport matrix.

Long paths, SHAs, and URLs must wrap on narrow screens.

## Required Sections

1. **Verdict:** Passed, failed, or incomplete; computed story, check, and frame
   totals.
2. **Scope:** Requirements tested and explicit exclusions.
3. **Test boundary:** Real layers used and deterministic or mocked boundaries.
4. **Traceability:** Requirement -> story -> checks -> frames or tests.
5. **Stories:** Given, ordered When steps, Then assertions, checks, and frames.
6. **Regression proof:** Before and after evidence when regression attribution
   is claimed.
7. **Automated validation:** Focused tests, integration tests, typecheck, build,
   lint, and report-validator output actually run.
8. **Residual risk:** Untested model behavior, engines, surfaces, OSes, or
   timing.

Do not present pending CI as a passing validation item.

## Machine-Readable Markup

Prefer these attributes so validation does not depend on presentation classes:

```html
<article data-user-story="S01" data-status="pass">...</article>
<li data-evidence-check data-status="pass">...</li>
<figure data-evidence-frame="S01-03-input-recovered">...</figure>
```

Add one nonempty design marker so the visual decision remains inspectable
without reading the authoring session:

```html
<aside
  data-evidence-design
  data-direction="signal-ledger"
  data-color-strategy="committed"
  data-layout="dispatch"
  data-type-system="humanist-sans-plus-mono"
  data-motion="none"
  data-voice="test-ledger"
>
  Scene: A reviewer checks recovery controls in a dim operations room.
  Reference: a railway interlocking test ledger.
</aside>
```

Legacy Knight Rider reports using `article.story`, `.checks li`, and
`.terminal-frame` are also accepted by the bundled validator.

## Evidence Integrity

- Compute totals from the same story, check, and frame records used to render.
- Keep frame labels unique and ordered.
- Make every frame nonempty and identify its story, step, engine, and surface.
- Pair visual frames with programmatic state assertions for invisible behavior
  such as draft value, selection identity, or ownership flags.
- Include exact failure output for a failed check; never hide or omit it.
- Mark skipped stories separately and exclude them from pass totals.
- Reject stale branch, SHA, PR, issue, or report metadata.

## Design Quality

Apply the active `artifactory-design` skill rather than creating a parallel
design system here. Before authoring, write its scene/reference brief and
declare `strategy`, `palette`, `typeset`, `compose`, `cadence`, and `voice`.
After drafting, run its `slopcheck` and reject all absolute bans.

The design must strengthen evidence navigation: verdict first, compact metadata,
traceable stories, readable checks, and terminal frames that remain visually
authentic. Do not recolor captured terminal content. Do not clone a previous
report's visual direction by default; reuse the semantic structure while
choosing a reference appropriate to the feature and audience. Do not ship the
Artifactory design sheet's fallback palette or an undeclared generic
`system-ui` theme.

## Self-Contained HTML

- Inline CSS, JavaScript, terminal HTML, and raster data.
- Do not use external scripts, stylesheets, fonts, images, iframes, or media.
- Add `<base target="_blank">` for external links.
- Ensure same-page anchors still scroll locally with `target="_self"` or a
  local click handler. Artifactory also injects an anchor fix at upload time.
- Use semantic headings, navigation, main content, articles, lists, and figures.
- Respect reduced motion and keyboard focus.
- Make the page fluid from 320px through wide desktop.
- Put horizontal scrolling inside terminal frames, not on the document.
- Apply `overflow-wrap:anywhere` to paths, SHAs, and URLs.

## Validation Matrix

At minimum render:

| Viewport | Size     | Requirement                                           |
| -------- | -------- | ----------------------------------------------------- |
| Desktop  | 1440x900 | No document overflow; report and frames readable      |
| Mobile   | 390x844  | No document overflow; metadata wraps; controls usable |

Check:

- expected metadata strings are present;
- expected story and check counts match;
- frame count meets the declared total;
- no failed check elements exist;
- all frame elements have visible, nonempty content;
- every same-page anchor points to an existing ID;
- no subresource request leaves the file;
- no uncaught page error or failed resource request occurs;
- `scrollWidth <= clientWidth` at both viewports;
- prompts, paths, terminal output, and logs contain no credentials, tokens,
  customer data, or unrelated private content.

## PR Evidence Block

Use one current block:

```markdown
<!-- feature-bugbash-evidence -->

Feature bug bash evidence for `<full SHA>`:

- Stories: `<passed>/<total>`
- Checks: `<passed>/<total>`
- Frames: `<count>` nonempty
- Matrix: `<engine x surface x viewport>`
- Boundary: `<deterministic UX | live LLM | hybrid>`; `<injected boundary>`
- Tests: `<commands and results>`
- CI: `<live status, including pending/skipped>`
- Issues: `<links>`

Report: `<Artifactory artifactUrl>`
```

Edit this block when evidence changes. Do not append a second stale block.
