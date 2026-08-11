---
program: testing-certification
doc_type: lld
id: 19
title: AGENTS.md and Steering Scaffolding Renewal — Low-Level Design
owner: TBD
status: drafting
depends_on: []
unblocks: ["10","12","14"]
foundations_used: []
hld: 00-hld.md
plan_phase: 0
reference_implementation: https://github.com/kirodotdev/KiroCrew
---

# AGENTS.md and Steering Scaffolding Renewal — Low-Level Design

Status: Drafting · Owner: TBD · Date: 2026-08-06
Audience: program lead, all workstream owners, and every agent that edits this repo
Companion documents: `00-hld.md`, `01-foundation-architecture.md`, `02-implementation-plan.md`
Reference implementation: [KiroCrew](https://github.com/kirodotdev/KiroCrew) — used as the parameter for what good looks like

---

## 1. Scope

Renew the repository's agent-facing instruction layer so that an agent — ours or a
contributor's — picks up the right rules automatically instead of being told them in
a prompt. Covers the root `AGENTS.md`, the nested per-package `AGENTS.md` files, and a
new steering layer. Explicitly **not** in scope: changing what the CLI product does
with steering files, or authoring per-workstream agent rules (each LLD owns its own
§10 AI-native notes, which graduate up into this layer).

This workstream is listed as a Phase 0 foundation because the scenario, user-story,
and auto-review workstreams all generate agent-authored artifacts. If the instruction
layer is stale when those start, every generated scenario inherits the staleness.

## 2. Current state (measured 2026-08-06)

| Location | Lines | Character |
|---|---|---|
| `AGENTS.md` (root) | 422 | Codebase orientation, crate/package map, dev commands, TUI guidelines, change + comment discipline |
| `packages/tui/AGENTS.md` | — | Package-local rules |
| `packages/twinki/AGENTS.md` | — | Package-local rules |
| `.kiro/steering/` | **absent** | No always-on steering layer exists in the repo |

Two observations drive the design:

1. **The nesting convention already exists and works.** Three `AGENTS.md` files at
   descending scopes is the right shape; the problem is not structure, it is that the
   root file mixes durable orientation with volatile commands, so it drifts.
2. **There is no steering layer at all.** Everything an agent must *always* obey is
   buried inside a 422-line orientation document that an agent may only partially read.
   Rules that must never be missed do not belong in a file whose main job is a crate map.

## 3. What we take from KiroCrew (the parameter)

KiroCrew separates the instruction layer into distinct, purpose-scoped artifacts rather
than one large file. The patterns worth adopting:

| KiroCrew pattern | Adoption here |
|---|---|
| `.kiro/steering/*.md` as always-on rules, separate from orientation docs | Introduce `.kiro/steering/` for rules that must apply to every turn |
| `skills/` — task-scoped procedures loaded on demand | Introduce repo skills for recurring dev procedures (run the KAS path, capture smoke evidence, add a scenario) so they are not re-explained per session |
| Rules phrased as behaviour with an explicit negative ("do X, never Y") | Rewrite discipline sections in rule form so they are checkable, not advisory |
| Corrections crystallize into durable lessons | Define the promotion path: a repeated review correction becomes a steering rule, not a tribal habit |
| `doctor` — machine-verifiable environment readiness | Add a check that validates the instruction layer itself (see §6) |

The distinction we are buying: **orientation** (what the codebase is — read once, drifts
slowly) vs **steering** (what you must always do — read every turn, must be short) vs
**skills** (how to perform a specific procedure — loaded on demand).

## 4. Design

```
AGENTS.md (root)              orientation only: architecture map, crate/package
                              layout, where things live. No commands, no rules.
  packages/tui/AGENTS.md      package-local orientation + local invariants
  packages/twinki/AGENTS.md   package-local orientation + local invariants
  docs/design/testing-certification/AGENTS.md
                              program-local rules for anyone editing these docs

.kiro/steering/               NEW — always-on, short, imperative
  00-change-discipline.md     what a change must include before it is "done"
  01-testing.md               which suite to run for which change; never claim
                              green without running it
  02-comment-discipline.md    (moved out of root AGENTS.md)
  03-docs.md                  where designs live, frontmatter contract

.kiro/skills/                 NEW — on-demand procedures
  run-with-kas/               the CodeArtifact + build + launch sequence
  add-scenario/               how to author and validate a scenario (after F1)
  capture-evidence/           smoke/UX frame capture for a PR
```

Volatile content (dev commands, build sequences, auth token steps) moves **out** of
`AGENTS.md` and into skills, because commands are the fastest-drifting content in the
file and the least useful to an agent that is not currently running them.

## 5. Interfaces exposed

- **Frontmatter contract for design docs** (`program`, `doc_type`, `id`, `owner`,
  `status`, `depends_on`) — consumed by `scripts/testing-cert-status` and by agents
  answering "what is blocked on what". Defined in `01-foundation-architecture.md`.
- **Steering rule format** — each rule states the behaviour and its negative, so it
  can be checked by a reviewer or an agent.
- **Promotion path** — any LLD's §10 AI-native notes are the intake queue for this
  layer; recurring items graduate to steering or a skill.

## 6. Machine-checkable acceptance criteria

1. `.kiro/steering/` exists and every file in it is under 100 lines (long rules are
   not read; enforce the ceiling in CI).
2. Root `AGENTS.md` contains no shell command blocks — commands live in skills.
   Checkable: `grep -c '```bash' AGENTS.md` returns 0.
3. Every doc under `docs/design/testing-certification/` parses against the frontmatter
   contract; `scripts/testing-cert-status` exits 0.
4. A cold agent given only the repo can run the KAS test path by loading the
   `run-with-kas` skill, with no additional prompting. Verified by running it.
5. No rule appears in two places. Duplicated rules drift apart; a duplication check
   over steering + AGENTS files exits 0.

## 7. Rollout and gating

Phase 0, non-blocking to CI initially. Land the structure first, then enable the
§6 checks as advisory for one week, then promote to PR-blocking. Rationale: a
docs-layer check that blocks merges on day one will be disabled by the first person
it inconveniences.

## 8. Risks

- **Churn without adoption** — restructuring files nobody reads changes nothing.
  Mitigation: criterion #4 is a live test, not an inspection; if a cold agent cannot
  run the KAS path from the skill, the work is not done.
- **Steering bloat** — steering grows until it is another 422-line file. Mitigation:
  the 100-line ceiling in criterion #1, enforced.
- **Divergence from the product's own steering semantics** — this repo *is* the CLI
  that reads `.kiro/steering`. Mitigation: use the documented precedence and format
  exactly; do not invent repo-only extensions.

## 9. Open questions

- Do we adopt `.kiro/skills/` in-repo, or keep procedures in `docs/` and let agents
  find them? (KiroCrew's answer is skills; ours may differ because this repo ships
  the runtime that consumes them.)
- Who owns the steering layer after the first draft — the program lead, or whoever
  owns developer experience?
- Should the frontmatter contract extend to all of `docs/design/`, or stay scoped to
  this program until proven?

## 10. AI-native notes

Entry points: `AGENTS.md` (root), `packages/tui/AGENTS.md`, `packages/twinki/AGENTS.md`.
The invariant that is easy to break: putting a rule in an orientation file. If it is
something you must *always* do, it belongs in steering; if it is a procedure you
sometimes run, it belongs in a skill; only "what this codebase is" belongs in
`AGENTS.md`.
