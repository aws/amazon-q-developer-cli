---
name: publish-pr-review
description: Publish a semantic review as one GitHub review. Supports full mode (complete review body + inline) and incremental mode (new findings only). Supports REQUEST_CHANGES verdict for P1/P2 findings.
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

## Iteration Mode

The environment variable `REVIEW_MODE` determines the publish behavior:

- **`full`** (default, first review): Post the complete semantic review as parent body with all inline findings
- **`incremental`** (subsequent reviews): Post only NEW findings not already raised, with a brief summary body

## 1. Validate and deduplicate

Before any GitHub write:

1. Fetch the PR with `gh pr view` and require it to be open with a current head equal to `reviewed_head_sha`. Stop without posting if the head moved.
2. Fetch existing review bodies, inline comments, and issue comments. Suppress a finding if its hidden marker exists or any existing reviewer already raised the same concern. Markerless top-level comments count: compare normalized titles and the combination of path/code token plus failure mode or remediation.
3. Fetch the current diff. For every inline finding, prove that `line` and optional `start_line` are right-side added or modified lines in the same hunk. Keep an unanchorable concern in `summary_findings`; never attach it to an arbitrary line.
4. Do not publish findings below `likely` confidence.

**Incremental mode additional checks:**
5. Compare each finding against prior bot inline comments on this PR. A finding is a DUPLICATE if an existing bot comment targets the same `path` and addresses the same concern (normalized title + code token match).
6. Identify RESOLVED findings: prior bot comments where the targeted line has been modified or removed in the current diff. List these in the summary body.

If no new inline or summary finding remains:
- **Full mode:** exit cleanly without creating a review.
- **Incremental mode:** if all prior findings are resolved, submit an `APPROVE` review (see section 3).

## 2. Determine review event

Based on finding priorities:

| Condition | Event |
|-----------|-------|
| Any P1 finding (confirmed) present and unresolved | `REQUEST_CHANGES` |
| Any P2 finding (confirmed) present and unresolved | `REQUEST_CHANGES` |
| P2 (likely) or P3/P4 findings only | `COMMENT` |
| No new findings, all prior resolved (incremental only) | `APPROVE` |

## 3. Render the review

### Full mode

The parent body starts with the canonical signature and preserves the complete high-level review:

```markdown
**AI Generated - Semantic Reviewer**

<complete contents of review_path>

<!-- robertobot:<summary-finding-id> -->
```

Append one hidden summary marker for each new summary finding.

### Incremental mode

The parent body is a brief summary:

```markdown
**AI Generated - Semantic Reviewer**

**Re-review (iteration N):** X new finding(s), Y previously raised finding(s) now resolved.

**New findings:**
- [P1, confirmed] Finding title — `path/to/file.rs:42`
- [P2, likely] Finding title — `path/to/file.ts:108`

**Resolved (addressed since last review):**
- ~~Byte-slice panic~~ — `path/to/file.rs:38` (line removed/modified) ✅

<!-- robertobot:<summary-finding-id> -->
```

If approving (all resolved, no new findings):

```markdown
**AI Generated - Semantic Reviewer**

All previously raised findings have been addressed. ✅

**Resolved:**
- ~~Finding title~~ — `path/to/file.rs:38` ✅
- ~~Finding title~~ — `path/to/file.ts:108` ✅
```

### Inline comment format (both modes)

Render each inline finding as:

````markdown
**AI Generated - Semantic Reviewer**

**[P1, confirmed] Short finding title**

Evidence and impact.

**Suggested fix:** Concrete remediation.

```suggestion
exact complete replacement for the anchored range
```

<!-- robertobot:<inline-finding-id> -->
````

Keep the suggestion block only when it is mechanically apply-ready: preserve indentation and provide the exact complete replacement for the anchored line range. Otherwise publish only the prose remediation.

Keep `robertobot:*` markers for backward-compatible deduplication.

## 4. Submit atomically

Build one payload:

```json
{
  "commit_id": "<reviewed_head_sha>",
  "event": "<COMMENT|REQUEST_CHANGES|APPROVE>",
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

Do not create or patch a top-level issue comment. Do not post inline comments one at a time. GitHub rejects the entire review if an inline anchor is invalid, which prevents partial publication.

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
