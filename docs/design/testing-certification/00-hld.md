---
program: testing-certification
doc_type: hld
id: "00"
title: Kiro CLI Testing — High-Level Design
owner: Kenneth Sanchez
status: in-review
---

# Kiro CLI Testing — High-Level Design

Status: In Review · Owner: Kenneth Sanchez · Date: 2026-08-06
Audience: Kiro CLI engineers, release on-call, contributors
Companion documents: `01-foundation-architecture.md`, `02-implementation-plan.md`
Related existing docs: [`docs/testing.md`](../../testing.md) (canonical test inventory),
[`docs/review-playbook/`](../../review-playbook/) (review checks, not operational today)

---

## Problem

Kiro CLI ships nightly and RC builds on a schedule, but promoting an RC to stable still
depends on a **manual bug bash**. Engineers spend hours driving the CLI by hand to catch
regressions the automated suite misses. It is slow, it does not scale with contribution
velocity, and it is the main bottleneck between code landing and reaching customers.

The competitive pressure is real: Claude Code shipped 74 releases in 52 days in Q1 2026,
Codex CLI crossed 709 releases by mid-April, both gating entirely on automated
verification with no manual checkpoint between merge and release.

**The gap is structural, not quantitative.** We have ~4,400 Rust tests and 350+ TypeScript
test files. But:

- **KAS (V3) — the production engine going forward — has no deterministic full-stack
  coverage that gates PRs.** The only thing that exercises KAS end to end is the
  LLM-driven smoke suite, which is non-deterministic and therefore cannot gate.
- Almost nothing exercises **real multi-step workflows**. Every E2E test validates a
  single prompt and response, so bugs where turn-1 state affects turn-4 behaviour are
  invisible.
- There is **no automated UX review**. Spacing, glyph, status-indicator, and accessibility
  regressions are caught only by a human looking at the screen.
- The auto-review bot reviews code but **does not detect missing test coverage**.
- There is **no continuous exploratory or fuzz testing** — the manual bug bash is the only
  exploratory coverage.

The smoke suite has the right feature breadth (56 scenarios, both engines, three
platforms) but the wrong foundation for gating: it is LLM-driven on both the driving and
the judging side.

## Goals

1. Remove the manual bug bash from the release process.
2. Close the deterministic E2E gap for KAS.
3. Validate real multi-step user workflows.
4. Automate UX review against a defined design system.
5. Make the auto-review bot detect missing test coverage.
6. Run exploratory and fuzz testing continuously.
7. Enforce bounded contracts between components (ACP, KAS).
8. Static analysis for feature coverage.
9. Non-interactive test history.
10. Multi-client coverage: iTerm, Ghostty, macOS Terminal.
11. Apply self-learning to scenario development.

## Non-goals

1. Unit-test practices — the existing unit suite stays as is.
2. Replacing the Rust V2 test suite — V2 coverage remains until the engine retires.
3. Load or performance testing beyond the existing input-latency checks.

## Proposed solution

**Scenario-based testing is the shared foundation.** One scenario definition runs in two
modes: deterministic (mocked LLM) as a PR gate, and live (real LLM via Knight Rider) as
RC Q/A. The UX review and fuzz tiers build on the same scenario and Knight Rider
infrastructure, so a scenario authored once pays off across several tiers.

Five pillars, all resting on the substrate in
[`01-foundation-architecture.md`](01-foundation-architecture.md):

| Pillar | What it does | Gates |
|---|---|---|
| Scenario-based Q/A testing | Typed JSON scenarios, two execution modes, guidance/tolerance for live runs | PR (deterministic), RC (live) |
| Deterministic smoke / E2E | Real KAS agent + mocked model endpoint; closes the KAS gap | PR-blocking |
| Auto-UX review | Knight Rider frames evaluated against a written design system | PR-blocking when confident |
| Auto-review enhancements | Inline comments, scenario-gap detection, confidence-gated blocking | PR |
| Fuzz testing | Randomized action sequences, round-robin subsystems, repro minimization | Continuous, non-gating |

**What removes the bug bash is the combination**, not any single tier: deterministic
scenarios catch regressions on PR, live Q/A validates real workflows on RC, UX review
catches visual regressions, fuzz covers the unscripted space.

## Assumptions

- KAS (V3) is the engine we optimize for; the harness supports both engines but priority
  targets KAS.
- Knight Rider is the harness for live runs — we extend it rather than build a new driver.
- Live-mode scenarios are not byte-for-byte repeatable; we assert on outcomes with a
  tolerance/retry budget.
- Deterministic and live modes share one scenario definition.
- The design system does not exist as a document today. Writing it is part of this work.

## Open questions

Tracked in `02-implementation-plan.md` and each LLD's §9. The two that gate Phase 0:

1. **Can we mock the LLM for KAS at the model endpoint** (fake model server) so the real
   KAS agent runs deterministically? If not, the ACP-wire fallback does not exercise the
   KAS agent, tools, or session — materially weaker coverage.
2. **Is the live retry/tolerance budget per-turn, per-scenario, or both?**

## Provenance

This document is the in-repo record of the reviewed HLD. The review thread and comment
history live outside the repo; this file is canonical for implementation. When the two
diverge, update this file and note the change here.
