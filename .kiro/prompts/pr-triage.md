---
description: Triage open PRs in kiro-cli — identify what to review, merge, or close
---

# PR Triage

Triage open PRs in `kiro-team/kiro-cli` using `scripts/classify_prs.py` and `gh` CLI.

## Steps

1. **Generate fresh classification**
   ```bash
   python3 scripts/classify_prs.py --repo kiro-team/kiro-cli --limit 300 --output /tmp/pr_report.md
   ```

2. **Check dependabot PRs**
   ```bash
   gh pr list --repo kiro-team/kiro-cli --author app/dependabot --json number,title,mergeable,mergeStateStatus --limit 100
   ```
   - `CONFLICTING` → close them (dependabot will reopen fresh)
   - `MERGEABLE/BLOCKED` → list for review/merge
   - Close conflicting ones in batches: `gh pr close <numbers> --repo kiro-team/kiro-cli`

3. **Apply labels** to unlabeled human PRs based on classification:
   - `bug-fix` → `bug`
   - `feature` → `enhancement`
   - `papercut` → `papercut`
   - duplicate clusters → `duplicate`

4. **Recommend next actions**
   - Papercuts with < 50 lines changed: easiest wins to merge
   - Bug fixes touching prod code (`crates/`, `packages/tui/src/`): prioritize review
   - Duplicate PRs: flag to author, suggest closing the older one
   - Massive PRs (> 10k lines): flag for design review before merging

## Output

Present a prioritized list:
1. PRs ready to merge (no conflicts, small diff, labeled)
2. PRs needing review (prod-impacting features/bugs)
3. PRs to close (duplicates, stale, conflicting deps)
