<!--
  Generated from LLD-TEMPLATE.md. Fill every "> FILL:" line and delete it.
    Owner: TBD
  Keep the frontmatter keys; they are machine-read.

  Status vocabulary: not-started | drafting | in-review | approved | implementing | done
  Keep the keys; change only the values. `status` must be one of:
  not-started | drafting | in-review | approved | implementing | done
-->
---
program: testing-certification
doc_type: lld
id: 17
title: Extended Compatibility Matrix (arch, OS, terminal, deps) — Low-Level Design
owner: TBD
status: not-started
depends_on: ["13"]
unblocks: []
foundations_used: []
hld: 00-hld.md
plan_phase: 3
---

# Extended Compatibility Matrix (arch, OS, terminal, deps) — Low-Level Design

Status: Not Started · Owner: TBD · Date: TBD
Audience: <who reviews this>
Companion documents: `00-hld.md`, `01-foundation-architecture.md`, `02-implementation-plan.md`
Related existing designs: <paths under docs/design/, or "none">

---

## 1. Scope

> FILL: One paragraph. What this workstream delivers, and explicitly what it does
> not. If a reader could confuse your scope with a neighbouring workstream, name
> the neighbour and draw the line here.

## 2. Foundation dependencies

> FILL: Which of F1–F4 you consume, and what you assume about each. If you are
> assuming a shape for a foundation that is not yet approved, say so — this is the
> field the program lead reads to find coupling risk.

| Foundation | What you consume | Assumption you are making |
|---|---|---|
| F1 scenario schema | | |
| F2 KAS mock-LLM | | |
| F3 Knight Rider driver | | |
| F4 design-system rubric | | |

## 3. Design

> FILL: The actual design. Module boundaries, data flow, file paths you will add
> or change, and the interfaces you expose to other workstreams. Diagrams welcome
> (ASCII, consistent with docs/testing.md).

## 4. Interfaces you expose

> FILL: Anything another workstream or an agent will call, import, or parse.
> Treat this as a contract — changing it later requires a note in §9.

## 5. Test strategy for this workstream

> FILL: How this work is itself tested. "The tests test the tests" is not a joke
> here: a harness with no coverage of its own failure modes will report green
> while detecting nothing. Cover at minimum: does it fail when it should?

## 6. Machine-checkable acceptance criteria

> FILL: Numbered, each one a command or check a human or an agent can run to
> verify done. Prose acceptance criteria are not acceptable in this program.

1. `<command>` exits 0 and asserts <what>
2.

## 7. Rollout and gating

> FILL: Where this attaches to the pipeline (PR gate / nightly / RC / release),
> whether it blocks, and what the escape hatch is when it produces a false
> positive at 2am.

## 8. Risks

> FILL: Top 2–3, each with the mitigation. Include the risk that this workstream
> is late and what unblocks around it.

## 9. Open questions

> FILL: Anything you need the program lead or another owner to decide. Move each
> to a decision in §3 once answered, and leave a dated line saying who decided.

## 10. AI-native notes

> FILL: What an agent needs to know to work in this area productively — the
> entry-point files, the invariants that are easy to break, the commands to
> verify a change. Anything durable here should graduate into an AGENTS.md or a
> steering file (see 19-lld-agents-steering-renewal.md).
