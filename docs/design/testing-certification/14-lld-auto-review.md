---
program: testing-certification
doc_type: lld
id: 14
title: Auto-Review Enhancements (multi-model review, scenario-gap detection, gate blocking) — Low-Level Design
owner: Adam Cervantes
status: in-review
depends_on: ["10","12"]
unblocks: []
foundations_used: ["F1"]
hld: 00-hld.md
plan_phase: 2
---

# Auto-Review Enhancements (multi-model review, scenario-gap detection, gate blocking) — Low-Level Design

Status: In Review · Owner: Adam Cervantes · Date: 2026-08-12
Audience: Kiro CLI engineers, release on-call, contributors
Companion documents: `00-hld.md`, `01-foundation-architecture.md`, `02-implementation-plan.md`
Related existing designs: `.kiro/skills/semantic-pr-reviewer/SKILL.md` (the current reviewer's de-facto design of record)

---

## 1. Scope

Enhance the existing PR auto-review bot to run several specialized reviewers in parallel, detect missing scenario coverage, and gate the PR on a deterministic, confidence-based check. The bot today runs a single reviewing pass (an orchestrator delegating to one model reviewer) that posts verdict-carrying reviews (Approved, Comment, Requires Changes), recalls prior findings through its reviewer memory, and suggests reviewers; what it cannot do is tell whether a change is covered by scenarios, and its findings never gate a merge. The gap is the mechanism, not the memory: finding identity, lifecycle, and confidence live in prompts and review history today, with nothing deterministic for a gate to consume.

**Multi-model reviews** provide specialized reviewer lanes, each running at least two independent model reviewers for its review type.

**Scenario-gap detection** flags a PR that adds or changes a user-facing surface for which no covering scenario exists.

**Scenario stub suggestion** emits a scenario stub the author can adopt when a gap is confirmed.

**Confidence-gated blocking** adds a deterministic gate that blocks the PR on high-confidence findings.

**Architecture/design review** adds a reviewer lane that judges the structure of a change against the codebase's established patterns.

**Readiness labels** project the gate state onto the PR as bot-managed labels, queryable without opening the PR.

**Local review skill.** Run the same review lanes and gate against a working tree before any push, reporting findings without posting anything.

**Security fixes** are carried as design constraints: a PR cannot rewrite the review guidelines that judge it; the review-skip control is maintainer-gated; an incomplete or absent review reads as a failure, never as a silent pass.

**Not in scope**

- **Scenario corpus, tagging taxonomy, drift guardrails**: owned by LLD 12; the taxonomy is consumed and coordinated on, not authored here.
- **Code coverage and static analysis**: LLD 18. Below, *coverage* always means scenario coverage, never code coverage.
- **UX and design-system review**: owned by LLD 15, which judges the rendered UI against the design-system rubric; the architecture/design lane judges the structure of the code, not the pixels.
- **AGENTS.md and steering renewal**: LLD 19.
- **Scenario execution**: the gate checks that a covering scenario exists but never runs it; execution belongs to the deterministic and live tiers (LLDs 13 and 10).

## 2. Foundation dependencies

Only F1 is consumed. The review reads the PR diff and the scenario corpus's coverage metadata; it does not execute the agent, drive a running TUI, or judge rendered UI.

| Foundation | What you consume | Assumption you are making |
|---|---|---|
| F1 scenario schema | The schema (today at `packages/tui/e2e_tests/smoke/scenarios.schema.json`; its reference doc `20-scenario-schema-reference.md` is an unwritten stub): the tag vocabulary to query for covering scenarios, and the full scenario shape to emit stubs that validate against it. | The schema carries a coverage-tag field expressive enough to map a changed surface to scenarios; F1 is not started and the schema on disk is scoped to Knight Rider smoke tests, so this consumes a shape that is not yet approved. |
| F2 KAS mock-LLM | Nothing | Reviewer lanes call their models through the review workflow; no part of the review executes the agent against a mocked model endpoint. |
| F3 Knight Rider driver | Nothing | The review operates on the PR diff and repository state; nothing in it drives a running TUI. |
| F4 design-system rubric | Nothing | The architecture/design lane judges code structure against the codebase's established patterns; no lane judges rendered UI against visual rules. |

## 3. Design

### Review pipeline

One review pipeline runs at a time per PR, re-runnable at the same head SHA (reopen, rerun), split into a deterministic shell, model-driven reviewer lanes, and a conditional verification pass:

1. **Preflight (deterministic).** The auto-review bot loads the review guidelines from the base branch, builds each lane's prompt from its guideline body, honors a maintainer-recorded, SHA-scoped skip, and maps the diff to changed surfaces. No model calls happen in this step; the prompts, the gate policy, the skip decision, and the surface map are fixed deterministically before any model reads the diff.
2. **Reviewer lanes (model-driven).** A lane is the pipeline's track for one review type. It runs a configurable number of independent model reviewers (at least two; two by default) under the lane's prompt, each following the discovery-then-falsification protocol below, and emits the findings that survive. Lanes run in parallel with each other and the reviewers inside a lane run in parallel as well. Reviewers that independently emit the same finding raise its agreement count, the confidence measure the gate consumes. Each finding later surfaces as a blocking or non-blocking comment; that marking is the gate's decision (step 5), applied per the predefined gate policy.
   - **Functional**: logic errors, regressions, missed edge cases.
   - **Security**: security issues introduced by the change, from unsafe code patterns to suspicious new dependencies.
   - **Scenario coverage**: user-facing changes with no covering scenario.
   - **Architecture/design**: the structure of the change against established patterns.
3. **Aggregation (deterministic).** The auto-review bot dedups findings across lanes and across pushes by fingerprint, carries finding state forward from prior pushes, and computes each finding's agreement count.
4. **Verification (model-driven, conditional).** Review is not a vote: when a finding's agreement is 1 and its kind's gate policy sets `escalate_lone`, the auto-review bot dispatches one verification pass. A fresh reviewer receives the finding as untrusted input with a prove-or-disprove charter fixed at preflight and must re-derive it from the code; a finding that does not survive stays non-blocking. A verification call that errors is a run failure under Decision 6, never a non-survival: only a completed pass that fails to re-derive the finding leaves it non-blocking.
5. **Gate (deterministic).** A finding blocks if and only if the gate policy marks its kind as blocking, it meets the kind's required agreement count (a lone finding meets it by surviving verification), and (for scenario-gap findings) the structural check confirms no covering scenario exists. Every push triggers a full re-review.
6. **Report.** The auto-review bot posts or updates inline comments and the summary comment, records the machine-readable finding record in the check-run output (Decision 20), posts the validated scenario stub when the lane confirms a gap, sets the check-run conclusions (the required `auto-review` and the per-lane advisory checks), and syncs the readiness label.

```
PR opened / reopened / push / ready for review / review command
  -> preflight (deterministic): base-branch guidelines; skip check; diff -> surfaces
  -> reviewer lanes (parallel; per reviewer: discovery -> falsification -> survivors):
       functional | security | scenario-gap (+ stub on confirmed gap) | architecture
  -> aggregate (deterministic): fingerprint dedup across lanes and pushes; agreement count
  -> verify (conditional): lone finding in an escalate_lone kind -> prove-or-disprove pass
  -> gate (deterministic): kind marked blocking AND (agreement >= required count OR verified lone)
                           AND, for scenario-gap findings, the structural check confirms the gap
  -> report: inline comments; summary comment; finding record; stub; check-run conclusions; readiness label
```

### Review guidelines

One markdown file per lane under `.kiro/review/` (`functional.md`, `security.md`, `scenario-coverage.md`, `architecture.md`), shaped like a Kiro steering file. The YAML frontmatter carries the lane's gate policy: the kind vocabulary, with a blocking flag, a required agreement count, and an optional `escalate_lone` flag per kind. The markdown body carries the guidance the lane's prompt is built from, in rule form: each rule names its trigger, the failure mechanism, the evidence a reviewer must trace, and the preferred fix. The gate parses only the frontmatter and never interprets prose; reviewers read only the body. Editing the body changes what reviewers look for; editing the frontmatter changes what blocks.

### Reviewer protocol

Every finding-emitting reviewer is two model calls (the scenario-coverage lane's filter reviewers are the single-call exception; see that lane). The discovery call reads the diff under the lane's prompt and generates candidate findings with generous recall. The falsification call receives those candidates in a fresh context as untrusted input and works to kill them: a candidate survives only if this call re-derives the concrete input, the call path, and the observable outcome from code it opened itself. Only survivors are emitted; a finding the reviewer cannot ground is dropped, never emitted at reduced confidence. Reviewers falsify their own candidates, not another reviewer's; independence across reviewers is measured afterward as agreement. Reviewer calls are read-only: they read the diff (fetched over the API) and the base-branch tree for surrounding context, the head is never checked out (Decision 19), and they never build, run tests, or mutate state.

### Lane designs

**Functional lane.** Reviewers examine the diff with surrounding file context under the lane's prompt, looking for logic errors, regressions, and missed edge cases. Emits `functional` findings.

**Security lane.** Reviewers examine the diff for security issues introduced by the change: unsafe code patterns (command and query injection, secret handling, missing authorization, unsafe deserialization) and supply-chain risks such as a new dependency that is unmaintained, typosquatted, or otherwise suspicious. Known-vulnerability scanning of dependencies stays with static analysis (LLD 18); this lane judges what a vulnerability database cannot. Emits `security` findings.

**Scenario-coverage lane.** Two stages. The deterministic stage takes the changed surfaces from preflight and queries the corpus by the F1 coverage tags; a surface with zero covering scenarios is a candidate gap, and the query result is the finding's evidence. The model stage runs the lane's reviewers as independent filters: each judges whether the change actually adds or alters user-facing behavior (a refactor of a user-facing file is not a gap), and a candidate passes only when the reviewers agree it does. The deterministic stage supplies the evidence, so these reviewers filter rather than discover and run single calls instead of the discovery-then-falsification protocol. A gap is confirmed only when both stages agree; this is the structural confirmation the gate requires before a `scenario-gap` finding can block. On a confirmed gap the lane's output action emits a stub that validates against `scenarios.schema.json`, posted as a suggestion the author can adopt; the write and the post are performed by the deterministic report step (step 6), never by a reviewer, keeping the read-only allowlist intact.

**Architecture/design lane.** Reviewers examine the structure of the change against the codebase's established patterns: module boundaries, coupling, layering, and consistency with existing conventions, judged against the lane's guidelines. Emits `architecture` findings.

### Findings

Every lane emits findings in one shape:

- `class`: the finding type (functional bug, security issue, scenario gap, architecture concern); each class maps to one lane and its guideline file.
- `kind`: the defect category within the class, from the closed vocabulary in the lane's guideline frontmatter; the initial vocabulary is the categories each lane design names. The kind distinguishes two different findings at the same location and anchors the fingerprint.
- `location` and `message`: the file and line range the finding points at, and the explanation posted as the inline comment. The message is display text only; it never enters the finding's identity.
- `evidence`: the material supporting the finding, such as the diff hunk or the failed coverage query.
- `agreement`: the number of the lane's reviewers that independently emitted the finding; the gate compares it against the count the gate policy requires.
- `fingerprint`: a stable identity for the finding, hashed from the class, the kind, and the normalized location (line numbers shift when code above them changes); dedup and cross-push state carry on it.
- `suggestion` (optional): an apply-ready replacement for the finding's anchored range, present only when the fix is mechanical; the report step renders it as a committable suggestion block, and a finding without one states its fix as prose.

A finding presents as blocking or non-blocking, nothing finer. The marker is the gate's decision for that finding; class, kind, and agreement are gate inputs, never comment labels. A blocking finding's message states what fails, why it blocks, and one concrete fix. When the fix is mechanical, the finding carries it in the optional `suggestion` field and the comment renders it as a committable suggestion block: one click to apply rather than a copy-edit, the same adoptable treatment the scenario lane's stub already receives. A suggestion block appears only when the replacement is apply-ready for the comment's anchored range; otherwise the fix stays prose. The suggestion is model-authored code that a maintainer's click commits, and a suggestion block carries no addressing of its own: its file and range are the enclosing comment's anchor, which the report step constructs from the finding's model-authored `location`. The confinement check therefore runs where the addressing lives, with two predicates for two gates: an inline comment anchors to any diff-addressable line (hunk context included, matching what the platform accepts), and a finding whose `location` is not diff-addressable reports through the summary's watch-for list; a suggestion renders only when the anchored span lies within the diff's changed lines, since those are the bytes a maintainer's click commits, and a rendered suggestion replaces exactly the anchored span, replacement text of any length, so span growth is impossible by construction and a suggestion can never reach code its finding does not anchor. A finding anchored to context lines keeps its inline comment and states its fix as prose. When duplicates merge at aggregation, the merged finding keeps its suggestion only when every merged emission carries byte-identical text; these are the exact bytes a maintainer commits, so nothing nondeterministic chooses them. Rendering happens in the deterministic report step, so Decision 19's read-only reviewer allowlists are unaffected; the residual control on a rendered suggestion is the maintainer's explicit click. It must be addressed before merge: fixed and verified by the deterministic recheck (Decision 21 for code findings; scenario-gap findings use the coverage query), or dismissed as a false positive.

An example finding:

```json
{
  "class": "security",
  "kind": "injection",
  "location": { "file": "packages/tui/src/git.ts", "lines": "142-151" },
  "message": "branchName is interpolated into a shell command; a crafted branch name executes arbitrary commands. Pass the arguments as a list instead of building a command string.",
  "evidence": "exec('git checkout ' + branchName) added in this diff",
  "suggestion": "await execFile('git', ['checkout', branchName]);",
  "agreement": 2,
  "fingerprint": "9f3a2c17b04d"
}
```

### Finding lifecycle

Findings persist across pushes on their fingerprint; a new push recomputes findings and reconciles them against existing state. Duplicates merge at aggregation before anything posts, so they never surface as findings; when a change moves or reshapes a flagged site enough to change the fingerprint, the auto-review bot retracts its stale comment and the recomputed finding posts as a new open finding. Retraction minimizes the comment as outdated, never deletes it: human discussion beneath a retracted comment stays readable, and the audit trail survives re-fingerprinting churn (a large refactor moves many fingerprints at once). A dismissal is scoped to the finding's fingerprint: it holds across pushes while the finding remains the same finding, and when the site changes enough to re-fingerprint, the recomputed finding returns as a new open finding for fresh evaluation.

```
new finding
     |
     v
   open ---deterministic recheck passes after a push---> addressed
    | ^                                                      |
    | +----------the finding recurs on a later push----------+
    |
    +---maintainer dismissal (fingerprint-scoped)---> dismissed
    +---human resolve (any state; edge drawn once)---> resolved (terminal)
```

| State | Set by | Gate effect |
|---|---|---|
| `open` | bot | blocks while the gate condition holds |
| `addressed` | bot, only when a deterministic recheck passes after a push | clears |
| `dismissed` | maintainer (not the author), scoped to the fingerprint | clears; recorded to the lessons store |
| `resolved` | human only | terminal |

The gate reads these states, never GitHub's thread-resolved flag; otherwise an author could unblock a PR by resolving conversations.

### Report

Inline comments are the primary channel: each blocking finding posts at its location, and non-blocking findings post at their locations up to a per-run cap of fifteen. A blocking finding always posts, since it must be actionable where it blocks; the cap binds only non-blocking comments. Selection under the cap is deterministic (non-blocking findings rank by agreement count, ties broken by normalized location order), and the posted set always equals the selected set: a previously posted comment whose finding falls out of the selection on a later run is retracted like any superseded comment, so the same state renders the same comments on every run; findings held back by the cap remain in the finding record and are counted in the summary. Cap displacement deliberately reuses the retraction mechanic: the minimized comment does not mean the site moved or the finding closed, the finding stays open in the record, and the summary's held-back list is the truth surface. Each comment carries deterministic provenance wording derived from the run record, phrased for the two-reviewer default (raised independently by both reviewers; raised by one of two reviewers) and parameterized by the configured count; confidence surfaces as that provenance, never as a model-declared score or label. Each comment also links to its kind's section in the lane guideline (§4), pinned to the base-branch commit the run loaded. One summary comment per PR is updated in place across pushes, never re-posted per revision; it stays compact: a one-or-two line description of the verdict, a delta line, a one-line recommendation, and a watch-for list for findings that cannot pin to a line (chiefly the architecture/design lane), each entry carrying its finding's fingerprint so the fingerprint-addressed dismissal form (§4) can name it, plus, when the cap applied, the held-back non-blocking findings as a compact list (location, kind, and fingerprint, one line each) rather than a bare count, so the fingerprint-addressed dismissal form (§4) reaches a finding the cap never posted, and any pending dismissal proposals awaiting a maintainer. The delta line is required and states what changed since the last reported run (`since <sha>: N cleared · N new · N carried · N dismissed`, naming the prior record's head SHA); its counts come from the cross-push reconciliation the run already performs; when no prior record is reachable (a PR's first review, or state aged out of the window) the delta line is omitted and the summary carries the no-prior-state notice instead. The check-run conclusions and readiness label recompute on every push; when the last blocking finding clears, the verdict updates without a manual re-trigger. Only the newest review run for the PR writes PR-level state (the summary comment and the label); a review run superseded by a later start, whether by a new push or a same-SHA re-run, discards its writes. A record-update run (a dismissal or proposal taking effect, §4) is not a review run and always writes the record; it writes the projection surfaces (the summary, the conclusions, and the label) only under a publish-time guard: no review run for the head is in flight or started after it, and its record names the current head. Guarded out, it writes the record alone, and the review run's report step re-reads the newest record immediately before publishing, so relieving state recorded while a review was in flight folds into that review's projection instead of racing it. Both interleavings converge: the newest published verdict reflects the push and the dismissal alike. The report step's write set is closed to what step 6 names: inline comments, the summary comment, the finding record in the check-run output, the check conclusions (required and per-lane advisory), the readiness label, and the schema-validated stub; nothing else mutates.

### Readiness labels

The label is the run state projected onto the PR list:

| Run state | Label |
|---|---|
| in flight (preflight through reporting) | `readiness: checking` |
| gate clears | `readiness: passed` |
| gate blocks | `readiness: action required` |
| errored | `readiness: action required`; the check run fails; never `passed` |
| skipped | no label |
| draft, no run yet | no label; no conclusion posted (§7) |
| draft, reviewed | exactly one label per the verdict; no conclusion posted (§7) |

A maintainer-recorded skip is not an absent run: the skipped run publishes a passing `auto-review` conclusion annotated as skipped by the maintainer, so the required check clears instead of wedging the PR. The SHA binding is recorded by the skip run itself, not the comment: the run resolves the PR's head SHA at execution, writes the skip (SHA, maintainer, timestamp) into the pipeline's own §4 finding record in the check-run output (Decision 20; a comment store would be writable by the tier the skip gate excludes), and later runs re-verify against that record, so a skip never carries across a push. The skip run itself must complete and publish that conclusion; marker presence alone never satisfies the gate. Fail-closed (Decision 6) covers runs that start, or should have started, and do not complete; never a completed skip.

### Security mechanisms

The §1 constraints land as: the review guidelines are read from the base branch during preflight, so the PR head cannot influence the lane prompts or the gate policy; reviewer agents run under explicit read-only tool allowlists (no write tools, no shell execution), so a prompt-injected diff cannot drive mutation or secret use (Decision 19); the skip control is a maintainer-recorded marker pinned to a commit SHA and re-verified on every run, with maintainer authorization verified through the repository permissions API from the event's commenter identity, never from any author-editable field (the two command tiers are defined in §4); state that can relieve the gate (the recorded skip, dismissals, lifecycle status) is read only from the pipeline's own provenance-verified finding record in the check-run output, never from comment bodies, which write access can edit (Decision 20); a run that does not complete reports a failing check run, so an incomplete review never reads as a pass.

### Learning loop

A maintainer dismissal writes the finding's fingerprint and reason to a scoped review-lessons store that lanes consult at dispatch, so a dismissed false positive is not re-raised at the same site within that PR (suppression beyond the PR is an open question; §9); the store is review data, not steering docs. An adopted stub flows into the corpus through the normal PR path (coordinated with LLD 12). The loop also runs on misses: a defect that escaped review becomes a guideline-body rule through the normal PR path, with the incident that earned it linked from the rule. The store's format and location are an open question (§9).

### Local review skill

A local review skill at `.kiro/skills/local-auto-review/` runs the same review against a working tree before any push: it fans the lanes out as read-only sub-agents over the working diff, reads the same base-branch guideline bodies, runs under the same read-only tool allowlists (Decision 19), and pipes findings through the same `scripts/auto-review/gate`. The gate policy and fingerprinting are therefore byte-identical to the pipeline's; reviewer emissions still vary run to run, as they do in CI, and the verdict can differ for a stated reason: a local run reads no finding record, so it evaluates every finding as newly open, ignores dismissals and `addressed` state, and can block where CI passes. It prints blocking and non-blocking findings with the same provenance wording and posts nothing: no comments, no checks, no labels, no record. Beyond collapsing the author loop from push-and-wait to a command, it doubles as an offline harness: run across historical PRs it exercises kind binning and the agreement mechanic before any workflow exists, measuring the agreement-undercounting risk (§8) early instead of discovering it through a falling recall catch rate.

### Files added or changed

`.github/workflows/kiro-review.yaml` (extended: trigger types gain `reopened`, `ready_for_review`, and `pull_request_review_comment` (created), which the reply form of `/auto-review dismiss` arrives on (a review-thread reply is not an `issue_comment` event), plus draft-mode publication handling (conclusions withheld on drafts, §7); permissions gain `checks: write`, which publishing the `auto-review` conclusion requires and the current grants lack, and `actions: read`, which enumerating the pipeline's prior runs and verifying a record's producing workflow requires (§4); lane fan-out, gate step, check-run conclusions, label sync); `.github/workflows/review-recall.yaml` (new: the advisory nightly recall job from §5); `.kiro/review/` (new: per-lane guideline files; the contract is in §4); `.kiro/agents/pr-reviewer.json` and `.kiro/agents/semantic-reviewer.json` (existing single-reviewer path, generalized to per-lane agents); `.kiro/agents/resources/REVIEWER_PROMPT.md` (the orchestrator prompt, split into per-lane prompt scaffolds; each folds in its lane's guideline body); `.kiro/skills/semantic-pr-reviewer/SKILL.md` (the current review methodology the orchestrator delegates to, refactored per lane alongside the prompt scaffolds, with its `pr-memory` and `slack-publish` companion skills updated for the new summary and labels); `scripts/auto-review/` (new: surface mapping, fingerprint dedup, gate evaluation, label sync); `.kiro/skills/local-auto-review/` (new: the §3 local review skill, running the same lanes and gate against a working tree, print-only).

### Decisions

1. Lanes propose; the gate enforces. No model output blocks a merge directly (program invariant: deterministic gates block; non-deterministic tiers inform).
2. Cross-model agreement is a confidence input, not a blocking authority.
3. Stub suggestion is the scenario-gap lane's output action, not a lane of its own.
4. `addressed` requires a deterministic recheck; `resolved` is human-only.
5. The gate reads bot-tracked finding state, not GitHub thread resolution.
6. An incomplete or absent run reads as a failing check (fail-closed); a maintainer-recorded skip is neither, and publishes a passing conclusion (Decision 18).
7. Lessons live in a scoped review store, not in steering docs.
8. Findings present as blocking or non-blocking; no finer severity ladder appears in comments (team decision, 2026-08-11).
9. Blocking behavior is defined by the gate policy, not lane type; any lane can produce a blocking finding.
10. Every lane runs at least two independent reviewers; the count is configurable.
11. A finding-emitting reviewer is a discovery call followed by a falsification call in a fresh context; only findings the falsification call re-derives from the code are emitted. An unsure finding is dropped, never emitted at reduced confidence. (The scenario-coverage lane's reviewers filter deterministic evidence with single calls; §3.)
12. Confidence is the agreement count, not a score: no numeric or model-declared confidence exists in the pipeline, and comments render agreement as deterministic provenance wording derived from the run record.
13. Finding identity is class, kind, and normalized location; the kind comes from the closed vocabulary in the guideline frontmatter, and the free-text message never enters identity.
14. Duplicates merge before posting, and a finding whose fingerprint changes is a comment retraction (minimized as outdated, never deleted) plus a new open finding; the lifecycle carries no superseded state.
15. Dismissal is fingerprint-scoped: it survives pushes that leave the finding intact and lapses when the site changes enough to change the fingerprint.
16. Review guidance ships as per-lane steering-style guideline files: reviewer prose in the body, gate policy in the YAML frontmatter. The gate parses only frontmatter and never interprets prose.
17. Review is not a vote: a lone finding in a kind marked `escalate_lone` receives one prove-or-disprove verification pass, and survival substitutes for the missing agreement. Silence from other reviewers never discards a verified high-consequence finding.
18. A maintainer-recorded skip publishes a passing `auto-review` conclusion annotated as skipped; it is carved out of Decision 6's absent-run rule so the skip escape hatch works while the check is required. Decided 2026-08-12 by the owner.
19. The pipeline keeps the existing workflow's `pull_request_target` trigger: runs execute in base-repository context, so fork PRs are reviewed with the same machinery and secrets as today; safety rests on reviewers being read-only and consuming the head as an API diff, never checking out or executing it. Read-only is enforced, not assumed: each per-lane reviewer agent runs under an explicit read-only tool allowlist (no write tools, no shell execution), dropping the current invocation's trust-all-tools flag, which today bypasses the agents' runtime allowlists and policies (while their declared grants include write and shell tools); mutation is confined to the deterministic report step. Decided 2026-08-12 by the owner.
20. The machine-readable finding record lives in the check-run output the pipeline publishes, not in any comment: comment bodies are editable by write access, so a comment-stored record would place the gate's state one permission tier below the skip and dismiss commands that write it (the failure mode of the retired `[review:ignore]` marker). The summary comment renders a human-readable projection that is never read back as state, and commands are honored only from comment creation events, verified at event time; a command edited into an existing comment fires nothing. The store's boundary is stated as what holds, not as app isolation: check runs share the github-actions identity with every workflow in the repository, so readers verify a record's provenance (a run of the base-branch pipeline workflow itself) rather than trusting the check name, and rewriting the store requires a workflow change that lands through review; the defense is that no actor-editable surface is ever read as state. The store is also commit-addressed where the summary comment was PR-addressed, so the read enumerates the pipeline's own prior runs for the PR and dismissals survive a force-push through the replaced head's record (§4). Decided 2026-08-13 by the owner.
21. A code finding (functional, security, or architecture lane) moves to `addressed` by a deterministic recheck: the push modified the finding's fingerprinted site, and the new run's lane does not re-raise a finding with the same identity. The site change is the deterministic half; the absent re-raise is the confirmation. A finding at an unmodified site never clears, however the lane's emissions vary between runs, so reviewer nondeterminism cannot silently clear a finding; a re-raise at the modified site keeps it open under Decision 14's identity rules. Scenario-gap findings keep their own recheck (the coverage query). Promoted from the §9 question queue. Decided 2026-08-13 by the owner.

## 4. Interfaces you expose

**Review guidelines.** Maintainers edit the per-lane guideline files under `.kiro/review/`; the auto-review bot reads them from the base branch only. The YAML frontmatter is the machine-read surface: the lane's kind vocabulary, with a blocking flag, a required agreement count, and an optional `escalate_lone` flag per kind. The markdown body is the reviewer-read surface, folded into the lane's prompt. Frontmatter field semantics are the contract; the review behavior of every PR changes with these files, so edits land through the normal PR path like any code change. Each kind in the frontmatter vocabulary has a matching body section under a heading anchored by the kind name, so a finding's kind resolves mechanically to the rule prose that defines it. Every inline comment links there, and the report step pins the anchor to the base-branch commit the run loaded during preflight: the linked text is exactly the rule that judged the finding, the finding is contestable on the rule's stated grounds, and guideline edits surface in review output rather than remaining invisible prompt tuning.

**Check run.** The gate publishes a single required check run named `auto-review` (a check name independent of the `kiro-review.yaml` workflow filename); its conclusion is the gate verdict for the PR's current head SHA, and branch protection pins that name to make the gate required. Beside it, the run publishes one advisory check per lane, named `auto-review / <lane>`: each lane check's conclusion states whether that lane currently contributes an open blocking finding, and its output summarizes the lane's findings, so a reader can tell which lane is unhappy without opening findings one by one. Lane checks never gate: branch protection pins only the bare `auto-review` name, the required check's conclusion is the only output that blocks a merge, and lane checks, labels, and comments project the same state without gating. Lane checks post and withhold together with the required check, and they are non-record-bearing by contract: the machine-readable finding record lives only in the check run named exactly `auto-review`, and the record reader ignores every other check a run publishes (Decision 20).

**Readiness labels.** `readiness: checking`, `readiness: passed`, `readiness: action required`; exactly one is present whenever a run has completed its gate evaluation, including on draft PRs, where the label posts while the check conclusion is withheld (§7); only the auto-review bot writes them, and a maintainer skip is the stated exception: a skipped PR carries the passing check and no label. A label on a draft reflects completed gate evaluation, never a published verdict (the conclusion is withheld, §7), so a consumer reading `readiness: passed` as mergeable must also read the PR's draft flag; a draft cannot merge regardless. Release tooling, dashboards, and on-call queries may key on these exact strings; renaming a label or adding one is a contract change.

**PR commands.** Comment commands parsed by the auto-review bot from comment creation events only (a command edited into an existing comment fires nothing): `/auto-review rerun` (write access) re-runs the pipeline for the current head SHA; `/auto-review skip` (maintainer only) records the SHA-scoped skip from §3 and itself fires the run that publishes the passing skip conclusion (a crashed skip run is retried with `/auto-review rerun`, which honors a recorded marker read from the pipeline's own finding record, never from any comment); `/auto-review dismiss` acts by tier: from a maintainer it dismisses the finding by fingerprint and records the reason to the lessons store; from the PR author it records a dismissal proposal instead, which changes nothing at the gate (the finding stays open and blocking; a proposal is record metadata, not a lifecycle state) but surfaces in the summary as awaiting a maintainer, whose confirming `/auto-review dismiss` completes it and adopts the proposal's rationale unless they state their own; from anyone else it is rejected. Addressing has two forms with identical tiers and effect: the reply form (`/auto-review dismiss <reason>` on the finding's inline thread) names the finding implicitly, and `/auto-review dismiss <fingerprint> <reason>` posted anywhere on the PR names it explicitly, which is what makes a watch-for finding, one with no inline comment, dismissable at all (§3 surfaces each watch-for entry's fingerprint for this use). Both the dismissal and the proposal take effect through a record-update run that never cancels an in-flight review (§7). The two tiers verify differently: `rerun` keeps today's author-association gate (owner, member, or collaborator in the event metadata), while `skip` and `dismiss` verify the commenter's repository permission (maintain or admin) through the permissions API, because author association alone cannot distinguish a maintainer from any other collaborator; a dismissal proposal verifies only that the commenter is the PR author, which is event metadata. These replace the existing surfaces in the same change: the `/review` comment command is retired in favor of `/auto-review rerun`, and the author-writable `[review:ignore]` body marker stops being honored; skip authority moves to the maintainer-only, SHA-scoped command.

**Finding record.** The per-PR finding state (the §3 finding shape plus its lifecycle status, any recorded skip, and any pending dismissal proposals) is published as a machine-readable block in the `auto-review` check-run output; the summary comment renders a human-readable projection of the same state and is never read back as state, because comment bodies are editable by write access (Decision 20). The store is not app-isolated (check runs created with the workflow token belong to the shared github-actions identity), so the reader verifies provenance rather than trusting the check name: a record is accepted only from a run of the base-branch pipeline workflow itself, and a record published under the pipeline's check name by any other workflow is rejected as foreign and treated as absent. Each record embeds the complete current state and the PR number it belongs to, so the reader needs only the newest reachable one: the bot's next run enumerates the pipeline's own prior runs newest-first, reading only each run's check named exactly `auto-review`, and takes the newest record whose embedded PR number matches; a matching record that cannot be parsed falls to the reset path rather than being skipped for an older one, so state never silently rolls back to a stale head. The record's embedded number is what keys a run to the PR: under `pull_request_target` a run's own head SHA is a base branch commit, and the Actions API's run-to-PR association is empty for fork PRs, so neither is usable. Run history retains the runs for every superseded head, so this reaches state recorded at the parent of an ordinary push and at every force-pushed-away head including the one the PR opened with, and skips past a cancelled or crashed predecessor. The enumeration is bounded rather than repo-wide: it reads a fixed window of the pipeline's most recent runs (the bound pinned by the owner at implementation from measured run traffic), and a PR whose newest record has aged out of the window falls to the stated reset path rather than extending the walk. That is how dismissals survive a squash force-push. When no prior record is reachable, or the newest matching one cannot be parsed, the run treats all findings as new instead of failing, and the summary carries the no-prior-state notice: no prior state was reachable and findings are reported as new. The notice holds for a first review and for aged-out state alike, which the reader cannot distinguish. Other agents may read either surface.

**Scenario stubs.** A stub posted on a confirmed gap always validates against `scenarios.schema.json`; a stub that fails validation is not posted. Authors adopt stubs unchanged through the normal PR path into the LLD 12 corpus.

## 5. Test strategy for this workstream

The deterministic shell is unit-tested directly; the model-driven lanes are tested through a scripted reviewer that stands in for real models, so the pipeline's behavior is asserted deterministically end to end.

**Unit tests (deterministic shell).** Table-driven tests over the pure functions in `scripts/auto-review/`: surface mapping (fixture diffs to expected surfaces); gate evaluation (findings plus gate policy to verdict, covering a blocking kind, a missed agreement count, and the scenario-gap structural confirmation); fingerprinting (the same finding across pushes hashes identically, shifted line numbers normalize away, distinct kinds at one location stay distinct); lifecycle reconciliation (recheck to `addressed`: a modified site with no same-identity re-raise clears, an unmodified site carries the finding open regardless of lane silence, a re-raise at a modified site stays open; fingerprint-scoped dismissal persistence; retraction on re-fingerprint); guideline frontmatter parsing (valid, invalid, and unknown-field cases; a kind with no matching body section fails validation); and finding-record round-trips (serialize, publish, parse back; an unparseable record reads as all-new findings).

**Pipeline tests (scripted reviewer).** The reviewer is an interface, and CI substitutes a scripted implementation that emits canned findings, making a full pipeline run deterministic. Fixture PRs assert: a planted blocking finding blocks the check and labels `action required`; a clean run passes and labels `passed`; two scripted reviewers emitting the same finding produce agreement 2 and a single comment; a fixture with more non-blocking findings than the comment cap posts exactly the cap's worth of inline comments with deterministic selection, lists each held-back finding's location, kind, and fingerprint in the summary, posts every blocking finding regardless, minimizes a comment displaced by the cap on a later run while its finding stays open in the record, and dismisses a held-back finding through the fingerprint form; a lone finding in an `escalate_lone` kind dispatches exactly one verification pass and blocks only when the scripted verifier confirms it; a crashing verification pass fails the check (fail-closed) instead of demoting the finding; a crashing reviewer job fails the check (fail-closed); each lane publishes its advisory check beside the required one, a lane contributing an open blocking finding shows it on its lane check, and a record block placed in a lane check's output is ignored by the next run's reader; a superseded run (a later start for the same PR, by a new push or a same-SHA re-run) discards its PR-level writes; a dismissal landing during an in-flight review updates the record without touching the projection and the review's published conclusion reflects both the push and the dismissal, while a dismissal with no review in flight republishes the conclusions and label immediately; skip, dismiss, and rerun commands enforce their permission gates, the fingerprint-addressed dismiss form included, which dismisses a watch-for finding that has no inline comment; label sync leaves exactly one readiness label after any gated transition and zero after a maintainer skip (remove-before-add asserted); the summary comment is edited in place across two pushes, never re-posted, and its delta line reports the reconciliation against the prior run (a cleared finding and a new finding count correctly); a draft push runs the lanes, posts inline comments and the readiness label, and publishes no check conclusion, required or lane; the ready-for-review transition publishes the conclusions for the current head; a fingerprint-scoped dismissal survives the PR's first squash force-push (the record recovered from the replaced head through the pipeline's own prior runs) and, when no prior record is reachable, the summary carries the no-prior-state notice; a retraction minimizes the stale comment as outdated rather than deleting it; a finding with no line anchor appears in the summary's watch-for list.

**Security regression tests.** A fixture PR that edits `.kiro/review/` is reviewed under the base branch's guidelines, never its own; a skip or dismiss from anyone who is neither a maintainer nor the PR author is rejected; a dismiss from the PR author records a proposal without moving the finding's state or the gate, and a maintainer's confirming dismiss completes it with the proposal's rationale; a skip recorded for one head clears nothing at a new head SHA (the next push is re-reviewed in full); an incomplete run never reports a passing check; each shipped reviewer agent definition carries no write or shell tool grant (asserted against the agent configs); a scripted injection fixture (a diff embedding instructions to write files or run commands) completes with zero repository or state writes; a scripted reviewer emitting a finding whose `location` is not diff-addressable has no inline comment or suggestion posted (the finding reports through the watch-for list with its suggestion dropped); a finding anchored to an unchanged context line in a hunk gets its inline comment but no suggestion block; and a valid finding's suggestion renders replacing exactly its anchored span; a tampered projection (a skip and a dismissal edited into the summary comment) changes nothing, because runs read state only from the pipeline's own check-run record; and a record published under the pipeline's check name by a different workflow is rejected as foreign and treated as absent.

**Live recall check (scheduled, advisory).** A nightly job replays a small fixed set of seeded-defect PRs (drawn from real historical bugs) against the real lanes and reports the catch rate; it watches for prompt and model drift and never blocks merges.

The blocking path is the tested path: every fixture set includes at least one case that must fail, so a pipeline that stops detecting cannot pass its own tests.

## 6. Machine-checkable acceptance criteria

1. `bun test scripts/auto-review` exits 0; the suite contains the §5 fixtures, including at least one must-fail case per lane and the fail-closed case for a crashed reviewer job.
2. `scripts/auto-review/validate-guidelines` exits 0 against the shipped `.kiro/review/` files and exits 1 against the malformed fixture (missing frontmatter, unknown field, kind without a blocking flag).
3. `scripts/auto-review/gate --replay fixtures/run-01` exits 0 and prints the expected verdict; two consecutive runs produce byte-identical output.
4. `scripts/auto-review/pipeline --fixture seeded-injection --reviewer scripted` exits nonzero and reports the verdict `action required`; the same command with `--fixture clean` exits 0 and reports `passed`.
5. `scripts/auto-review/pipeline --fixture uncovered-surface --reviewer scripted` exits 0 and writes a scenario stub to `out/stub.json`, and `scripts/auto-review/validate-stub out/stub.json` exits 0 (schema validation against `scenarios.schema.json` at its §2 path; the criterion runs today and becomes contractual once F1 freezes the schema).
6. `scripts/auto-review/pipeline --fixture edits-guidelines --reviewer scripted` exits 0, asserting the run used the base branch's guidelines and the PR's edited guidelines had no effect.
7. `grep -qE '^  recall:' .github/workflows/review-recall.yaml && grep -q 'schedule:' .github/workflows/review-recall.yaml` exits 0: the advisory nightly recall workflow ships with the change, defines the `recall` job key, and is schedule-triggered; renaming the job or dropping the schedule breaks this criterion.
8. `scripts/auto-review/validate-agents` exits 0 against the shipped reviewer agent definitions, asserting each carries no write or shell tool grant, and exits 1 against a fixture agent definition that grants a write tool.
9. `.kiro/skills/local-auto-review/` run with the scripted reviewer substituted exits nonzero and prints the blocking finding against a fixture tree containing one, and exits zero against the clean fixture, with zero network writes in both cases.

## 7. Rollout and gating

**Attach points.** The pipeline runs on the PR gate: PR opened, reopened, pushed to, marked ready for review, or explicitly re-run (`/auto-review rerun`); reopening without a push re-fires the run so the required check never sits stale at a reopened head. It keeps the existing workflow's `pull_request_target` trigger (Decision 19), so fork PRs are reviewed the same way they are today. Draft PRs are reviewed on every push exactly like ready PRs; what a draft run withholds is check publication: it posts inline comments, the summary, and the readiness label, and publishes no `auto-review` conclusion and no lane checks, so the required check stays pending rather than reading as passed (a posted neutral conclusion would count as passing under branch protection). The withholding exists because of the conclusion's gating effect, not the review itself, so draft authors keep every-push feedback, matching the current workflow's draft cadence. The ready-for-review transition triggers a run that publishes the conclusions for the current head, so no PR presents a mergeable state without a gated verdict; fail-closed (Decision 6) governs ready PRs, and a draft head has no conclusion to fail. A maintainer skip publishes its passing conclusion on any head, draft included: the command is explicit, and a draft cannot merge regardless. The advisory recall job (§5) runs nightly and never gates. Nothing attaches to RC or release.

**Blocking rollout.** Three phases:

1. Advisory: the check publishes real conclusions but branch protection does not require it; comments and labels flow. The dismissal record accumulates per-kind false-positive data.
2. Narrow blocking: branch protection requires `auto-review`; the shipped gate policy marks only the highest-precision kinds blocking (initial set: the security lane's injection and secret-handling kinds, agreement 1).
3. Steady state: kinds are promoted to or demoted from blocking through guideline frontmatter PRs, informed by each kind's measured dismissal rate.

The single-reviewer path is replaced, not run alongside: the change that lands the lanes retires the existing reviewer invocation and its verdict-carrying reviews in the same commit, so no PR ever carries both the old review and the new pipeline's output. PRs already open at the cutover need one extra step, because the old path was also what dismissed its own stale reviews once findings cleared: the cutover change runs a one-time sweep over open PRs that dismisses every open review the old bot identity left behind (with a cutover notice as the dismissal reason) and posts one notice comment on each affected PR naming the change and where the verdict now lives (the check run, the readiness label, and the summary comment), and the new pipeline's first run on a PR repeats the dismissal as a backstop for any PR the sweep misses (one reopened after the sweep, for example), so no PR is stranded behind a verdict nothing can retract, and none waits for its next trigger to shed a stale one. The sweep reports any PR whose stale review it could not dismiss, so the residue is handled by hand instead of discovered later. The verdict surface changes with it: in phase 1 the human-visible Requires Changes review objects disappear in favor of an advisory check run and the summary comment; nothing is lost at the gate (those reviews never moved the merge decision), but the reviewers box empties, and that is the single most visible change for readers who will never open this document. Through phase 1 the summary comment carries a one-line footer pointing at the check run and label as the verdict surface, so the question of whether the review bot broke is answered on the PR itself.

**Escape hatches.** In increasing blast radius: a maintainer dismisses the finding (fingerprint-scoped, immediate, nothing deploys); a maintainer skips the review for the PR head (`/auto-review skip`, SHA-scoped, publishing the passing skip conclusion of Decision 18 so the required check clears); a guideline PR flips the kind's blocking flag (reviewed under base-branch guidelines; if the misfiring kind blocks even that PR, the skip applies); repo admins unrequire the check in branch protection (break-glass). The 2am path is the first two: each is a single action on the PR itself. An author cannot climb any rung alone, but can pre-stage the first: a dismissal proposal (§4) records the rationale so the maintainer's confirmation is one reply.

**Cost and latency controls.** One concurrency group per PR, retained from the existing workflow, which already cancels an in-flight run when a new push supersedes it; the §3 stale-run write guard remains as the backstop for races. Command-triggered runs split by what they do: `rerun` and `skip` produce a fresh verdict for the head, so they join the same per-PR concurrency group and supersede like any newer start; `dismiss` (and its proposal variant) fires a lightweight record-update run instead, no model calls, which applies the state change and always writes the record, and recomputes and republishes the projection (summary, conclusions, label) only under the §3 publish-time guard (no review run for the head in flight or newer, and its record naming the current head), so a recompute from stale findings can never overwrite a newer verdict. On a draft it withholds conclusions exactly as review runs do. With no review in flight the dismissal's effect is immediate, which is what keeps it the first escape-hatch rung; with one in flight the record write lands and that review's report step folds it in. Record-update runs serialize in their own per-PR group and never cancel the review group, so the command most likely to be typed mid-review cannot kill the review it replies to. A comment event that parses to no command never enters any group, so a mere reply cannot cancel an in-flight review. Every-push full re-review replaces the existing workflow's ten-minute debounce and incremental-comment mode, and the tradeoff is stated plainly: cancellation bounds concurrency, not spend (calls already dispatched are not refunded), so worst-case rapid-push cost rises versus today's debounced single reviewer. Draft pushes spend full runs like ready pushes, and the advisory phase's per-PR measurement includes them. The advisory phase measures per-PR calls, cost, and time to first signal (p50 and p95 wall-clock from the triggering event to the first posted finding surface, and to the published conclusion), and the owner pins a per-PR cost ceiling and a time-to-first-signal target from that data before phase 2: cost and latency are the two go/no-go inputs for making the check required, since the latency number is what decides whether authors wait for the bot or merge around it. If measured cost exceeds the ceiling, a per-window budget (at most one full run per PR per ten-minute window) returns before phase 2. A full run is roughly fourteen model calls (three lanes at two reviewers times two calls, plus the scenario-coverage lane's two single-call filters), plus one verification call per escalated lone finding.

## 8. Risks

**False-positive fatigue.** Wrong blocking findings train authors to dismiss reflexively and maintainers to skip, and the bot's signal dies. Mitigated by the falsification protocol (unsure findings are never posted), the advisory phase measuring per-kind dismissal rates before anything blocks, the narrow initial blocking set, dismissal being a single fingerprint-scoped action, and the rule link every comment carries (§4), which makes a disputed finding arguable on the rule's stated grounds. The dismissal record keeps fatigue measurable: a kind whose dismissal rate climbs is demoted by a one-line frontmatter PR.

**Agreement undercounting.** Two reviewers can find the same defect and still fingerprint apart (different kind binning, shifted location), reporting agreement 1 where 2 is true; kinds that require agreement 2 then under-block. Mitigated by coarse kind vocabularies (few, broad categories bin more consistently), location normalization at hunk granularity, the nightly recall job, which surfaces systematic under-blocking as a falling catch rate, and the local review skill run across historical PRs (§3), which measures binning consistency before anything gates; critical kinds can set their required agreement to 1 or mark `escalate_lone` so a lone finding earns verification instead of silence.

**F1 schema drift.** The scenario-coverage lane consumes a schema that is not yet approved (§2); if the tag vocabulary or scenario shape moves after this ships, coverage queries and stubs break. Mitigated by consuming F1 through its own tooling instead of hand-parsing it: stub validation reads the live schema and coverage queries go through the corpus's query path, so drift surfaces as failing §6 checks rather than silent misses.

A late landing stalls nothing: the existing single-reviewer bot keeps running unchanged, and no other workstream consumes these interfaces.

## 9. Open questions

1. Lessons store shape and scope. Format and location are undecided, and so is suppression scope: whether a dismissal suppresses re-raising per PR or repo-wide. A repo-wide wrong lesson silences a real finding class at that site. Default until decided: per-PR scope. Decider: owner.
2. Workflow topology. One extended `kiro-review.yaml` (as designed) versus one workflow per lane plus an aggregator, which buys per-lane required checks and independent retries (advisory per-lane checks ship in either topology, §4; the split's residual benefit is making them required and independently retryable). Default: single workflow; if it is ever split, every lane workflow must share the per-PR concurrency group string or push cancellation breaks. Decider: owner with program lead.
3. Suggested human reviewers. The single-reviewer bot already suggests reviewers from review context and its memory; the open question is whether the multi-lane summary carries that mechanism forward or replaces it with a maintained surface-to-owner mapping. Default: carry the existing mechanism forward unchanged. Decider: program lead.
4. Semantic-merge checking. Evidence from KiRoom's review system shows clean merges with green builds and broken combined behavior; a merge-result check reviews the merged tree rather than the diff, so it is a new lane or a separate workstream. Decider: program lead.
5. Existing-comment awareness. Reviewers that see open human discussion would avoid duplicate feedback, but PR comments are writable by any participant and become model input; this requires the untrusted-evidence treatment first. Default: not included. Decider: owner.
6. Promotion evidence. §7 phase 3 promotes kinds to blocking on measured dismissal rate without pinning the threshold or sample size. Decider: program lead with the team.
7. Surface mapping. Preflight maps the diff to changed surfaces, but what a surface is (granularity) and how a diff hunk becomes one (path patterns to named surfaces, or inference from F1 tags) are undefined. Default: a maintained path-glob to named-surface table in `scripts/auto-review/`. Decider: owner, before implementation.

## 10. AI-native notes

**Entry points.** `.github/workflows/kiro-review.yaml` (triggers, lane fan-out, gate step, label sync); `.github/workflows/review-recall.yaml` (the advisory nightly recall job); `scripts/auto-review/` (surface mapping, fingerprinting, gate evaluation, pipeline and validation CLIs); `.kiro/review/` (per-lane guidelines: gate policy in frontmatter, reviewer prose in the body); `.kiro/skills/local-auto-review/` (the local runner over the same lanes and gate; §3); `.kiro/agents/` (per-lane agent definitions and prompt scaffolds); `.kiro/skills/semantic-pr-reviewer/` (the review methodology the agents delegate to).

**Invariants that are easy to break.**

- The gate never interprets prose: blocking derives only from frontmatter policy plus agreement or verification.
- Review guidelines load from the base branch only; loading anything review-related from the PR head reopens the self-review hole.
- The fingerprint excludes the message; hashing message text breaks dedup, dismissals, and agreement counting at once.
- An incomplete run fails the check, and a maintainer skip publishes a passing conclusion; no code path may let a crashed lane read as success, and no path other than the skip command may publish success without a completed run.
- Only the newest review run for the PR writes PR-level state, and a record-update run touches the projection only under its publish-time guard (no review run in flight or newer, record naming the current head); the two guards together keep superseded or stale runs from clobbering the verdict and labels.
- State is read only from the pipeline's own check-run record, provenance-verified (the shared github-actions identity means the check name alone proves nothing), and only from the check named exactly `auto-review`; lane checks are non-record-bearing; the summary comment is a projection, and parsing state out of any comment body reopens the write-tier forgery hole of the retired body marker.
- Reviewer calls are read-only, enforced by the per-lane tool allowlists; adding build or test execution to a reviewer, or widening an allowlist, reintroduces environmental failures and the injection-to-mutation path.
- Exactly one readiness label whenever a run completes its gate evaluation, including on drafts where the conclusion is withheld, and none after a maintainer skip; sync removes before it adds.
- A lone finding in an `escalate_lone` kind gets exactly one verification pass; survival substitutes for agreement, and silence never discards it.
- Confidence is never numeric: comments render agreement only as the deterministic provenance wording.
- A stub that fails schema validation is never posted.

**Verify commands.** Run the §6 list end to end; the short loop is `bun test scripts/auto-review`, then `scripts/auto-review/gate --replay fixtures/run-01`, then `scripts/testing-cert-status` for doc consistency.

Durable items here graduate to a steering file once implementation lands (see `19-lld-agents-steering-renewal.md`).
