---
name: publish-pr-review
description: Publish a semantic review as one GitHub review. Supports full mode (concise parent body + inline) and incremental mode (new findings only). Supports REQUEST_CHANGES verdict for P1/P2 findings.
---

# Publish Semantic Findings Inline

Use this skill after `semantic-pr-reviewer` writes a Markdown review and its adjacent `*.publish.json` manifest. This skill is instructions only: it does not require workflow changes, helper scripts, or a custom result contract.

## Inputs

The manifest supplies:

- `repository`, `pull_number`, and `reviewed_head_sha`
- `review_path`, which must be the adjacent Markdown file with the same basename
- `inline_findings` for actionable concerns with meaningful changed-line anchors
- `summary_findings` for actionable concerns that cannot be anchored to one changed line

Each finding ID must be unique and match `^[a-z0-9][a-z0-9-]{0,63}$`.

Each finding must include a `priority` field: `P1`, `P2`, `P3`, or `P4`.

Map each finding to a merge-impact label before publishing:

- **Blocking**: confirmed `P1` or confirmed `P2`
- **Non-blocking**: `P2` (likely) and all `P3`/`P4`

## Iteration Mode

The environment variable `REVIEW_MODE` determines the publish behavior:

- **`full`** (default, first review): Post a concise parent body with all inline findings
- **`incremental`** (subsequent reviews): Post only NEW findings not already raised, with a brief summary body

`REVIEW_ITERATION` is the number of prior bot reviews on the PR. Treat `REVIEW_ITERATION >= 2` as a third-or-later review pass.

## 1. Validate and deduplicate

Before any GitHub write:

1. Fetch the PR with `gh pr view` and require it to be open with a current head equal to `reviewed_head_sha`. Stop without posting if the head moved.
2. Fetch existing review bodies, inline comments, and issue comments. Suppress a finding if its hidden marker exists or any existing reviewer already raised the same concern. Markerless top-level comments count: compare normalized titles and the combination of path/code token plus failure mode or remediation.
3. Fetch the current diff. For every inline finding, prove that `line` and optional `start_line` are right-side added or modified lines in the same hunk. Keep an unanchorable concern in `summary_findings`; never attach it to an arbitrary line.
4. Do not publish findings below `likely` confidence.

**Incremental mode additional checks:**
5. Compare each finding against prior bot inline comments on this PR. A finding is a DUPLICATE if an existing bot comment targets the same `path` and addresses the same concern (normalized title + code token match).
6. Identify RESOLVED findings: prior bot comments where the targeted line has been modified or removed in the current diff. List these in the summary body.
7. Check whether the bot has a prior `REQUEST_CHANGES` review on this PR (set `HAS_PRIOR_REQUEST_CHANGES=true`). This determines whether an `APPROVE` is required to dismiss the stale block.

Before choosing the review event, build:

- `NEW_FINDINGS`: findings left after deduplication in this pass
- `UNRESOLVED_FINDINGS`: `NEW_FINDINGS` plus every prior bot finding whose target line is still unchanged in the current diff
- `BLOCKING_FINDINGS`: confirmed P1/P2 entries inside `UNRESOLVED_FINDINGS`

If no new inline or summary finding remains:
- **Full mode:** exit cleanly without creating a review.
- **Incremental mode:** if all prior findings are resolved and `HAS_PRIOR_REQUEST_CHANGES` is true, dismiss the prior review and post a `COMMENT` confirming resolution. Otherwise exit cleanly.

## 2. Determine review event

Evaluate these rules top-down against `UNRESOLVED_FINDINGS`:

| Condition | Event |
|-----------|-------|
| Any blocking finding remains unresolved on the PR | `REQUEST_CHANGES` |
| Every unresolved finding is non-blocking and `HAS_PRIOR_REQUEST_CHANGES` is true | `COMMENT` + dismiss prior review |
| Every unresolved finding is non-blocking | `COMMENT` |
| No unresolved findings remain and `HAS_PRIOR_REQUEST_CHANGES` is true | `COMMENT` + dismiss prior review |
| No unresolved findings remain | skip (no review needed) |

### Dismissing a prior REQUEST_CHANGES

When the bot previously posted `REQUEST_CHANGES` and blocking findings are now resolved, the bot MUST dismiss its own stale review. GitHub Actions tokens cannot submit `APPROVE` reviews due to branch protection, so use the dismiss endpoint instead:

```bash
# Find the bot's latest REQUEST_CHANGES review ID
REVIEW_ID=$(gh api "repos/$REPOSITORY/pulls/$PULL_NUMBER/reviews" \
  --jq '[.[] | select(.user.login == "github-actions[bot]" and .state == "CHANGES_REQUESTED")] | last | .id')

# Dismiss it
if [ -n "$REVIEW_ID" ] && [ "$REVIEW_ID" != "null" ]; then
  gh api --method PUT "repos/$REPOSITORY/pulls/$PULL_NUMBER/reviews/$REVIEW_ID/dismissals" \
    -f message="Blocking findings resolved. Remaining items are non-blocking." \
    -f event="DISMISS"
fi
```

This clears the "Changes Requested" status from the PR without needing APPROVE permission.

## 3. Render the review

### Full mode

The parent body starts with the canonical signature and stays short. Synthesize it from `review_path`; do not paste the full review body.

```markdown
**AI Generated - Semantic Reviewer**

**Description**
1-2 lines on what the PR changes and the overall risk.

**Recommendation**
Request changes. 2 blocking findings remain.

**Watch For**
- [Blocking] Short finding title
- [Non-blocking] Short follow-up title

**Memory Context**
- Related PR or known pattern relevant to this diff
- If none: No prior patterns flagged for this area.

**Suggested Reviewers**
- reviewer1
- reviewer2

<!-- robertobot:<summary-finding-id> -->
```

Append one hidden summary marker for each new summary finding.

### Incremental mode

The parent body is a brief summary:

```markdown
**AI Generated - Semantic Reviewer**

**Description**
1-2 lines on what changed in this pass and what remains open.

**Recommendation**
Request changes. Blocking findings remain unresolved on the PR.

**Watch For**
- [Blocking] Finding title — `path/to/file.rs:42`
- [Non-blocking] Finding title — `path/to/file.ts:108`

<!-- robertobot:<summary-finding-id> -->
```

Do not append Memory Context, Suggested Reviewers, or any other appendix in incremental mode.

If approving (all resolved, no new findings):

```markdown
**AI Generated - Semantic Reviewer**

**Description**
All previously raised findings have been addressed.

**Recommendation**
Approve. Prior REQUEST_CHANGES dismissed.
```

If approving with only non-blocking findings still open:

```markdown
**AI Generated - Semantic Reviewer**

**Description**
Blocking findings are resolved. Remaining open items are non-blocking.

**Recommendation**
Approve. Remaining findings are non-blocking. Prior REQUEST_CHANGES dismissed.

**Watch For**
- [Non-blocking] Finding title — `path/to/file.ts:108`

<!-- robertobot:<summary-finding-id> -->
```

In both cases above, after posting the `COMMENT` review, dismiss the prior `REQUEST_CHANGES` review using the dismissals API (see section 2).

### Inline comment format (both modes)

Render each inline finding as:

````markdown
**AI Generated - Semantic Reviewer**

**[Blocking | P1, confirmed] Short finding title**

Evidence and impact.

**Suggested fix:** Concrete remediation.

```suggestion
exact complete replacement for the anchored range
```

<!-- robertobot:<inline-finding-id> -->
````

Use `Non-blocking` instead of `Blocking` when the finding is advisory.

Keep the suggestion block only when it is mechanically apply-ready: preserve indentation and provide the exact complete replacement for the anchored line range. Otherwise publish only the prose remediation.

Keep `robertobot:*` markers for backward-compatible deduplication.

## 4. Submit atomically

Build one payload:

```json
{
  "commit_id": "<reviewed_head_sha>",
  "event": "<COMMENT|REQUEST_CHANGES>",
  "body": "<signed review body>",
  "comments": [
    {
      "path": "src/file.ts",
      "line": 42,
      "side": "RIGHT",
      "body": "<signed inline finding>"
    }
  ]
}
```

For a multi-line comment, include `start_line` and `start_side: "RIGHT"`.

Recheck the PR head immediately before posting, then submit exactly once:

```bash
gh api --method POST "repos/$REPOSITORY/pulls/$PULL_NUMBER/reviews" \
  --input /tmp/pr-review-payload.json
```

**After posting**, if the event was `COMMENT` and `HAS_PRIOR_REQUEST_CHANGES` is true and no blocking findings remain, dismiss the stale review:

```bash
REVIEW_ID=$(gh api "repos/$REPOSITORY/pulls/$PULL_NUMBER/reviews" \
  --jq '[.[] | select(.user.login == "github-actions[bot]" and .state == "CHANGES_REQUESTED")] | last | .id')

if [ -n "$REVIEW_ID" ] && [ "$REVIEW_ID" != "null" ]; then
  gh api --method PUT "repos/$REPOSITORY/pulls/$PULL_NUMBER/reviews/$REVIEW_ID/dismissals" \
    -f message="Blocking findings resolved. Remaining items are non-blocking." \
    -f event="DISMISS"
fi
```

Do not create or patch a top-level issue comment. Do not post inline comments one at a time. GitHub rejects the entire review if an inline anchor is invalid, which prevents partial publication.

Note: `APPROVE` is not used because GitHub Actions tokens are not permitted to approve PRs in this repository. Use `COMMENT` + dismissal instead.

## 5. Verify

Read the created review and its comments through the GitHub API. Confirm:

- the review author is the expected automation identity;
- state matches the intended event (`CHANGES_REQUESTED`, `COMMENTED`, or `APPROVED`);
- `commit_id` equals `reviewed_head_sha`;
- the parent body begins with the canonical signature;
- every expected inline comment belongs to this review, has the expected path and right-side line range, and contains exactly one finding marker;
- every summary finding marker appears exactly once in the parent body; and
- the PR head still equals `reviewed_head_sha`.

Report the parent review URL and each inline discussion URL.

## 6. Log the verdict

After successful publication, output a summary for the workflow log:

```
Review published: <event> | Findings: <P1_count> P1, <P2_count> P2, <P3_count> P3 | Mode: <full|incremental> | Iteration: <N>
```
