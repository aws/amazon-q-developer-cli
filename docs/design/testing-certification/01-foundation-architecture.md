---
program: testing-certification
doc_type: foundation
id: "01"
title: Foundation Architecture
owner: Kenneth Sanchez
status: drafting
hld: 00-hld.md
provides: ["F1", "F2", "F3", "F4"]
---

# Foundation Architecture

Status: Drafting · Owner: Kenneth Sanchez · Date: 2026-08-06
Audience: all workstream owners — every LLD in this program consumes something defined here
Companion documents: `00-hld.md`, `02-implementation-plan.md`

---

## Why this document exists

Five workstreams depend on four shared pieces of substrate. If each workstream designs
its own version of that substrate, the program's central bet — *a scenario authored once
runs in every tier* — fails quietly: the tiers drift apart and we end up maintaining
three scenario formats and two mocking strategies.

This document specifies the substrate once. Once a foundation here is marked `approved`,
it is frozen for the phase, and workstream LLDs may assume it. Owners declare what they
assume in their §2 table, which is how coupling risk surfaces before it becomes rework.

## The four foundations

| Id | Foundation | Blocks | Owner | Status |
|---|---|---|---|---|
| F1 | Scenario schema + `Scenario` type | 10, 12, 13, 14, 16 | Kenneth Sanchez | Not started |
| F2 | KAS mock-LLM (fake model endpoint) | 13, 16 | Felix Ding | Not started |
| F3 | Knight Rider act-react / goal-seeking driver | 12, 15 | Kenneth Sanchez | Not started |
| F4 | Design-system rubric | 15 | TBD | Not started |

---

## F1 — Scenario schema and `Scenario` type

**Decisions needed from the lead:**

1. Schema location and whether the JSON schema or the TypeScript type is normative.
   (Recommendation: the JSON schema is normative; the type is generated from it, so they
   cannot drift.)
2. The `verify[]` predicate vocabulary — the HLD proposes `screen.*`, `store.*`,
   `agent.*`, `file.*`, `exit.*`. Is that closed or extensible, and if extensible, who
   approves a new predicate?
3. Whether the builder is a separate package or lives beside the runner.
4. The tagging taxonomy — **this is jointly owned with workstream 12 (Zoe)** and must be
   published before corpus authoring starts.

> FILL: The design. Include the schema shape, the predicate-to-capture-API mapping
> (`screen.contains` → `getSnapshot()`, `store.<path>` → `getStore()`,
> `agent.tool` → `getAgentState()`, `file.*` → sandbox fs, `exit.code` → `expectExit()`),
> and the versioning rule for the schema itself.

**Freeze condition:** the existing 56-scenario `scenarios.json` corpus parses against the
schema with zero hand edits. If the corpus needs editing to fit, the schema is wrong.

**Acceptance criteria**
1. `scenarios.schema.json` exists and validates the full existing corpus.
2. The generated `Scenario` type compiles and is imported by the runner.
3. One p0 scenario executes end-to-end with `verify[]` predicates actually asserted
   (today `verify[]` is declared but executed nowhere — that is the bug this closes).

---

## F2 — KAS mock-LLM

The single biggest hole in the current suite: KAS is the production engine and has no
deterministic full-stack coverage that gates PRs.

**The load-bearing decision.** Mock at the **model endpoint**, not the ACP wire. Point
KAS at a fake model server that returns the scenario's canned responses, so the real KAS
agent, its tools, and its session all run. Mocking at the ACP wire is what the existing
ACP integration tier already does — it fakes the agent, which leaves precisely the gap we
are trying to close.

**Open question that must close in Phase 0:** is the fake-model-endpoint approach
feasible given we do not own KAS? If not, the fallback is an ACP-wire mock, which is
simpler but does not exercise the KAS agent, tools, or session — materially weaker
coverage that must be stated as such, not quietly accepted.

> FILL: The design. Fake model server shape, how a scenario's canned responses are
> loaded, how KAS is pointed at it, and how the harness spawns KAS as the ACP back-end.

**Acceptance criteria**
1. A p0 scenario runs deterministically against the real KAS agent, twice, with identical
   results.
2. The run fails when it should — deliberately break an assertion and confirm red.
3. No real model call leaves the process. Verifiable by network assertion.

---

## F3 — Knight Rider act-react driver

Extends the existing HTTP driver so a driver-LLM works toward a scenario **goal** and
reacts to what the agent actually did, rather than replaying a fixed script against an
agent that took a different path.

**Decisions needed:**
1. Does the driver use the same model as the agent-under-test, or a pinned model for
   consistency? (Open question in the HLD.)
2. Is the tolerance/retry budget per-turn, per-scenario, or both? (Open question in the
   HLD. Recommendation: both, with per-turn as the primary and per-scenario as a cap.)

> FILL: The design. The act-react loop, the guidance-injection point, budget accounting,
> and how a scenario declares its guidance prompts.

**Acceptance criteria**
1. A live scenario reaches its goal after at least one injected steering prompt.
2. A scenario that cannot reach its goal fails once the budget is spent, and reports
   which turn exhausted it.
3. Frame capture is reusable by workstream 15 without modification.

---

## F4 — Design-system rubric

An authoring task, not a coding task — it can run fully parallel to F1–F3. The rubric is
the UX review agent's oracle; without it there is nothing to review against.

Content lives in [`23-design-system.md`](23-design-system.md). Source material: the
existing theme files, the glyph set, and the three accessibility display settings.

**Acceptance criteria**
1. An agent can apply the rubric to a captured frame and produce a finding that a second
   agent reproduces from the same frame.
2. Every rule is stated as an observable property of a rendered frame. A rule that cannot
   be checked against pixels or cells does not belong in the rubric.

---

## Cross-cutting: the AI-native contract

Machine-readability is a foundation, not a nicety — the auto-review workstream has to
answer "does this PR add a surface with no covering scenario?", which is only answerable
if ownership, coverage, and status are queryable rather than prose.

- `program.json` — ownership, dependency, and status graph.
- Frontmatter on every doc in this directory — `program`, `doc_type`, `id`, `owner`,
  `status`, `depends_on`, `unblocks`.
- `scripts/testing-cert-status` — validates the above and prints the blocking graph.
- Instruction layer — see [`19-lld-agents-steering-renewal.md`](19-lld-agents-steering-renewal.md).

## Freeze log

Record each foundation freeze with a date and who approved it. A foundation that is
frozen without an entry here is not frozen.

| Foundation | Frozen on | Approved by | Notes |
|---|---|---|---|
| F1 | — | — | |
| F2 | — | — | |
| F3 | — | — | |
| F4 | — | — | |
