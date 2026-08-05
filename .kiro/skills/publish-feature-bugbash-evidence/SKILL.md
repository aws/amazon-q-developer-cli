---
name: publish-feature-bugbash-evidence
description: Plan and run feature-focused bug bashes, turn multi-step Given/When/Then user stories into Knight Rider evidence, validate self-contained HTML reports, publish them privately to Artifactory, and keep pull-request evidence current. Use for Kiro CLI feature validation, UX regression proof, activity-tray or workflow interaction audits, deterministic ACP/TUI testing, live-LLM bug bashes, Artifactory evidence uploads, or requests to attach visual test evidence to a PR.
---

# Publish Feature Bug Bash Evidence

Create reviewable proof for a feature or regression without mixing generated
evidence into the product change. Keep the tested revision, user stories,
checks, frames, report, upload, and PR reference traceable end to end.

## Non-Negotiable Defaults

- For queue, tray, workflow navigation, typing, deletion, rendering, or focus
  behavior, default to deterministic UX evidence unless model reasoning itself
  is a requirement.
- Use the real source TUI, Twinki, PTY input path, stores, and production event
  handlers. Inject only backend timing or state at the supported ACP test
  boundary.
- Call that boundary **deterministic UX integration**, not end to end. Reserve
  end-to-end claims for runs that include the real backend behavior under test.
- Publish through Artifactory `upload_artifact` or
  `upload_artifact_version`; do not invent or reverse-engineer an upload
  endpoint.
- Publish one self-contained HTML file, not a report bundle or archive.
- Before authoring HTML, load and apply the installed `artifactory-design` skill,
  write its required visual brief, and run its `slopcheck`; do not improvise a
  generic report theme or copy a prior report's palette.
- Tie every report and PR evidence block to the exact tested full SHA.
- Sanitize prompts, paths, logs, and frames before upload.

Whenever the user asks for a plan or handoff, name the validator command, the
Artifactory tool and private upload fields, and the stable PR evidence marker.
Do not replace these concrete steps with generic publication advice.

## Compose Existing Harness Guidance

Read the repository's current guidance before starting:

- `.kiro/skills/knight-rider/SKILL.md` for harness launch and HTTP endpoints.
- `.kiro/skills/tui-bug-bash/SKILL.md` for broad engine and surface audits.
- the active `artifactory-design` skill and its `slop.md` for report
  composition, typography, color, layout, semantics, and anti-patterns. It is
  installed by `artifactory-mcp setup`; if unavailable, complete that setup
  before building the report.
- `scripts/knight-rider.sh` for guarded startup and teardown.

Do not duplicate or override those mechanics here. Use this skill for the
feature-story, evidence-quality, publication, and PR-handoff layer.

## 1. Pin the Target

Record the repository, branch, full commit SHA, PR URL, linked issues, and
feature flags before testing. Confirm the PR head SHA matches the local
checkout. Stop if either moves during the run; rerun on the new SHA.

Query the live PR for the branch under test. Do not infer its number or head
from changelog fragments, prior reports, nearby worktrees, or remembered state.

Use an isolated worktree for product changes. Keep generated reports outside
the source tree unless the repository explicitly tracks them.

## 2. Write Multi-Step User Stories

Read [story-design.md](references/story-design.md), then group stories by
behavioral boundary rather than by source file. Give every story:

- a stable ID and short title;
- one concrete Given precondition;
- an ordered When sequence with more than one user action;
- intermediate assertions after state-changing actions;
- a final Then result;
- a recovery or negative-control assertion;
- the engines, surfaces, viewports, and feature flags it covers.

Cover plausible interactions, not a blind Cartesian product. Include adjacent
actions that can steal input, focus, ownership, selection, or lifecycle state.
For a regression, establish the prior behavior on the same inputs and identify
the introducing commit only when history or a before-build proves it.

## 3. Choose the Evidence Boundary

State one of these modes in the report:

- **Deterministic UX:** Use the real source TUI, renderer, raw PTY input, stores,
  and production event handlers. Inject deterministic backend events through
  the existing ACP or integration-test boundary. Use this when validating input
  routing, rendering, state transitions, races, and recovery. Enable real
  features with their supported environment variables, such as
  `KIRO_ENABLED_FEATURES`, instead of mocking feature availability.
- **Live LLM:** Use the real model when planning, tool selection, model
  interpretation, or generated content is part of the behavior under test.
- **Hybrid:** Use deterministic setup for hard-to-time states and a live model
  only for the behavior that requires model reasoning.

Mock only external nondeterminism at an existing supported boundary. Never call
a run "end to end" without naming that boundary. A deterministic UX run is
valid evidence when model reasoning is not under test.

For Activity Tray, queue, task, workflow-switching, Ctrl+O, and input-recovery
stories, choose deterministic UX unless a story explicitly asserts model
planning or generated content.

## 4. Execute and Capture

For every story:

1. Assert the Given state from the screen or test state.
2. Perform one user action at a time.
3. Assert the resulting state before continuing.
4. Capture transitions, not only the final screen.
5. Prove recovery by continuing to type, navigate, submit, cancel, or repeat.

Name frames `<story-id>-<step>-<expected-state>`. Label engine and surface when
they can differ. Keep a frame only when it proves a stated check; add a text or
state assertion for behavior a screenshot cannot prove.

Re-run the exact original repro after the fix. Add focused automated tests for
the ownership or state contract, then run the relevant integration suite.
Treat Knight Rider evidence as review proof, not a replacement for tests.

## 5. Build the Report

Before writing HTML, load the active `artifactory-design` skill and `slop.md`.
Write its two-sentence scene/reference brief, then declare a named direction
using `strategy`, `palette`, `typeset`, `compose`, `cadence`, and `voice`. Choose
for this report's audience and evidence shape; never standardize every bug bash
on one terminal-green, blueprint, or card-grid theme. Record the direction and
brief in the report's design marker/colophon.

Follow [report-contract.md](references/report-contract.md). Generate one
self-contained HTML file with inline CSS, JavaScript, and frame content. Derive
story, check, and frame totals from run data instead of hand-writing them. After
the first draft, run `slopcheck`; if two or more anti-patterns remain, redesign
one layer before validation rather than patching individual elements.

Include the exact test boundary and clearly distinguish:

- behavior proved by PTY frames;
- behavior proved by programmatic assertions;
- focused and integration tests run;
- CI status observed separately.

Never write "CI is green" from local results. Query the live PR checks.

## 6. Validate Before Upload

Run the bundled validator from the repository root so it can resolve the
project's Playwright installation:

```bash
node .kiro/skills/publish-feature-bugbash-evidence/scripts/validate-report.mjs \
  /absolute/path/report.html \
  --expect-sha "$(git rev-parse HEAD)" \
  --expect-branch "$(git branch --show-current)" \
  --expect-pr "https://github.com/OWNER/REPO/pull/NUMBER" \
  --expect-stories 8 \
  --expect-checks 24 \
  --min-frames 16 \
  --strict
```

Add `--require-text VALUE` for issue URLs or required claims and
`--forbid-text VALUE` only when that value must not appear anywhere in the
report, such as a superseded report ID. The validator checks required metadata,
counts, failed checks, nonempty frames, self-containment, link targets, browser
errors, and horizontal overflow at 1440x900 and 390x844.

Do not upload while validation reports an error. Resolve warnings where
possible; Artifactory can inject `<base target="_blank">` and same-page anchor
handling, but author them deliberately for local correctness.

## 7. Publish Privately

If Artifactory tools are unavailable, install and register the supported MCP:

```bash
toolbox registry add s3://buildertoolbox-registry-artifactory-us-west-2/tools.json
toolbox install artifactory-mcp
artifactory-mcp setup
mwinit
```

`artifactory-mcp setup` registers the server with Kiro and can propagate it to
scoped kiro-cli agents. Use the MCP tool after setup; do not call a guessed HTTP
endpoint.

Call `upload_artifact` with separate fields:

```json
{
  "filePath": "/absolute/path/report.html",
  "title": "Feature name - bug bash evidence",
  "summary": "Stories and scope tested at the exact branch and SHA.",
  "tags": ["kiro-cli", "bug-bash", "feature-name"],
  "visibility": "PRIVATE",
  "waitForReady": true
}
```

Use `upload_artifact_version` when replacing an existing report and preserving
its URL is preferable. First call `get_artifact`, confirm the current version is
the report being replaced, and pass its returned `sha256` verbatim as
`expectedCurrentSha256`:

```json
{
  "artifactId": "existing-artifact-uuid",
  "filePath": "/absolute/path/report.html",
  "expectedCurrentSha256": "sha256-from-get-artifact",
  "waitForReady": true
}
```

If the version call reports a conflict, re-read the artifact and ask before
overwriting the newer content. Keep reports private unless the user explicitly
requests a broader audience. Capture the returned `artifactUrl`, version,
upload notes, and digest. Call `get_artifact` again, confirm the new version is
ready, and validate the downloaded published bytes with the same report
command.

When PR reviewers need access, keep `visibility` private and grant the intended
aliases or team group the `VIEWER` role with `update_artifact_sharing`. Do not
make evidence public only to avoid configuring its access list.

## 8. Update PR Evidence

Post or update one evidence section, rather than accumulating stale comments.
Include:

- tested full SHA and branch;
- story, check, and frame pass counts;
- engine, surface, viewport, and evidence mode;
- linked issue coverage;
- Artifactory `artifactUrl`;
- explicit test and CI status.

Before posting, re-read the PR head SHA. Replace stale evidence when the branch
changes. Use a stable marker such as `<!-- feature-bugbash-evidence -->` so an
existing comment can be found and edited.

## Completion Gate

Finish only when:

- every requirement maps to at least one story;
- every story has assertions and evidence;
- every frame is nonempty and attributable;
- no story or check is failed or silently skipped;
- desktop and mobile have no document-level horizontal overflow;
- the report carries a named design direction and brief composed through
  `artifactory-design`, with its final `slopcheck` complete;
- report metadata matches the current PR head;
- the private Artifactory URL resolves;
- the PR contains one current evidence reference;
- live CI status is reported without overstating pending or skipped jobs.
