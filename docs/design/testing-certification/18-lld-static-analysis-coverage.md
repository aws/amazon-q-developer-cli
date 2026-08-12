---
program: testing-certification
doc_type: lld
id: 18
title: Static Analysis and Coverage Enforcement — Low-Level Design
owner: Adam Cervantes
status: in-review
depends_on: []
unblocks: []
foundations_used: []
hld: 00-hld.md
plan_phase: 2
---

# Static Analysis and Coverage Enforcement — Low-Level Design

Status: In Review · Owner: Adam Cervantes · Date: 2026-08-12
Audience: Kenneth Sanchez (program lead) and workstream owners
Companion documents: `00-hld.md`, `01-foundation-architecture.md`, `02-implementation-plan.md`
Related existing designs: `packages/tui/docs/COVERAGE.md`

---

## 1. Scope

Provide static analysis for the TypeScript packages of kiro-cli and ensure the code
is properly covered through per-PR coverage floors. TypeScript is the main language
of V3 and carries ~68% of the repo's code churn over the last six months (measured
2026-08). The gap is the rule set, not the plumbing: the required `tui-lint` check
already runs ESLint for `packages/tui`, but no complexity, duplication, or
code-smell rule is enforced anywhere, and `packages/twinki` and
`packages/terminal-harness` carry no ESLint configuration at all.

**Static analysis** delivers the rules that judge code structure: cyclomatic
complexity limits, code duplication detection, and code-smell rules; these land as a
new blocking `code-quality.yml` workflow with an in-code debt baseline that may only
decrease (scoped disables justified by structured `LINT-DEBT` comments).

**Coverage enforcement** delivers the mechanism that ensures the code is covered
properly by tests: per-PR code-coverage reporting and coverage floors that may only
rise, so a PR that lowers coverage on the TypeScript packages fails the gate.

**Not in scope**

- **Rust code**: the crates are already gated by clippy (`-D warnings`, curated
  lints), fmt, and deny. This includes the Rust portions of the V3 launch path
  (`crates/chat-cli`), which keep their existing clippy coverage.
- **Scenario- and feature-coverage detection** ("does this PR add a surface with no
  covering scenario?"): this belongs to LLD 14 (Auto-Review) with the corpus
  guardrails of LLD 12. In this document *coverage* always means code coverage,
  never scenario coverage.

## 2. Foundation dependencies

None of F1–F4 is consumed: both deliverables operate on the source tree and CI
configuration, not on scenarios, mocks, or drivers.

| Foundation | What you consume | Assumption you are making |
|---|---|---|
| F1 scenario schema | Nothing | Coverage floors are computed from the existing unit and integration suites; coverage produced by scenario runs does not count toward the floors. |
| F2 KAS mock-LLM | Nothing | The measured suites run without a live or mocked LLM; no part of this gate executes the agent. |
| F3 Knight Rider driver | Nothing | Lint and coverage are batch operations over the source tree and test processes; nothing in the gate drives a running TUI. |
| F4 design-system rubric | Nothing | Code-smell rules judge code structure only; no rule encodes visual or UX conventions. |

## 3. Design

**Components.** The gate has four parts, all wired into one new blocking workflow
(`code-quality.yml`):

1. **Lint rules (ESLint).** Extend `packages/tui/eslint.config.js` with the
   complexity and code-smell rule set, and add configs to `packages/twinki` and
   `packages/terminal-harness`, which have none today. The repo-root
   `eslint.config.js` (shared ignores and a baseline block) is deliberately
   unchanged: the pinned ESLint (v10) looks up config from the file being
   linted, so a package's own config shadows the root one for its tree and the
   per-package configs govern. Cyclomatic complexity (the ESLint `complexity`
   rule) is the primary threshold, matching the metric named in the
   implementation plan; a secondary bundle of structural rules (`max-depth`,
   `max-params`, `sonarjs/cognitive-complexity`) backstops the patterns cyclomatic
   complexity misses.
2. **Duplication detection (jscpd).** `.jscpd.json` scopes detection to production
   code (test globs excluded); `.jscpd-baseline.json` records the current absolute
   clone count. The check fails when the count exceeds the baseline, and the
   baseline may only decrease. An absolute count is used rather than a percentage
   because a percentage silently admits more duplication as the codebase grows.
3. **Debt limit (LINT-DEBT).** Existing violations are not fixed up front; they are
   annotated in place with scoped disables carrying a structured justification
   comment (`// LINT-DEBT(rule): reason`).
   `scripts/code-quality/annotate-lint-debt-ts.py` performs the one-time adoption
   annotation; `scripts/code-quality/check-lint-debt.py` enforces that every disable
   is justified and that the debt count never exceeds the committed baseline;
   loosening the baseline requires a visible, reviewed diff (§7).
4. **Coverage floors.** Per-package coverage thresholds enforced from the existing
   test runners; a per-PR coverage report is posted to the PR. Floors rise as
   coverage improves, and a PR that drops a package below its floor fails. The
   floors absorb the existing check rather than coexisting with it: `tui.yml`
   already fails below 80% functions and lines using bun's "All files" summary,
   a number its own comment notes is depressed because exclusions apply to the
   coverage table but not the summary. The floors here are computed from the
   per-package table, with `coverage-config.json` (documented in
   `packages/tui/docs/COVERAGE.md`) remaining the single source of truth for
   exclusions; the landing series deletes the `tui.yml` coverage block in the
   same slice that brings the tui floor live.

**Data flow.**

```
PR opened/updated
  -> code-quality.yml (blocking)
       |- eslint: complexity + code-smell rules (LINT-DEBT disables honored)
       |- check-lint-debt.py: justifications well-formed; debt count <= baseline
       |- jscpd + check-duplication.py: clone count <= baseline (prod code only)
       '- coverage: per-package run with floors; report posted to the PR
```

**Files added or changed.** `.github/workflows/code-quality.yml` (new, blocking);
`packages/tui/eslint.config.js` (rules added); `packages/twinki/` and
`packages/terminal-harness/` ESLint configs (new); `.jscpd.json` and
`.jscpd-baseline.json` (new);
`scripts/code-quality/{check-lint-debt,check-duplication,annotate-lint-debt-ts}.py`
(new).

**Reference implementation.** A working spike of all four components validated the
approach end to end; the TypeScript slices land here, split into reviewable PRs
(§7). The spike's Rust pieces are out of scope per §1.

**Decisions.**

1. The debt limit lives in code as scoped disables, not in config excludes: debt is
   visible at the offending line and greppable.
2. Cyclomatic complexity is the primary threshold (the plan's named metric, enforced
   by the ESLint `complexity` rule); a structural bundle including cognitive
   complexity is secondary. Decided 2026-08-10 by the owner.
3. The duplication gate is an absolute count on production code.
4. A separate `code-quality.yml` rather than extending `tui.yml`: independent
   ownership and a clean required-check boundary.
5. Justifications are structured comments so an agent can parse, count, and burn
   them down.
6. The gate blocks from day one, landed as one stacked series ending in the required
   check.
7. Coverage floors absorb `tui.yml`'s existing threshold check rather than
   coexisting with it: one floor mechanism, computed from the per-package table,
   with `coverage-config.json` as the exclusion authority. Decided 2026-08-12 by
   the owner.
8. A package's rules and its debt annotations are atomic (they land in the same
   commit), because the already-required `tui-lint` check enforces new
   `packages/tui` rules immediately, ahead of `code-quality` becoming required.
   Decided 2026-08-12 by the owner.

## 4. Interfaces you expose

Each item below is a contract; changing it requires a dated note in §9. Consumers
read these contracts opportunistically; nothing blocks on them, so the frontmatter
deliberately carries no dependency edges.

- **The required check name (`code-quality`)**: branch protection and anything that
  queries check runs key on this exact name.
- **The `LINT-DEBT` comment grammar** (`// LINT-DEBT(rule): reason`):
  machine-parseable by design; burn-down tooling and agent workflows (LLD 19
  steering, LLD 14 review) may read and count these.
- **`.jscpd-baseline.json`**: the machine-readable duplication baseline; consumed by
  `check-duplication.py` and available to any dashboard or agent.
- **The per-PR coverage report**: posted on the PR; LLD 14's scenario-gap detection
  may read it. Exact format is fixed at implementation and recorded here when it
  lands.
- **`scripts/code-quality/*` exit semantics**: exit 0 on pass, non-zero on
  violation; CI and local hooks rely on this.

## 5. Test strategy for this workstream

Every check ships with a negative test proving it fails when it should; a gate is
only trusted after it has been seen red.

- **Lint**: a fixture file exceeding the cyclomatic complexity threshold must fail
  the ESLint step; a compliant file must pass it.
- **Debt limit**: unit tests on `check-lint-debt.py` cover the three failure modes
  separately: an unjustified disable, a malformed `LINT-DEBT` comment, and a debt
  count above the baseline. Each must exit non-zero with a message naming the
  offending file and line.
- **Duplication**: a fixture pair of duplicated blocks pushing the count above
  baseline must fail `check-duplication.py`; the same fixtures inside a test glob
  must not count.
- **Coverage**: a fixture package configured below its floor must fail the coverage
  step; the per-PR report must still be posted on failure (the report is
  diagnostic, not conditional on green).
- **Script tests** live in `scripts/code-quality/tests/` and run in the same
  workflow ahead of the checks they validate; a broken checker fails the gate
  rather than silently passing it.
- **End-to-end proof at landing**: the stacked landing series includes one commit
  that intentionally violates each check to demonstrate red in CI, reverted within
  the stack before the final merge.

## 6. Machine-checkable acceptance criteria

1. `bunx eslint .` run inside each of `packages/tui`, `packages/twinki`, and
   `packages/terminal-harness` exits 0 on the landed main; adding a fixture
   function above the cyclomatic complexity threshold makes it exit non-zero.
   (The per-package invocation mirrors how `tui-lint` runs; under the pinned
   ESLint v10, config lookup starts from the file being linted, so either
   invocation exercises the per-package configs the gates read.)
2. `scripts/code-quality/check-lint-debt.py` exits 0 on main; appending an
   unjustified `eslint-disable` to any production file makes it exit non-zero and
   name the file and line.
3. `scripts/code-quality/check-duplication.py` exits 0 on main; duplicating a
   30-line production block makes it exit non-zero. The same block duplicated under
   a test glob leaves it at 0.
4. The per-package coverage run exits 0 on main; raising any package's floor above
   its current coverage makes it exit non-zero, proving floors are enforced rather
   than decorative.
5. `python3 -m unittest discover -s scripts/code-quality/tests` exits 0; the suite
   runs on the standard library alone, with no dependency to install.
6. `grep -qE '^  code-quality:' .github/workflows/code-quality.yml` exits 0: the
   workflow ships with the change and defines the job key branch protection pins,
   so renaming the job breaks this criterion (the §10 invariant). Branch
   protection requiring it, and the posted coverage report, are verified at
   rollout (§7).

## 7. Rollout and gating

**Attachment.** The gate runs as a required check named `code-quality` on every PR
targeting main. It has no nightly, RC, or release role; everything it asserts is
per-PR.

**Blocking.** Blocking from day one (§3 decision 6). Landing is a stacked PR series:
adoption annotations and configs land in reviewable slices, and the final PR in the
stack flips `code-quality` to required. The already-required `tui-lint` check is a
second enforcement path: it runs ESLint against the same `packages/tui` config, so
a rule added there blocks TUI PRs immediately, before the flip. Slices are
therefore atomic per package (§3 decision 8): a package's rules and its debt
annotations land in the same commit, so no PR ever meets a rule without its
annotations. Rollout is verified live:
`gh api repos/kiro-team/kiro-cli/branches/main/protection/required_status_checks`
lists `code-quality`, and the landing series shows the coverage report posted on a
draft PR.

**Escape hatch.** Both baselines are checked-in files, so the gate never needs a
global off switch; every loosening is itself a reviewed diff:

- A false-positive lint or duplication finding at 2am is unblocked by a justified
  `LINT-DEBT` disable or an explicit baseline edit in the same PR; both are visible
  in the diff and auditable after the fact.
- A true emergency (the gate itself is broken) uses GitHub's admin bypass of
  required checks, with a follow-up issue filed to restore enforcement before the
  next release.

## 8. Risks

1. **Adoption blast radius.** Annotating existing violations touches many files at
   once (the spike touched hundreds), risking review fatigue and merge conflicts
   with in-flight work. Mitigation: annotations are script-generated and land as
   per-package slices in the stacked series; each slice is mechanically reviewable
   and cut immediately before landing to minimize conflict windows.
2. **Threshold tuning.** A cyclomatic threshold set too low floods the codebase
   with debt annotations; set too high, the gate is decorative. Mitigation: tune
   against the current complexity distribution before landing so the initial
   threshold annotates only the worst tail, and record the chosen value and its
   distribution snapshot in the landing PR.
3. **Baseline erosion.** Repeated "just this once" baseline edits hollow the gate
   out. Mitigation: every loosening is a reviewed diff (§7), and the baseline
   files' git history makes the trend inspectable; a sustained upward trend is a
   review-culture signal, not a tooling failure.

Lateness does not cascade: no workstream in `program.json` depends on this one, and
partial landing (lint and duplication before coverage floors) is the fallback.

## 9. Open questions

1. **Scenario-suite coverage and the floors** (program lead). Once the deterministic
   scenario gate exists, should coverage produced by scenario runs count toward the
   per-package floors? This design assumes not (§2, F1 row); a yes changes the
   coverage measurement inputs and should be decided before floors are tuned.
2. **Which suites feed the floors** (owner, at implementation). packages/tui has
   unit, integration, and E2E suites; the floor computation must name which runs
   count. Default position: unit and integration only, since E2E runtime would make
   the gate slow and flaky-coupled.
3. **HLD goal 8 ownership** (program lead). Goal 8 ("Static analysis for feature
   coverage") is excluded here (§1) and delivered by LLD 14's scenario-gap
   detection; confirm that assignment, or record it in the HLD, so the goal is not
   read as unowned.

## 10. AI-native notes

Entry points: `.github/workflows/code-quality.yml` (the gate),
`packages/*/eslint.config.js` (rules), `.jscpd.json` and `.jscpd-baseline.json`
(duplication config and baseline), `scripts/code-quality/` (checkers and the
adoption annotator).

Invariants that are easy to break:

- The required check name `code-quality` is load-bearing: branch protection keys on
  it, and renaming the workflow job strands branch protection waiting on a check
  that never reports, blocking every PR until protection settings are updated.
- `LINT-DEBT` comments are parsed, not decorative: freeform edits to the
  justification format break `check-lint-debt.py` counting.
- Baselines are contracts, not caches: never regenerate `.jscpd-baseline.json`
  wholesale to make CI pass; a baseline edit is a reviewed decision (§7).
- "Production code" is defined by the globs in `.jscpd.json`; adding a source
  directory without updating the globs miscounts silently.

Verify a change with the §6 commands; the fastest local loop is
`check-lint-debt.py` plus `check-duplication.py` before pushing. Durable rules from
this section should graduate into AGENTS.md (see 19-lld-agents-steering-renewal.md).
