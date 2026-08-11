---
program: testing-certification
doc_type: index
title: Kiro CLI Testing & Release Certification — Program Index
owner: Kenneth Sanchez
status: drafting
---

# Kiro CLI Testing & Release Certification

**Goal: remove the manual bug bash from the release process** by absorbing its coverage
into automated gates.

This directory is the program's single source of truth. The HLD states the problem, the
foundation-architecture document defines the shared substrate, and each owner brings one
LLD. `program.json` is the machine-readable view of the same information — agents and CI
read it, humans read this file.

## Documents

| Doc | Purpose | Owner | Status |
|---|---|---|---|
| [00-hld.md](00-hld.md) | Problem, goals, proposed solution | Kenneth Sanchez | Drafting |
| [01-foundation-architecture.md](01-foundation-architecture.md) | The shared substrate (F1–F4) all workstreams build on | Kenneth Sanchez | Drafting |
| [02-implementation-plan.md](02-implementation-plan.md) | Dependency map, phasing, KiroCrew learnings | Kenneth Sanchez | Drafting |
| [LLD-TEMPLATE.md](LLD-TEMPLATE.md) | Copy this to start your LLD | — | — |

### Low-level designs (one per workstream owner)

| Doc | Workstream | Owner | Phase | Depends on |
|---|---|---|---|---|
| [10](10-lld-scenario-framework.md) | Scenario runner & framework: schema, runner, assertions | Kenneth Sanchez | 0 | F1, F3 |
| [11](11-lld-kas-mock-and-contract.md) | KAS mock-LLM + ACP contract enforcement | Felix Ding | 0 | F2 |
| [12](12-lld-user-story-scenarios.md) | User-story scenarios, taxonomy, drift guardrails | Zoe Lin | 1 | 10 |
| [13](13-lld-deterministic-smoke-and-gates.md) | Deterministic smoke suite + release gates | Kenneth Sanchez | 1 | 10, 11 |
| [14](14-lld-auto-review.md) | Auto-review: inline comments, scenario-gap detection | TBD | 2 | 10, 12 |
| [15](15-lld-auto-ux-review.md) | Auto-UX review: frames vs design system | TBD | 3 | F3, F4 |
| [16](16-lld-fuzz.md) | Fuzz: generator, rotation, repro minimizer | TBD | 3 | 10, 11 |
| [17](17-lld-compatibility-matrix.md) | Compatibility matrix: arch, OS, terminal, deps | TBD | 3 | 13 |
| [18](18-lld-static-analysis-coverage.md) | Static analysis + coverage enforcement | Adam Cervantes | 2 | — |
| [19](19-lld-agents-steering-renewal.md) | AGENTS.md + steering scaffolding renewal | TBD | 0 | — |

### Reference documents (foundation artifacts)

| Doc | Purpose | Foundation |
|---|---|---|
| [20-scenario-schema-reference.md](20-scenario-schema-reference.md) | Normative reference for `scenarios.schema.json` | F1 |
| [23-design-system.md](23-design-system.md) | The rubric the UX review agent enforces | F4 |
| [24-release-certification.md](24-release-certification.md) | Channel promotion ladder, artifact attestation | — |

Numbering leaves gaps (03–09, 21–22) deliberately, so a document can be inserted without
renumbering the set. Test inventory and tier definitions are **not** duplicated here —
[`docs/testing.md`](../../testing.md) already holds them and stays the canonical inventory.

## How to contribute your LLD

```bash
cd docs/design/testing-certification
cp LLD-TEMPLATE.md 1X-lld-<your-workstream>.md   # or edit the stub already there
```

Then:

1. Fill every `> FILL:` line and delete it. A section left as a FILL line reads as
   not-started regardless of what the frontmatter says.
2. Update the frontmatter `status` as you go. It is machine-read — do not remove keys.
3. Be explicit in §2 about what you are **assuming** about a foundation that is not yet
   approved. That field is how the lead finds coupling risk before it becomes rework.
4. §6 acceptance criteria must be commands or checks, not prose. "Scenarios run reliably"
   is not a criterion; `bun test <path>` exiting 0 is.
5. Open a PR per LLD. LLDs are reviewed like code.

## Rules of engagement

- **One schema, one corpus.** The program's central bet is that a scenario authored once
  runs in every tier. If your workstream wants its own scenario format, raise it as an
  open question instead of forking.
- **The foundations are frozen once approved.** F1 in particular: schema churn after
  corpus authoring begins is the top program risk.
- **Deterministic gates block; live tiers inform.** Nothing non-deterministic blocks a
  merge. Live and fuzz tiers report at RC and nightly.
- **Ownership shown here for Zoe, Felix and Adam is assumed, not confirmed.** See
  `assumed_ownership_unconfirmed` in `program.json`. Correct it if wrong.

## Status at a glance

```bash
scripts/testing-cert-status        # parses frontmatter + program.json
```
