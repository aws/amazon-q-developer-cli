---
description: Fold reviewer PR comments back into the weekly ops review report and push a fixup
---

# Apply Ops Review Comments

After the ops meeting, fold the PR comments left on a `[Kiro-CLI] Weekly Ops Review` PR
back into the report by following **Step 11 of the `weekly-ops-review` skill** — it is the
single source of truth for this workflow.

You may pass the PR number or URL (e.g. `apply the ops review comments on PR 3145`). If you
don't, find the open PR whose head branch matches `ops/weekly-review-*`.

What the skill does:
- Reads all unresolved PR comments — both conversation comments
  (`gh pr view {pr} --json comments,reviews`) and inline review comments with file/line
  anchors (`gh api repos/{owner}/{repo}/pulls/{pr}/comments`).
- Interprets each actionable comment as an edit to `.ops/weekly-reviews/{end_date}.md`
  (commonly Sections 3 Pain Level, 5 Action Items, 8 Security Risks, 10 Dashboard Review,
  or a correction to a table row).
- Checks out the PR branch, applies the edits, and pushes a fixup commit.

Key rules (enforced by the skill):
- Apply only what the comments ask; if a comment is ambiguous, leave it and flag it in the
  summary rather than guessing.
- Do NOT resolve reviewers' comment threads on their behalf.
- Do NOT merge the PR — the team merges when the report is finalized.
- Only stage `.ops/weekly-reviews/{end_date}.md`; never bundle unrelated changes.
