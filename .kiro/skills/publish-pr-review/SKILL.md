---
name: publish-pr-review
description: Publish a semantic review as one GitHub COMMENT review whose parent body contains the complete review and whose actionable findings are attached inline.
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

## 1. Validate and deduplicate

Before any GitHub write:

1. Fetch the PR with `gh pr view` and require it to be open with a current head equal to `reviewed_head_sha`. Stop without posting if the head moved.
2. Fetch existing review bodies, inline comments, and issue comments. Suppress a finding if its hidden marker exists or any existing reviewer already raised the same concern. Markerless top-level comments count: compare normalized titles and the combination of path/code token plus failure mode or remediation.
3. Fetch the current diff. For every inline finding, prove that `line` and optional `start_line` are right-side added or modified lines in the same hunk. Keep an unanchorable concern in `summary_findings`; never attach it to an arbitrary line.
4. Do not publish findings below `likely` confidence.

If no new inline or summary finding remains, exit cleanly without creating a review. A clean review, approval, issue comment, or `Ship it!` message is outside this skill.

## 2. Render one combined review

The parent body starts with the canonical signature and preserves the complete high-level review:

```markdown
**AI Generated - Semantic Reviewer**

<complete contents of review_path>

<!-- robertobot:<summary-finding-id> -->
```

Append one hidden summary marker for each new summary finding.

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

## 3. Submit atomically

Build one payload:

```json
{
  "commit_id": "<reviewed_head_sha>",
  "event": "COMMENT",
  "body": "<signed complete semantic review>",
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

## 4. Verify

Read the created review and its comments through the GitHub API. Confirm:

- the review author is the expected automation identity;
- state is `COMMENTED` and `commit_id` equals `reviewed_head_sha`;
- the parent body begins with the canonical signature and contains the complete Markdown review;
- every expected inline comment belongs to this review, has the expected path and right-side line range, and contains exactly one finding marker;
- every summary finding marker appears exactly once in the parent body; and
- the PR head still equals `reviewed_head_sha`.

Report the parent review URL and each inline discussion URL.
