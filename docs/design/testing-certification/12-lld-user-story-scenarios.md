---
program: testing-certification
doc_type: lld
id: 12
title: User-Story Scenarios, Taxonomy and Drift Guardrails — Low-Level Design
owner: Zixuan (Zoe) Lin
status: drafting
depends_on: ["10"]
unblocks: ["14"]
foundations_used: ["F1","F3"]
hld: 00-hld.md
plan_phase: 1
---

# User-Story Scenarios, Taxonomy and Drift Guardrails — Low-Level Design

Status: Drafting · Owner: Zixuan (Zoe) Lin · Date: 2026-08-14
Audience: TUI maintainers, testing-certification workstream owners, auto-review (workstream 14)
Companion documents: `00-hld.md`, `01-foundation-architecture.md`, `02-implementation-plan.md`
Related existing designs: `10-lld-scenario-framework.md`, `scenario-runner-v2-architecture.md`

---

## 1. Scope

This workstream delivers three things:

1. **Scenario corpus** — a comprehensive set of user-story scenarios that cover the full product surface of the Kiro CLI TUI, organized as multi-step journeys rather than isolated command checks.
2. **Taxonomy** — a classification system (categories, priorities, surface tags) that makes coverage gaps visible and queryable without manual inspection.
3. **Drift guardrails** — automated checks that detect when a new user-facing feature ships without a covering scenario, preventing the corpus from going stale.

This workstream does **not** define:
- The scenario runner or execution engine (workstream 10).
- The KAS mock strategy or fixture generation (workstream 11).
- How the auto-review bot uses coverage data to gate PRs (workstream 14 — but this workstream provides the coverage manifest that 14 reads).
- The design-system rubric for visual assertions (workstream 15 / F4).

The line between this workstream and 10: workstream 10 defines how scenarios execute; this workstream defines what scenarios exist and how you know when one is missing.

## 2. Foundation dependencies

| Foundation | What you consume | Assumption you are making |
|---|---|---|
| F1 scenario schema | `Scenario` type, manifest shape under `scenarios/`, predicate vocabulary, category enum | The category enum is open to extension via schema change; the existing 7 values are not frozen. The tagging taxonomy (this workstream's output) feeds back into F1 as an approved extension. |
| F2 KAS mock-LLM | Nothing directly | Corpus scenarios that exercise tool-use and multi-turn flows will need deterministic fixtures from F2 to gate PRs. This workstream authors the scenarios; F2/workstream 11 authors the fixtures. |
| F3 Knight Rider driver | Live execution harness for non-deterministic validation | Multi-step user-journey scenarios may require `waitForIdle` settling that exceeds current timeouts. Assume the driver supports configurable per-scenario timeouts (already present in schema). |
| F4 design-system rubric | Nothing directly | The `observe` field in scenarios is free-form guidance for F4's UX review. This workstream writes `observe`; F4 decides how to evaluate it. |

**Coupling risk:** The category enum extension requires a schema change coordinated with workstream 10 (Kenneth). This is the only hard dependency — everything else is additive.

## 3. Design

### 3.1 Taxonomy

The taxonomy has three orthogonal dimensions:

**Categories** (mutually exclusive — one per scenario):

| Category | What it covers |
|---|---|
| `boot` | TUI launch, exit, pipe/headless, agent config loading, auth (login, token refresh), CLI flags |
| `panels` | Read-only overlay panels: /help, /usage, /hooks, /stats, /changelog, /config, and other informational overlays |
| `agents` | Agent picker, swap, return, /plan, /guide, create/edit, upgrade, discovery, steering files, AGENTS.md |
| `model-config` | /model picker, /effort level, model defaults, sticky model |
| `conversation` | Multi-turn coherence, context retention, compaction state, cancel+recovery, tangents, goal loop, interrupt modes, rewind, clear |
| `rendering` | Markdown, code blocks, streaming display, Lite UI, response formatting |
| `keyboard` | Command history, multi-line input, autocomplete, fuzzy search, voice input, custom keybindings, cursor movement |
| `permissions` | Tool approval dialog, trust/untrust, granularity, governance (enterprise blocking), InfraSafety |
| `tool-use` | Read, write, shell, grep, glob, code intelligence, web fetch/search, knowledge base, AWS CLI, multi-tool chain |
| `subagents` | Subagent spawn, delegation, result routing, crew (multi-subagent), approval routing, crew monitor |
| `session-management` | New, save, load, resume, session dashboard, spawn, switch, bookmarks |
| `cloud` | Cloud session connect, repo attach, autonomous mode, disconnect, quit prompt, scrollback reconciliation |
| `workflows` | Workflow run/pause/resume/cancel, DAG monitor, retry, spec create/run/view |
| `context-management` | Context add/remove/clear, compaction, usage panel, file attachment, image paste, memories, steering auto-load |
| `settings` | Theme, verbosity, status line, keybinding config, interrupt behavior setting, notifications, tool filters |
| `accessibility` | ASCII mode, no-animations, no-icons, screen reader compat, combined modes |
| `mcp` | MCP server start/stop, OAuth, tool discovery, registry, failure handling, per-server enable/disable |

**Category design principles (decided 2026-08-14, Zoe):**

- Each category has one clear failure mode. You never debate "does this go here or there?"
- **Tiebreaker rule:** when a scenario crosses two categories, categorize by the *primary user intent* — what bug would you file? (e.g., Ctrl+C during tool approval → `permissions`, because the bug is "approval dialog didn't handle cancel correctly")
- `boot` = "can you get into/out of the app?" (includes auth — it's pre-conversation)
- `panels` = "read-only overlay opens, renders, ESC closes" (informational panels without a clear domain home)
- `agents` = "which agent is running and what does it know?" (includes steering/AGENTS.md — they define agent identity)
- `model-config` = "which model is answering?"
- `conversation` = "does the conversation logic work?" (multi-turn, cancel, rewind, tangents, goal — behavioral correctness)
- `rendering` = "does the output look right?" (markdown, code blocks, streaming display, Lite UI — visual correctness)
- `session-management` = "managing your sessions" (pure UI/persistence, no LLM involved)
- `cloud` = network/sandbox-specific lifecycle failures, not just "sessions but remote"
- `permissions` = "is this tool allowed to execute?" (includes governance and InfraSafety — superset of trust)
- `tool-use` = "tool ran, did it work correctly?" (execution, not permission)
- `context-management` = "what can the agent see?" (includes compaction — it manages context window)

**Scenario-to-surface matching rule:** A scenario covers a command surface if any of its steps contains `type:/<command-name>` or `prompt:` text that invokes the command. For panels, a scenario with `type:/<panel-command>` + `enter` + a `screen.contains` verify covers that panel. For tools, a scenario whose steps trigger the tool (via `prompt:` that invokes it) covers it.

**Migration mapping (7 current → 16 new):**

| Current category | New category | Which scenarios |
|---|---|---|
| `basic` | `boot` | `boot` |
| `slash-commands` (panel openers) | `panels` | `/help`, `/tools` (no subcommand), `/mcp`, `/usage`, `/hooks`, `/code`, `/knowledge`, `/stats`, `/config` |
| `slash-commands` (agent switches) | `agents` | `/agent`, `/plan`, `/guide` |
| `slash-commands` (model/effort) | `model-config` | `/model`, `/effort` |
| `slash-commands` (context ops) | `context-management` | `/context show/add/remove/clear` |
| `slash-commands` (trust ops) | `permissions` | `/tools trust/untrust/trust-all/reset` |
| `slash-commands` (session ops) | `session-management` | `/chat new`, `/chat save`, `/chat load`, `/quit` (→ `boot`) |
| `slash-commands` (state mutations) | `conversation` | `/clear`, `/compact`, `/goal`, `/rewind` |
| `conversation` | `conversation` | All except `save-load-continuity` (→ `session-management`) |
| `keyboard` | `keyboard` | All except `ctrlc-exit` (→ `boot`) |
| `permissions` | `permissions` | All |
| `tool-use` (approval tests) | `permissions` | `tool-use-shell`, `tool-use-write-approval` |
| `tool-use` (execution tests) | `tool-use` | `tool-use-read` |
| `tool-use` (cancel tests) | `conversation` | `tool-cancel`, `tool-cancel-recovery` |
| `subagents` | `subagents` | All |

**Priority** (exactly one per scenario):

| Level | Meaning | Gating behavior |
|---|---|---|
| `p0` | Blocks release. Failure = cannot ship. | Must pass in deterministic mode on every PR. |
| `p1` | Should pass. Failure = high-confidence regression. | Must pass in live mode on RC. Deterministic mode recommended. |
| `p2` | Informational. Failure = investigate but don't block. | Runs in nightly, non-gating. |

**Tags** (multiple per scenario, freeform but with canonical values):

| Tag | Meaning |
|---|---|
| `smoke` | Part of the fast smoke suite (existing) |
| `regression` | Guards a specific past bug |
| `journey` | Multi-step user workflow (> 3 steps with state across turns) |
| `feature:<name>` | Covers a feature-gated capability (e.g. `feature:workflows`, `feature:cloud`) |
| `engine:<name>` | Narrower than the `engine` field — marks scenarios that test engine-specific behavior |

**Surface manifest** (new file, described below):

A machine-readable registry of non-extractable user-facing surfaces for coverage tracking.

### 3.2 Coverage assessment and drift detection

**Design decision (2026-08-14, Zoe):** The full scenario suite runs with code coverage instrumentation on every PR. This replaces a separate lint script — coverage is the source of truth for both drift detection and coverage assessment.

#### Mechanism

The scenario suite runs with coverage enabled (`--coverage` flag on the runner). This produces a coverage report showing which source files/functions were exercised.

```
On every PR:
  1. Run full scenario suite with coverage instrumentation
  2. New source files/functions with 0% coverage from ANY test (unit or scenario) → fail CI
  3. Scenario-specific coverage tracked as separate metric (not blocking)
  4. New user-facing code covered by unit test but NOT by scenario → advisory comment
```

**Two layers of enforcement:**

**Layer 1 — Coverage gate (deterministic, blocks PR):**
- New code must be covered by *something* (unit test or scenario)
- Overall coverage cannot decrease (ratchet — can only go up)
- The coverage floor is stored in `scenarios.coverage.json`, auto-updated when coverage improves
- **Scenario coverage report posted on every PR:** shows scenario-specific coverage % (before → after), highlights files with decreased scenario coverage, and lists new files with 0% scenario coverage

**Layer 2 — Scenario coverage advisory (non-deterministic, non-blocking):**
- When a PR introduces user-facing changes without a corresponding scenario, workstream 14's auto-review bot posts: "this adds/changes user-facing behavior but no scenario covers it — consider adding one"
- LLM reads the diff and judges: is this a user-facing change? If yes, does an existing scenario exercise it?
- Non-blocking per program rule: deterministic gates block, non-deterministic tiers inform

**What this catches across both layers:**

| Change type | Layer 1 (coverage) | Layer 2 (advisory) |
|---|---|---|
| New command, no tests at all | Blocks ✓ | Comments ✓ |
| New command, unit-tested only | Passes | Comments ✓ |
| New UI component, no tests | Blocks ✓ | Comments ✓ |
| New UI component, unit-tested only | Passes | Comments ✓ |
| Behavioral change to existing code | Scenario fails ✓ | Comments ✓ |
| Internal refactor, behavior unchanged | Passes | Silent |

#### Gate-aware enforcement

When `KIRO_ENABLED_FEATURES` includes a feature gate in the test environment, code behind that gate is exercised by scenarios and contributes to coverage. If a gate is enabled but no scenario touches the gated code, coverage drops → CI fails.

### 3.4 Scenario authoring strategy

The existing 62 scenarios are shallow: most test a single command invocation and verify one screen assertion. The gap is multi-step user journeys. The corpus expansion prioritizes:

**Phase 1 — fill p0 gaps:**
- Session lifecycle: boot → prompt → response → save → quit → resume → verify state
- Tool approval flow: prompt → tool fires → approval dialog → approve → result renders
- Context window: many turns → approaching limit → compact → continue
- Model switching mid-conversation
- Agent swap and return

**Phase 2 — journey coverage:**
- Cloud session full lifecycle (connect → repo → prompt → disconnect)
- Workflow run/cancel/retry
- Multi-session spawn and switch
- Knowledge base add/search/remove
- MCP server start/stop/tool invoke
- Spec create/run/view

**Phase 3 — edge cases and regressions:**
- Interrupt modes (steer vs queue)
- Accessibility modes
- Error recovery (network failure, auth expiry, MCP crash)
- Concurrent operations

Each new scenario follows this contract:
- Minimum 3 steps (single-step tests stay in the existing command tier)
- Must have `priority` set
- Must have at least one `verify` predicate that is not `screen.contains`
- Must reference at least one surface in the manifest
- `observe` field filled for live-mode visual review

### 3.5 File layout

```
packages/tui/e2e_tests/smoke/
  scenarios/
    shared/scenarios.json           # portable: runs under whichever backend the lane picks
    krs-mock/conversation.json      # deterministic journeys, turns inline per scenario
    live/scenarios.json             # live-only
  scenarios.schema.json             # schema (category enum extended)
  scenarios.coverage.json           # coverage floor (ratchet threshold)
```

One schema, one corpus: every manifest above validates against the same
`scenarios.schema.json`, and the loader reads every `.json` in a backend
directory. Splitting by backend directory is how a scenario declares what it
needs (a `krs-mock` scenario carries the `turns` that answer its prompts);
splitting by category *within* a directory is only file hygiene, so a manifest
stays reviewable as the corpus grows. Neither is a second format.

## 4. Interfaces you expose

| Consumer | What they get | Contract |
|---|---|---|
| Workstream 10 (runner) | Extended `category` enum in schema + `--coverage` flag on runner | Additive only; no existing category removed. Coverage output format TBD with runner owner. |
| Workstream 14 (auto-review) | Scenario corpus + coverage report — the auto-reviewer uses coverage data to judge whether PRs introduce uncovered user-facing code | Corpus file path is stable; coverage report format is JSON |
| CI | Scenario suite exit code + coverage ratchet check | 0 = all scenarios pass and coverage ≥ floor; non-zero = regression |
| Scenario authors | Taxonomy documentation (this LLD + schema) | Categories, priority semantics, and tag vocabulary are defined here |

## 5. Test strategy for this workstream

The outputs of this workstream are a corpus, a coverage ratchet, and the taxonomy. Testing:

1. **Coverage instrumentation works:** running `run-smoke.ts --coverage` produces a coverage report with per-file percentages. Verify by checking that known-exercised files show >0% and known-unexercised files show 0%.
2. **Coverage ratchet enforces:** artificially lowering the floor in `scenarios.coverage.json` and running CI produces a pass; raising it above actual coverage produces a fail.
3. **Schema validity:** every manifest under `scenarios/` validates against `scenarios.schema.json`, and no scenario id is duplicated across manifests. Tested by `manifests.test.ts` and `schema-refs.test.ts`.
4. **Corpus execution:** every new scenario must pass in at least one backend before merging. The scenario runner (workstream 10) handles execution; we verify authoring correctness by running `bun packages/tui/e2e_tests/smoke/run-smoke.ts --backend krs-mock --scenario <id>`.

## 6. Machine-checkable acceptance criteria

1. `bun packages/tui/e2e_tests/smoke/run-smoke.ts --backend acp-mock --engine kas --priority p0 --coverage` exits 0 and every p0 scenario passes.
2. Coverage report shows >0% for all user-facing source files that have a corresponding scenario.
3. The coverage ratchet (`scenarios.coverage.json`) prevents merging PRs that reduce coverage below the floor.
4. Adding a new command handler with no scenario that exercises it → coverage for that file is 0% → CI fails.
5. The `category` enum in `scenarios.schema.json` includes all 17 categories listed in §3.1, and every manifest under `scenarios/` validates against it.
6. Every scenario under `scenarios/` has a non-empty `priority` field.

## 7. Rollout and gating

**Phase 0 prerequisites — done (2026-08-20):**
- Migrated the corpus from 7 categories to 17 (schema enum + manifests in one
  change), per the mapping table in §3.1: 45 scenarios moved.
- Backfilled `priority` on the 42 scenarios that lacked one.
- `priority` is now a required field in `scenarios.schema.json`, enforced
  corpus-wide by `scenarios.schema.test.ts` (category and priority must both be
  values the schema declares, and ids must be unique across manifests).
- Record ACP-wire fixtures for new journey scenarios using `record-acp-wire-fixtures.ts`
- Add coverage instrumentation to the scenario runner (`--coverage` flag)

| Phase | What ships | Gating behavior |
|---|---|---|
| Phase 1 (this PR stack) | Taxonomy extension (17 categories), coverage instrumentation, p0 scenarios with fixtures | Full scenario suite runs with coverage on every PR. Coverage ratchet prevents regression. |
| Phase 2 | Journey scenarios, expand coverage across all categories | Coverage floor increases as scenarios are added. |
| Phase 3 | Edge-case scenarios, regression tags | No change to gating; additional scenarios expand coverage further. |

**Escape hatch:** if coverage enforcement blocks a legitimate PR that can't add a scenario (e.g., emergency fix), the coverage floor can be temporarily lowered with a justification comment in `scenarios.coverage.json`. The floor must be restored within 7 days.

**Implementation status (this PR).** What is wired today is narrower than the target design in §3.2:

- **Scenario runner — wired, blocking.** The deterministic journeys ride `rc-certification.yml`'s `Smoke Lane (krs-mock)` (real KAS, fake KRS, scripted model) on every verification PR. That lane is blocking, and it selects by corpus location rather than tag: anything added under `scenarios/krs-mock/` joins it automatically. It runs `kas` only (the deterministic backends are KAS-shaped; there is no deterministic v2 path) and ubuntu-only for now. **Decided 2026-08-20 (Zoe, with kensave):** this workstream does not add a second krs-mock lane. A parallel advisory lane would run the same corpus twice under two owners, so the scenarios join the existing blocking lane instead.
- **krs-mock is fully offline.** The fake KRS answers `ListAvailableModels` as well as inference, so the lane needs no credential: no real key, no `KIRO_REVIEW_PAT`, and it is not restricted to internal PRs on that account. (Superseded 2026-08-20: an earlier revision of this design required a real key because KAS gated every prompt on a control-plane model list the mock did not serve.)
- **Coverage + drift — wired, advisory inside the blocking lane.** The lane sets `KIRO_COVERAGE=1`; `launch.rs` then spawns the TUI under `bun test --coverage` through the real Rust→bun launch, so coverage is collected without leaving the prod path. `scenario-coverage.ts` merges the per-scenario lcov and compares it against `scenarios.coverage.json` when that floor exists. No floor is committed yet, so the step bootstraps (reports the number, exits 0); it is `continue-on-error`, so coverage reporting never gates merge even though the lane does.
- **Next to close the gap:** commit the first `scenarios.coverage.json` floor from a green run, then promote the drift check from advisory to gating once the corpus is broad enough that the ratchet is meaningful.

## 8. Risks

1. **Schema migration churn.** The category enum change (7→17) requires re-categorizing all existing scenarios in one atomic PR. **Mitigation:** Phase 0 prerequisite delivers the migration as a single PR with a mechanical mapping. The mapping is defined in this document (§3.1).

2. **Fixture dependency for p0 scenarios.** New journey scenarios need recorded ACP-wire fixtures to run deterministically. **Mitigation:** the existing `record-acp-wire-fixtures.ts` script records fixtures by running scenarios against a live LLM once. For state-management scenarios (compaction, save/load, clear), fixtures are trivial canned acknowledgments.

3. **Coverage instrumentation overhead.** Running with coverage adds time to CI. **Mitigation:** coverage overhead for TypeScript (v8/c8) is typically <20% wall-clock. If unacceptable, coverage can run on a parallel CI lane that doesn't block merge but reports to the ratchet.

4. **Category boundaries for journey scenarios.** Multi-step scenarios can cross categories. **Mitigation:** the tiebreaker rule ("categorize by primary user intent — what bug would you file?") is documented in the design principles. When ambiguity persists, the scenario author's judgment is final — the taxonomy serves discoverability, not rigid enforcement.

## 9. Open questions

- ~~2026-08-14: Should `surface-manifest.json` be auto-generated from source code or hand-maintained?~~ **Decided 2026-08-14 (Zoe):** Neither. Code coverage from running the full scenario suite is the source of truth. No separate lint or manifest needed — coverage instrumentation catches uncovered new code directly.
- ~~2026-08-14: Should the `observe` field follow a structured checklist format or stay free-form prose?~~ **Decided 2026-08-14 (Zoe):** Keep free-form prose. The LLM judge (F4) can interpret it. Revisit if F4 owner needs structure.
- ~~2026-08-14: Should feature-gated surfaces block CI when the gate is enabled in the test environment?~~ **Decided 2026-08-14 (Zoe):** Yes. When a gate is enabled in CI, code behind it contributes to coverage. If no scenario exercises it, coverage drops below the ratchet → CI fails.
- ~~2026-08-14: Who judges whether a change is a surface change?~~ **Decided 2026-08-14 (Zoe):** Coverage instrumentation judges deterministically (did the code execute?). LLM judge in workstream 14's auto-review bot provides advisory commentary on behavioral coverage. No structural lint needed.

## 10. AI-native notes

Entry points:

- `packages/tui/e2e_tests/smoke/scenarios/` — the scenario corpus, one directory per backend
- `packages/tui/e2e_tests/smoke/scenarios.schema.json` — schema (category enum lives here)
- `packages/tui/e2e_tests/smoke/scenarios.coverage.json` — coverage ratchet floor
- `packages/tui/e2e_tests/smoke/run-smoke.ts` — scenario runner CLI (coverage is driven by KIRO_COVERAGE)

Invariants:

- One corpus, one schema. Manifests may be split by backend directory and by category for file hygiene, but never forked into a second scenario format.
- Coverage can only go up. The ratchet floor in `scenarios.coverage.json` is the minimum.
- Categories are mutually exclusive. A scenario belongs to exactly one.
- Priority is mandatory on every scenario. Schema enforces this.
- Every `scenario`-tagged test must declare its `backend` explicitly. A scenario with no `backend` matches every lane (including `live`), so an omitted `backend` makes the test run where it has no fixture and fail confusingly.
- The full scenario suite runs with coverage on every PR.

Useful commands:

- `bun packages/tui/e2e_tests/smoke/run-smoke.ts --backend acp-mock --engine kas --coverage`
- `bun packages/tui/e2e_tests/smoke/run-smoke.ts --backend acp-mock --engine kas --category conversation`
- `bun packages/tui/e2e_tests/verification/list-scenario-categories.ts` — the categories the lanes shard on
