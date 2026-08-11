---
program: testing-certification
doc_type: plan
id: "02"
title: Implementation Plan
owner: Kenneth Sanchez
status: drafting
hld: 00-hld.md
---

# Kiro CLI Testing — Implementation Plan

Implementation plan for the *Kiro CLI Testing — High Level Design*. Turns the HLD
into a dependency-ordered, phased delivery plan whose end state is **removing the
manual bug bash from the release process**.

## Stated assumptions (ownership)

Ownership in the HLD is partially assigned. This plan assumes:

- **Zoe (Zixuan Lin)** — *User Stories Scenarios Testing*: authoring the user-story
  corpus for every surface/feature (sub-agents, chat, slash commands, permissions,
  etc.), clustering by category, and building the coverage-enforcement + drift-detection
  guardrails.
- **Felix Ding** — *KAS Mocked Testing*: KAS mock-mode server, ACP contract publication
  + compile-time enforcement, version-bump validation, dual-run (ACP-wire vs LLM) wiring.
- **Kenneth Sanchez** — *Deterministic Smoke Tests*: porting the LLM-driven smoke suite
  to a deterministic (mocked) permutation and the deployment gate/blocker.
- **Adam Cervantes** — *Static Analysis*: cyclomatic complexity, duplication, code-smell
  rules.
- Owners marked *TBD* in the HLD table remain open; this plan assigns them to phases,
  not people.

If any of these are wrong, correct them and the dependency graph below still holds —
only the owner column shifts.

---

## 1. Dependency mapping

Five workstreams, but they are **not** peers — three of them consume artifacts the
first two must produce. The critical path runs through the Scenario Framework and the
KAS mock.

```
                 ┌─────────────────────────────────────────────┐
                 │ FOUNDATION (must land first — see §2)         │
                 │  F1 scenarios.schema.json + Scenario type     │
                 │  F2 KAS mock-LLM (fake model endpoint)        │
                 │  F3 Knight Rider act-react extension          │
                 │  F4 Design-system rubric document             │
                 └───────┬───────────────┬───────────────┬───────┘
                         │               │               │
        ┌────────────────┘        ┌──────┘         ┌──────┘
        ▼                         ▼                ▼
  Scenario-Based Testing    Deterministic E2E   Auto-UX Review
  (F1,F3)                   (F1,F2)             (F4,F3)
        │                         │                │
        │  scenario corpus        │ PR gate        │ blocks on rubric
        ▼                         ▼                ▼
  Auto-Review Enhancements  ◄─────┴──── needs scenario schema to detect
  (needs F1 + scenario         "PR adds surface with no covering scenario"
   corpus to know what a
   "covering scenario" is)

  Fuzz Testing (F1,F2)  ── produces ──►  new deterministic scenarios (feeds back into F1 corpus)
```

**Blocking relationships (what blocks what):**

| Blocker | Blocks | Why |
|---|---|---|
| **F1** scenario schema + `Scenario` type | Scenario-Based, Deterministic E2E, Auto-Review gap-detection | Everything downstream is "a run of a scenario" or "a check for a missing scenario". Nothing can start authoring against an unversioned schema. |
| **F2** KAS mock-LLM (fake model server) | Deterministic KAS E2E, KAS PR gate, Fuzz-on-KAS | This is the single biggest hole in the current suite. Without it there is no deterministic KAS path to gate on. Open question in HLD (fake model endpoint feasibility) must be resolved here first. |
| **F3** Knight Rider act-react/goal extension | Live-mode scenarios, Auto-UX frame capture | Both live Q/A and UX review drive the surface through Knight Rider. |
| **F4** Design-system rubric doc | Auto-UX Review (all of it) | The rubric IS the review's oracle. It does not exist today; it's a prerequisite deliverable, not a consumer. |
| Scenario corpus (Zoe) | Auto-Review scenario-gap detection, deterministic gate coverage | "No covering scenario" is only meaningful against a real corpus + tagging taxonomy. |
| Deterministic gate green + proven | Bug-bash removal | The readiness milestone gates on the other tiers being trustworthy. |

**Zoe-owned vs other-owned split:**

- **Zoe owns:** the *content* layer — the user-story scenario corpus, category
  clustering, coverage-enforcement guardrails, drift detection. She depends on F1
  (schema) and F3 (live driver) but does not build them.
- **Everyone else owns the plumbing:** Felix (KAS mock/contract), Kenneth (deterministic
  smoke port + deploy gate), Adam (static analysis), and TBD owners for Auto-UX,
  Auto-Review, Fuzz, and the compatibility matrix.

The clean seam: **Zoe authors scenarios; the framework team makes scenarios runnable.**
As long as F1's schema is frozen early, both can proceed in parallel.

---

## 2. Foundation work (must land first)

These four items unblock every parallel workstream. Sequence them ahead of everything
else; nothing in §5 Phase 2+ should start before its foundation dependency is merged.

### F1 — Scenario schema + `Scenario` type  *(P0, ~4d, blocks the most)*
- Author `scenarios.schema.json` (the repo already references it but it doesn't exist).
- Define the `Scenario` TypeScript type; validate the existing 56-scenario `scenarios.json`
  corpus against it (must parse cleanly — this proves the schema fits reality).
- Wire the `verify[]` predicates (`screen.contains`, `store.<path>`, `agent.tool`,
  `file.exists`, `exit.code`) onto the existing `E2ETestCase` capture APIs.
- **Freeze the schema before Zoe and the corpus authors start.** Schema churn after
  corpus authoring begins is the top project risk.

### F2 — KAS mock-LLM wiring  *(P0, ~2d, KAS changes required)*
- Resolve the HLD open question first: can KAS point at a **fake model server** that
  returns canned responses so the real KAS agent runs unchanged? This is the preferred
  design (mock at the model endpoint, not the ACP wire) because mocking at the ACP wire
  leaves the exact KAS-agent gap we're trying to close.
- Build KAS mock-mode: fake model endpoint returning scenario-canned responses.
- Fallback if infeasible: document the ACP-wire mock as a degraded option (does not
  exercise KAS agent/tools/session — call this out explicitly as reduced coverage).

### F3 — Knight Rider act-react extension  *(P1→pull earlier, part of foundation)*
- Extend the existing HTTP driver so a driver-LLM works *toward a scenario goal* and
  reacts to what the agent actually did (not blind replay).
- Add the guidance/tolerance API (steering prompts + per-turn retry budget).

### F4 — Design-system rubric document  *(P1→pull earlier, prerequisite)*
- Write down spacing, glyph/icon usage, status-indicator behavior, color/contrast, and
  the accessibility rules (ASCII mode, animations off, icons off).
- This is authored from the existing theme files + glyph set; it is a **writing** task,
  not a coding task, and can run fully in parallel with F1/F2.

### F5 — AGENTS.md + steering scaffolding renewal  *(P0, Phase 0, LLD 19)*

Added as a foundation because every downstream workstream produces **agent-authored**
artifacts — generated scenarios, review findings, UX findings. If the instruction layer is
stale when those start, every generated artifact inherits the staleness.

- Split the 422-line root `AGENTS.md` into **orientation** (what the codebase is) and move
  volatile content out: always-on rules into a new `.kiro/steering/`, procedures into skills.
- The repo already uses nested `AGENTS.md` (root + `packages/tui` + `packages/twinki`), so
  the nesting convention is kept; there is currently **no** `.kiro/steering/` at all.
- **KiroCrew is the reference parameter** for the three-way split (orientation / steering /
  skills), rule-with-a-negative phrasing, and the corrections-to-durable-lessons path.
- Detail in [`19-lld-agents-steering-renewal.md`](19-lld-agents-steering-renewal.md).

**Foundation exit criteria:** schema frozen and corpus parses against it; KAS deterministic
run of one p0 scenario passes end-to-end; Knight Rider drives one live scenario to a
judged pass; rubric doc reviewed and merged; `scripts/testing-cert-status` exits 0 and a
cold agent can run the KAS test path from a skill with no extra prompting.

---

## 3. Learnings to distill from KiroCrew (AI-native release foundations)

Reviewed `github.com/kirodotdev/KiroCrew`. Patterns worth adopting for a *fully
certified, AI-native build-and-release* process:

1. **Multi-channel release train with a promotion ladder.** KiroCrew ships
   `nightly → insider → stable` as first-class channels, each with its own CDN artifact
   and auto-update. Kiro CLI already has nightly/RC/stable; formalize the **promotion
   gate between channels** as the place the automated tiers attach (deterministic gate =
   merge→nightly; scenario/UX/smoke = nightly→RC; full matrix + live = RC→stable). This
   is exactly the "absorb the bug bash into the pipeline" framing.
2. **SHA-256-verified artifacts from a release CDN.** Every KiroCrew install path is
   checksum-verified. Adopt artifact attestation as part of RC certification so "certified
   build" means *verified bytes*, not just *green tests*.
3. **`doctor` as a machine-checkable readiness gate.** `kirocrew doctor` self-verifies the
   environment. Add a Kiro CLI equivalent that the pipeline runs as a hard gate — the
   compatibility-matrix work (§Phase 3) is essentially a distributed `doctor`.
4. **Self-learning from failures → durable lessons.** KiroCrew converts corrections and
   task failures into persistent lessons. This directly maps to HLD goal #11 (self-learning
   applied to scenario development) and to the fuzz tier: **every confirmed fuzz repro and
   every acted-on review comment should crystallize into a new deterministic scenario or a
   review rule.** Build the feedback loop, don't just file bugs.
5. **Skills as crystallized reusable patterns.** KiroCrew synthesizes repeated patterns
   into reusable skills. Treat the scenario builder + fixtures the same way: a library of
   reusable scenario fragments (login flow, approval flow, sub-agent spawn) so authoring a
   new user story composes existing pieces rather than restarting.
6. **Checkpointed long-running tasks with per-step validation.** KiroCrew's task runner
   plans → executes → validates → retries → resumes from checkpoints. This is the model for
   the live scenario driver's guidance/tolerance loop: validate each turn, steer within
   budget, resume rather than hard-fail.
7. **Defense-in-depth governance profiles + audit events.** Relevant to the auto-fix agents
   (P1 out-of-scope) and to the review bot's blocking authority — gate destructive/blocking
   automation behind confidence + audit, mirroring KiroCrew's approval/audit model.

Net: KiroCrew's contribution is the **release-certification spine** (channel promotion +
verified artifacts + machine readiness gate + failure→lesson loop). Kiro CLI's testing
tiers become the *checks that attach to that spine*.

---

## 4. Overlap with Zoe's work (and why it's acceptable)

Zoe owns *User Stories Scenarios Testing*. This plan overlaps her scope in three places:

| Overlap point | This plan | Zoe's work | Why acceptable |
|---|---|---|---|
| Scenario schema (F1) | Framework team authors/freezes the schema + `Scenario` type | Zoe authors scenarios *against* that schema | Shared foundation. One schema serving both PR gate and Q/A is an explicit HLD goal — divergence here is the failure mode, so overlap is the point. |
| Coverage enforcement | Auto-Review scenario-gap detection flags PRs with no covering scenario | Zoe builds coverage-enforcement + drift-detection guardrails | Two ends of the same guardrail: the bot flags at PR time, Zoe's guardrails enforce at the suite level. They must share the tagging taxonomy — coordinate, don't fork. |
| Scenario corpus | Fuzz tier converts confirmed repros into scenarios; Auto-Review generates scenario stubs | Zoe curates the human-authored user-story corpus | Both feed one corpus. Provenance differs (fuzz-derived / bot-stub / human-authored) but they live in one place under one schema. |

**Why the overlap is deliberate and safe:** these are *shared AI-native foundations*, not
duplicated effort. The HLD's central bet is "author a scenario once, run it in every tier."
That only pays off if the schema, tagging taxonomy, and corpus are shared. The coordination
contract is:

- Framework team owns **schema + runner** (F1).
- Zoe owns **corpus + taxonomy + drift guardrails**.
- Auto-Review/Fuzz owners **contribute to the corpus** through Zoe's taxonomy, not around it.

Freeze F1 early and publish the tagging taxonomy as a shared doc; the overlap then becomes
collaboration on one artifact rather than competing implementations.

---

## 5. Sequencing — phased execution

Milestone-driven. Each phase has a hard exit gate; the next phase's parallel workstreams
depend on the prior phase's foundation being merged.

### Phase 0 — Foundations (weeks 1–3)  *serial-ish, unblocks everything*
- F1 scenario schema + type + `verify[]` wiring **(freeze the schema)**
- F2 KAS mock-LLM (resolve fake-model-endpoint question, build mock mode)
- F4 design-system rubric doc (parallel, writing task)
- F3 Knight Rider act-react + guidance/tolerance API (start; can trail into Phase 1)
- F5 AGENTS.md + steering renewal, KiroCrew as reference (LLD 19; parallel, docs-only)
- **Exit gate:** schema frozen + corpus parses; one p0 scenario runs deterministically on
  real KAS; rubric merged.

### Phase 1 — Deterministic gate on KAS (weeks 3–6)  *the biggest hole first*
- Build KAS deterministic E2E harness (spawn KAS + mock LLM)
- Author initial p0 scenario set (Zoe + framework team, against frozen schema)
- Implement deterministic execution mode + structural assertions
- Port smoke suite to deterministic permutation (Kenneth); deploy gate/blocker
- Integrate deterministic gate into PR CI (Linux first)
- **Exit gate:** p0 deterministic scenarios gate every PR and block merge on failure;
  smoke deterministic run blocks deployment; Sev2.5 escalation + auto-ticket + Slack
  `kiro-cli-ops` wired.

### Phase 2 — Live Q/A + review intelligence (weeks 6–10)  *parallel*
- Live execution mode via Knight Rider + LLM-judge assertions
- Tag/priority/cadence runner (PR / RC / nightly / release selection)
- KAS contract publication + compile-time enforcement + version-bump validation (Felix)
- Auto-Review: inline diff comments → scenario-gap detection → stub suggestion →
  confidence-gated blocking
- Static analysis: TS linting/complexity/duplication (Adam) + per-PR coverage report
- **Exit gate:** RC runs the full live scenario suite; review bot posts inline + flags
  missing coverage; coverage report enforced (no regressions).

### Phase 3 — Coverage breadth + UX + fuzz (weeks 10–16)  *parallel*
- Auto-UX Review: baseline frames → path-based triggers → review agent vs rubric →
  blocking → (fast-follow) diff-classification trigger
- Compatibility matrix: CPU arch / OS variants, hard-dependency tests (glibc etc.),
  terminal-specific (Ghostty, iTerm, tmux), model-specific
- Fuzz tier: generator → round-robin subsystem rotation → repro minimizer →
  confirmed-repro-to-scenario feedback loop
- Zoe's full user-story corpus across all surfaces + drift guardrails
- **Exit gate:** UX regressions caught on PR; compatibility matrix runs on RC; fuzz runs
  continuously and its repros land as scenarios.

### Phase 4 — Certify & remove the bug bash (weeks 16–18)
- Adopt KiroCrew release spine: channel-promotion gates + SHA-256 artifact attestation +
  machine readiness gate (`doctor` equivalent)
- Run the automated tiers alongside a manual bug bash for 2–3 RCs to prove parity
- **Exit gate (the goal):** two consecutive RCs certified with zero bug-bash-only findings
  → **remove the bug bash from the release checklist.**

**Milestone summary:**

| Milestone | Phase | Removes |
|---|---|---|
| M1 KAS deterministic gate live | 1 | PR-level regression risk |
| M2 Live Q/A + review intelligence | 2 | Manual workflow validation + coverage blind spots |
| M3 UX + compatibility + fuzz | 3 | Visual/platform/unscripted blind spots |
| M4 Certified release spine | 4 | **The manual bug bash** |

---

## Top risks

- **Schema churn after corpus authoring starts** — freeze F1 before Phase 1. #1 risk.
- **KAS fake-model-endpoint turns out infeasible** — resolve in Phase 0; the ACP-wire
  fallback is strictly weaker coverage, so know early.
- **Live-tier flakiness erodes trust** — keep deterministic as the only hard PR gate;
  live tiers inform RC, never block merge.
- **Review-bot noise** — confidence-gate blocking hard; a bot the team learns to ignore is
  worse than no bot.
```
