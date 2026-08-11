# AGENTS.md — Testing & Release Certification program docs

Scope: `docs/design/testing-certification/`. Read with the root `AGENTS.md`, not instead
of it.

## What this directory is

The design record for the program whose goal is **removing the manual bug bash from the
Kiro CLI release process**. `00-hld.md` states the problem, `01-foundation-architecture.md`
defines the shared substrate (F1–F4), `02-implementation-plan.md` holds the dependency map
and phasing, and each `1X-lld-*.md` is one owner's low-level design.

`program.json` is the machine-readable view: ownership, dependency edges, and status.
Read it before answering any question about who owns what or what is blocked.

## Rules when editing here

- **Frontmatter is a contract.** Every doc carries `program`, `doc_type`, `id`, `owner`,
  `status`. LLDs also carry `depends_on`, `unblocks`, `foundations_used`, `plan_phase`.
  Never drop a key; `scripts/testing-cert-status` parses them and CI will fail.
- **`status` uses the fixed vocabulary** in `program.json`: `not-started`, `drafting`,
  `in-review`, `approved`, `implementing`, `done`. No other values.
- **Update `program.json` in the same change** as a frontmatter `status` or `owner` edit.
  Two sources of truth that disagree are worse than one that is stale.
- **Do not edit `LLD-TEMPLATE.md`** to start an LLD. Copy it.
- **A `> FILL:` line left in place means that section is not done**, whatever the
  frontmatter claims. Do not mark a doc `in-review` while FILL lines remain.
- **Acceptance criteria must be commands or checks**, never prose. If you cannot express
  it as something runnable, say so in §9 Open questions rather than writing a sentence
  that sounds like a criterion.
- **Do not duplicate the test inventory here.** `docs/testing.md` is canonical for tier
  counts and coverage floors; link to it.
- **Numbering has deliberate gaps** (03–09, 21–22). Use them for inserts rather than
  renumbering.

## Invariants that are easy to break

1. **One schema, one corpus.** The program's central bet is that a scenario authored once
   runs in every tier. If a change implies a second scenario format, that is a program
   decision — raise it as an open question, do not implement it.
2. **Deterministic gates block; non-deterministic tiers inform.** Never write a design in
   which a live-LLM or fuzz result blocks a merge.
3. **Foundations freeze.** Once a foundation is `approved` and logged in the freeze table
   in `01-foundation-architecture.md`, LLDs may assume it. Changing it afterwards requires
   a freeze-log entry and notifying every owner in its `blocks` list.
4. **Ownership for Zoe, Felix and Adam is assumed, not confirmed** — see
   `assumed_ownership_unconfirmed` in `program.json`. Do not present it as settled.

## Verifying a change

```bash
scripts/testing-cert-status            # frontmatter + program.json consistency
scripts/testing-cert-status --blocked  # what is blocked on what
```

Run it before opening a PR that touches this directory.
