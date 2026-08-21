---
name: contribute
description: Route incoming contributions to kiro-cli. Use when a user wants to file a bug, request a feature, or contribute a fix. Handles classification, deduplication, Taskei task creation, and routing through three paths based on issue complexity.
---

# Contribution Routing Skill

Route incoming bugs, feature requests, and contribution proposals for kiro-cli through the correct path. This skill handles two user types:

1. **Kiro users** — filing a bug or feature request
2. **Kiro contributors** — wanting to implement a fix or feature

## Current Phase

This skill implements Phase 1 (Foundation) of the delegated contribution model. Community Reviewers, AutoSDE Reviewer, and automated dedupe enforcement are not yet live. All routing is manual through this agent.

## Dry-Run Mode

If the user passes `--dry-run` or says "dry run" / "test mode", execute the full workflow but **do NOT create any tickets**. Instead:
- Run all dedup searches normally (read operations are safe)
- Classify normally
- At Step 4, instead of calling TaskeiCreateTask, output the complete task payload as formatted text: name, room, folder, type, tags, description with all fields filled in
- Prefix the Step 5 summary with `🧪 DRY RUN —` and show what would have been created
- This lets users and testers validate the full flow without side effects

## Step 0: Prerequisites Check

Before starting the flow, verify tooling is available. Run these checks silently and report any issues to the user upfront:

1. **builder-mcp**: Try a lightweight TaskeiListTasks call. If it fails, warn the user: "builder-mcp is unavailable — I can walk you through the flow but won't be able to create tasks. I'll give you the task details as copyable text instead."
2. **gh CLI**: Run `gh auth status` (allowed by shell config). If it fails, warn: "GitHub CLI is not authenticated — I won't be able to check for duplicate GitHub issues or open PRs. Run `gh auth login` or set `GH_TOKEN` to enable this. I'll proceed with Taskei-only dedup."

Report both results before proceeding. If both are unavailable, the agent can still classify and provide ticket text — just can't create anything or dedup against GitHub.

## Step 0b: Quick Check

- **If the user already has a PR**: Skip to Step 2 (dedup). After classification, for Path A they can submit directly. For Path B/C, create the task and link the PR in it. Tell them: "Link the Taskei task in your PR description — CI validates the reference."
- **If none of the above apply**, proceed to Step 1.

## Step 1: Gather Information

Ask the user to describe their issue or idea. You need:

- **Type**: bug report, feature request, or contribution proposal
- **Summary**: one-line description
- **Details**: for bugs — repro steps, expected vs actual behavior, environment; for features — use case, proposed behavior; for contributions — what they want to change and why

Extract what you can from the user's initial message. Do NOT ask for information they already provided. Ask only for what's missing, one or two questions at a time. Infer type and severity when obvious.

Briefly acknowledge the user's experience before proceeding. For bugs: empathize with the impact. For contributions: thank them for wanting to help.

For bugs, check the user's CLI version (`kiro-cli --version` if available) — the bug may already be fixed in a newer release.

Do NOT classify until you have enough detail to distinguish between Path A, Path B, and Path C.

## Step 2: Deduplicate (MANDATORY — do this before creating any ticket)

Tell the user: "Let me check if this is already tracked..." before running searches.

Search for existing tickets in this order:

### 2a. Search the Taskei intake room (source of truth)

Intake routing is defined by the Kiro Labs wikis (authoritative: https://w.amazon.com/bin/view/KiroLabs/Contribute and https://wiki.amazon.com/bin/view/KiroLabs/Backlog). Kiro CLI bugs and feature requests live in Taskei room `7c221a81-7ca7-436c-8f05-a7278949341b` — bugs in folder `0205a00e-4757-425d-bde0-e06884dce83e`, feature requests in folder `b710f4b4-3f53-4624-9b50-c239472dcf84`. Use builder-mcp TaskeiListTasks against the room (search the whole room, not just one folder, so cross-filed duplicates surface). NOTE: the tool takes STRUCTURED parameters and silently ignores unknown ones — there is no `filter` query string. Search per keyword (3-5 variants):

```
@builder-mcp/TaskeiListTasks
roomId: "7c221a81-7ca7-436c-8f05-a7278949341b"
name: { queryOperator: "contains", value: "KEYWORD" }
status: "ALL"
pagination: { maxResults: 100 }
```

Use `status: "ALL"` so closed duplicates surface. Legacy tickets may also exist in the SIM CTI queue (`Kiro / CLI / Intake`, resolver `kiro-core`) — a TicketingReadActions full-text pass there is a nice-to-have, not a gate.

If TaskeiListTasks fails (auth error, service unavailable), do NOT silently proceed. Tell the user what happened and give them options:
1. "Fix the issue and try again" (e.g., re-authenticate, check network)
2. "Search manually and confirm no duplicates" — ask the user to check issues.amazon.com themselves and confirm there are no existing tickets before proceeding

Do NOT proceed to task creation until BOTH of these are confirmed:
1. **No duplicate task in the intake room** — either via successful TaskeiListTasks search, or user manually confirms they checked the room at taskei.amazon.dev
2. **No existing PR already fixing this** — either via successful `gh pr list` search, or user manually confirms they checked open PRs on kiro-team/kiro-cli

GitHub issues search is secondary — skip it if `gh` failed, since the intake room is the source of truth.

### 2b. Search GitHub issues AND PRs on kiro-team/kiro-cli

Use the `gh` CLI. The user must have `GH_TOKEN` or `GITHUB_TOKEN` set:

```bash
gh api "repos/kiro-team/kiro-cli/issues?state=open&per_page=100" --jq '.[] | select(.pull_request == null) | {number, title, labels: [.labels[].name]}'
```

For targeted searches (try 3-5 keyword variations — rephrase using alternative terminology to catch semantic duplicates):

```bash
gh api "search/issues?q=repo:kiro-team/kiro-cli+is:issue+is:open+KEYWORDS&per_page=30" --jq '.items[] | {number, title, html_url}'
```

Also search open PRs — someone may already be fixing this:

```bash
gh pr list --repo kiro-team/kiro-cli --state open --search "KEYWORDS" --json number,title,url --limit 20
```

If `gh` fails with auth errors, see Step 0 for recovery.

### 2c. Evaluate matches

When both Taskei and GitHub have matches, present the Taskei task as the canonical tracker. Mention the GitHub issue as additional context but direct the user to interact with the Taskei task.

Classify each result into one of these tiers:

**Tier 1 — Exact duplicate** (same root cause and symptom):
- Do NOT create a new ticket
- Show the user the existing task
- Ask if they want to add context (use TaskeiUpdateTask `postCommentMessage` on the task, or `gh issue comment` for GitHub)
- If the user disagrees ("that's not the same issue"), proceed with ticket creation and add a cross-reference to the original in the description

**Tier 2 — Likely related** (same component or overlapping symptoms):
- Show to the user and ask: "Is this the same issue you're reporting?"
- If they say no, proceed with creation and add a cross-reference
- If they say yes, treat as Tier 1

**Tier 3 — Tangentially related** (same area but different issue):
- Proceed with creation, mention the related ticket in the description

**PR matches**: If an open PR already addresses the issue, tell the user: "PR #N is already in progress to fix this. You can watch it or test the branch." Include the PR link in any summary. If the user's issue is slightly different, proceed with ticket creation and cross-reference the PR.

### Compound failure

If both the room search and `gh pr list` fail, do NOT proceed to task creation. Tell the user: "I couldn't verify there are no duplicate tasks or existing PRs. Before I create a task, I need you to either:
1. Fix the connectivity/auth issue so I can retry
2. Manually confirm both: (a) no existing task in the intake room at taskei.amazon.dev, and (b) no open PR already fixing this at github.com/kiro-team/kiro-cli"

Only proceed after the user confirms both.

### Important

- The `kirodotdev/Kiro` public repo is OUT OF SCOPE — ignore it entirely
- The Taskei intake room is the source of truth, GitHub is secondary
- Distinguish between "search returned zero results" (good — proceed) and "search failed" (bad — dedup was not performed, warn the user)

## Step 3: Classify into Path

Based on the information gathered, classify into one of three paths. **Tell the user which path you chose and why** — e.g., "Because this touches the MCP protocol layer, it needs team review before implementation (Path B)."

If the user disagrees with the classification, discuss their reasoning. They may have context you don't.

### Path A — Straightforward Bug

Criteria (ALL must be true):
- Clear reproduction steps exist
- The fix is obvious or narrowly scoped (e.g., typo, off-by-one, missing null check, wrong error message)
- Does NOT touch core architecture (agent config loading, MCP protocol, ACP, tool execution pipeline)
- Does NOT change user-facing behavior beyond fixing the bug
- Does NOT require product/UX input
- Fix is contained to 1-3 files and doesn't touch shared utilities called from many places

Action:
1. Skip issue sign-off — no need to wait for team review
2. Create a Taskei task (see Step 4) if not already tracked
3. **Contributor conversion**: If the user filed the bug (not already planning to contribute), prompt them: "This looks like a quick fix — most Path A bugs take under an hour. Would you like to submit the PR yourself? See CONTRIBUTING.md for setup. PRs are merged on the weekly Wednesday cadence."

### Path B — Ambiguous Bug/Fix

Criteria (ANY triggers Path B):
- Bug is real but the root cause is unclear
- Multiple valid fix approaches exist
- Fix touches architecture (agent loading, MCP/ACP protocol, tool trust, TUI rendering pipeline)
- Fix could have side effects on other features
- Requires design discussion

Action:
1. Create a Taskei task with the proposed solution documented in the description
2. Tell the user: "This bug needs team review. Document your proposed solution in the Taskei task — the team reviews these at their weekly Wednesday meeting. Expect feedback within a week."
3. **Contributor conversion**: "If you're interested in contributing, document your proposed approach in the Taskei task. Once the team gives feedback, you can start coding. Watch the task for updates."
4. If the contributor has strong conviction about the approach, suggest they document it thoroughly in the task to accelerate the review. But do NOT greenlight coding before team feedback.

### Path C — Feature Request

Criteria (ANY triggers Path C):
- New user-facing command, flag, or capability
- Changes existing user-facing behavior (not a bug fix)
- UX/UI changes
- New architectural pattern or dependency
- Roadmap impact (new integration, new protocol support, etc.)

Action:
1. Create a Taskei task in the same intake room (tag `feature-request`) for Monday Office Hours review
2. Tell the user: "Feature requests go through product review at Monday Office Hours via the intake room. Watch the Taskei task for status updates."
3. **Warning**: "Features need product approval before implementation starts. Please wait for feedback before coding."
4. **Contributor conversion**: "If this gets approved, would you be interested in implementing it? We can note that on the ticket so the team knows there's a volunteer."

### Classification Edge Cases

- **Performance improvements** without behavior change → Path A if isolated, Path B if systemic
- **Refactors** → Path B (always needs team alignment)
- **Documentation fixes** → Path A (straightforward)
- **Dependency updates** → Path A if security patch, Path B if major version bump
- **Test additions** → Path A
- **Uncertain if bug or intended behavior** → Path B (create the task for the team to clarify)
- **Bug fix that introduces new configuration** → Path B if the config is an operational knob (e.g., timeout value). Path C if it creates a new user-facing workflow or significantly changes how users interact with the tool.

## Step 4: Create Tickets

If in dry-run mode, skip ticket creation and output the complete ticket payload as formatted text instead. Prefix with "🧪 DRY RUN — this ticket would be created:".

If ticket creation fails, show the user the error and provide the pre-filled ticket details as copyable text so they can create it manually. Ticket creation failure does not block PR submission — tell the user to create the ticket manually, then proceed with Step 5 guidance.

### Taskei Task Creation (Path A and Path B)

Use builder-mcp TaskeiCreateTask with:

- **roomId**: `7c221a81-7ca7-436c-8f05-a7278949341b` (the Kiro CLI intake room)
- **folder**: `0205a00e-4757-425d-bde0-e06884dce83e` (Bug Reports). CAVEAT (verified 2026-08): the API silently drops `folder` on create for non-resolvers (create succeeds, no warning) and rejects folder moves — and the team's board views filter by folder. After creating, verify placement with TaskeiGetTask. If the folder was dropped, give the user the wiki's pre-foldered create link as the fallback (https://taskei.amazon.dev/tasks/create?room=7c221a81-7ca7-436c-8f05-a7278949341b&folder=0205a00e-4757-425d-bde0-e06884dce83e&type=TASK) or ask in the room/Slack for a resolver to folder it, and note the placement gap in your summary rather than claiming board visibility
- **type**: `TASK` (matches the wiki's create template)
- **tags**: `straightforward-bug` for Path A, `needs-team-review` for Path B (REQUIRED — do not omit; Taskei tags are free-form strings)
- **name**: Clear, concise summary
- **Description** must include:
  - **Summary**: one paragraph
  - **Repro steps** (for bugs): numbered steps to reproduce
  - **Expected behavior**: what should happen
  - **Actual behavior**: what happens instead
  - **Environment**: OS, CLI version, relevant config
  - **Proposed solution** (Path B only): describe the approach and alternatives considered
  - **Existing code context**: relevant files, functions, or modules (use `code` and `grep` tools to find these — don't guess)
  - **Related work**: any existing tickets, PRs, or prior art found during dedup
  - **PR** (if exists): link to the PR
  - **Classification**: "Path A — Straightforward Bug" or "Path B — Ambiguous Bug"
  - **Volunteer** (REQUIRED if user wants to contribute): "Contributor [alias] has volunteered to implement once approach is approved"

**Path A ownership**: After creating the task, ask the user: "Would you like to take ownership of this fix and set the task to In Progress?" If yes, use TaskeiUpdateTask to assign it to the user and move its workflow step (ask for their alias if not known).

**Path B volunteer**: After creating the task, if the user expressed interest in contributing, ask for their alias and add it to the task. Do NOT assign or set In Progress — the team needs to approve the approach first.

### Taskei Task Creation (Path C)

Use builder-mcp TaskeiCreateTask in the same intake room (`7c221a81-7ca7-436c-8f05-a7278949341b`):

- **folder**: `b710f4b4-3f53-4624-9b50-c239472dcf84` (Feature Requests). Same caveat as Path A/B: the API may drop `folder` on create for non-resolvers — verify placement with TaskeiGetTask, and fall back to the wiki's pre-foldered create link (https://taskei.amazon.dev/tasks/create?room=7c221a81-7ca7-436c-8f05-a7278949341b&folder=b710f4b4-3f53-4624-9b50-c239472dcf84&type=TASK) if it was dropped
- **name**: Feature request summary
- **type**: `TASK` (matches the wiki's create template)
- **tags**: `feature-request` (REQUIRED — do not omit)
- **Description** must include:
  - **Use case**: why the user wants this — the problem they're solving
  - **Proposed behavior**: what it should do, concretely enough that someone could implement from this description
  - **Acceptance criteria**: 2-4 bullet points defining what "done" looks like
  - **Existing code context**: relevant files, functions, or modules in the codebase that this would touch or extend (use `code` and `grep` tools to find these — don't guess)
  - **Related work**: any existing tickets, PRs, or prior art found during dedup
  - **Alternatives considered**: other approaches (if not provided by user, write "Not specified")
  - **Scope**: what's affected, rough size estimate (small/medium/large)
  - **Implementation path**: high-level approach if the user has one (if not provided, write "TBD — pending product review")
  - **Classification**: "Path C — Feature Request"
  - **Volunteer** (REQUIRED if user wants to contribute): "Contributor [alias] has volunteered to implement if approved"

After the contributor conversion prompt, if the user says yes, ask for their alias and add it to the ticket. Do NOT assign or set In Progress — product approval is required first.

Before submitting any ticket, show the user the ticket description: "Here's what I'll file — does this look right?" Let them adjust before creation.

## Step 5: Summarize and Next Steps

If in dry-run mode, prefix all summaries with `🧪 DRY RUN —`.

If the user already has a PR, reference it by number instead of suggesting they submit one.

**For Path A:**
> ✅ Created Taskei task [TASK-ID] (https://taskei.amazon.dev/tasks/[TASK-ID]). This is a straightforward bug — you can submit a PR directly to `kiro-team/kiro-cli`. Link this task in your PR description — CI validates the reference. PRs are merged on the weekly Wednesday cadence.

**For Path B:**
> 📋 Created Taskei task [TASK-ID] with proposed solution. The team reviews these at their weekly Wednesday meeting — expect feedback within a week. Watch the task for updates. Once approved, link the task in your PR.

**For Path C:**
> 📝 Created Taskei task [TASK-ID]. This feature request will be reviewed at Monday Office Hours. Product approval is required before implementation can begin. Watch the task for status updates.

**For duplicates:**
> 🔗 This is already tracked in [TICKET-ID] ([link]). Added your context to the existing ticket.

## Out of Scope

Do NOT handle these — redirect the user elsewhere:

- **Contributor workspace setup/onboarding** — point to CONTRIBUTING.md
- **Public repo (`kirodotdev/Kiro`) interaction** — this skill only covers `kiro-team/kiro-cli`. If the user is an external contributor without internal access, tell them: "File your issue on `kirodotdev/Kiro` per the public CONTRIBUTING.md. This agent handles internal routing only."
- **Helping users get access to `kiro-team/kiro-cli`** — point to the team wiki or #kiro-cli-contributors Slack
- **Oncall/sev2 ticket triage** — use the `oncall` agent instead
- **PR review or code review** — use `@pr-triage` prompt instead
- **Emergency/hotfix bypass** — not yet defined. For urgent production issues, use the `oncall` agent
