---
description: Generate a classified PR report for kiro-cli, grouped by type with duplicate detection
---

# PR Report

Run `scripts/classify_prs.py` to fetch and classify all open PRs in `kiro-team/kiro-cli`.

```bash
python3 scripts/classify_prs.py --repo kiro-team/kiro-cli --limit 300 --output /tmp/pr_report.md
```

The report will be written to `/tmp/pr_report.md` and grouped as:
- **papercut** — small, low-risk improvements
- **bug-fix** → labeled `bug`
- **feature** → labeled `enhancement`
- **rfc** — proposals and design docs
- **deps** — dependabot bumps (separated from real bugs)

Each section is sorted by diff size (smallest first) with clickable PR links.

Duplicate clusters are listed at the bottom.

After generating, summarize:
1. Total counts per category
2. Any duplicate clusters worth flagging
3. Dependabot status: how many are conflicting vs mergeable
