#!/usr/bin/env python3
"""
classify_prs.py — Fetch and classify open PRs for a GitHub repo.

Usage:
    python3 scripts/classify_prs.py [--repo <owner/repo>] [--limit <n>] [--output <file>]

Defaults:
    --repo   kiro-team/kiro-cli
    --limit  300
    --output /tmp/pr_report.md

Requires: gh CLI authenticated

Classification rules:
    bug-fix   : branch/title contains fix/, fix(, bug, hotfix, patch
    feature   : branch/title contains feat/, feat(, feature, add, support
    rfc       : branch/title contains rfc, proposal, design, spec
    papercut  : branch/title contains chore, docs, refactor, update, bump,
                cleanup, improve, typo — OR diff < 50 lines and <= 3 files

Duplicate detection:
    Groups PRs by first 2-3 meaningful title words and flags clusters of 2+.

Prod impact proxy:
    non-prod  : title contains docs, readme, changelog, sop, agents.md
    prod      : everything else with a non-zero diff

Update this script when:
    - New branch naming conventions are adopted
    - New classification categories are needed
    - Repo target changes
"""

import argparse
import json
import re
import subprocess
import sys
from collections import defaultdict


def fetch_prs(repo: str, limit: int) -> list:
    result = subprocess.run(
        [
            "gh", "pr", "list",
            "--repo", repo,
            "--limit", str(limit),
            "--state", "open",
            "--json", "number,title,labels,author,createdAt,additions,deletions,changedFiles,headRefName,baseRefName",
        ],
        capture_output=True, text=True
    )
    if result.returncode != 0:
        print(f"Error fetching PRs: {result.stderr}", file=sys.stderr)
        sys.exit(1)
    return json.loads(result.stdout)


def classify(pr: dict) -> tuple[str, str]:
    title = pr["title"].lower()
    branch = pr["headRefName"].lower()
    diff = pr.get("additions", 0) + pr.get("deletions", 0)
    changed = pr.get("changedFiles", 0)

    if pr["author"]["login"] == "app/dependabot":
        return "deps", "non-prod"

    if any(x in branch or x in title for x in ["fix/", "fix(", "bug", "hotfix", "patch"]):
        kind = "bug-fix"
    elif any(x in branch or x in title for x in ["rfc", "proposal", "design", "spec"]):
        kind = "rfc"
    elif any(x in branch or x in title for x in ["chore", "docs", "refactor", "update", "bump", "cleanup", "clean", "improve", "typo"]):
        kind = "papercut"
    elif any(x in branch or x in title for x in ["feat/", "feat(", "feature", "add ", "support "]):
        kind = "feature"
    elif diff < 50 and changed <= 3:
        kind = "papercut"
    else:
        kind = "feature"

    if any(x in title for x in ["docs", "readme", "changelog", "sop", "agents.md"]):
        prod = "non-prod"
    elif diff == 0 and changed == 0:
        prod = "unknown"
    else:
        prod = "prod"

    return kind, prod


def find_duplicates(prs: list) -> dict:
    groups = defaultdict(list)
    for pr in prs:
        words = re.sub(r"[^\w\s]", " ", pr["title"].lower()).split()
        key = " ".join(words[1:3]) if len(words) > 2 else " ".join(words)
        groups[key].append(pr)
    return {k: v for k, v in groups.items() if len(v) > 1}


def render_report(prs: list, duplicates: dict, repo: str) -> str:
    from collections import defaultdict
    buckets = defaultdict(list)
    for pr in prs:
        kind, prod = classify(pr)
        pr["_kind"] = kind
        pr["_prod"] = prod
        buckets[kind].append(pr)

    lines = [f"# PR Classification Report ({len(prs)} open PRs)\n"]

    for kind in ["papercut", "bug-fix", "feature", "rfc", "deps"]:
        items = buckets[kind]
        prod_count = sum(1 for p in items if p["_prod"] == "prod")
        lines.append(f"## {kind.upper()} — {len(items)} PRs ({prod_count} prod)\n")
        lines.append("| # | Title | Author | Diff | Files | Prod |")
        lines.append("|---|-------|--------|------|-------|------|")
        for p in sorted(items, key=lambda x: x.get("additions", 0) + x.get("deletions", 0)):
            diff = p.get("additions", 0) + p.get("deletions", 0)
            title = p["title"][:65] + ("…" if len(p["title"]) > 65 else "")
            url = f"https://github.com/{repo}/pull/{p['number']}"
            lines.append(f"| [#{p['number']}]({url}) | {title} | {p['author']['login']} | {diff} | {p.get('changedFiles',0)} | {p['_prod']} |")
        lines.append("")

    lines.append(f"## POTENTIAL DUPLICATES — {len(duplicates)} clusters\n")
    for key, group in sorted(duplicates.items(), key=lambda x: -len(x[1])):
        lines.append(f"**`{key}`** ({len(group)} PRs)")
        for p in group:
            url = f"https://github.com/{repo}/pull/{p['number']}"
            lines.append(f"- [#{p['number']}]({url}): {p['title']}")
        lines.append("")

    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description="Classify open GitHub PRs")
    parser.add_argument("--repo", default="kiro-team/kiro-cli")
    parser.add_argument("--limit", type=int, default=300)
    parser.add_argument("--output", default="/tmp/pr_report.md")
    args = parser.parse_args()

    print(f"Fetching PRs from {args.repo}...")
    prs = fetch_prs(args.repo, args.limit)
    print(f"Fetched {len(prs)} PRs")

    duplicates = find_duplicates(prs)
    report = render_report(prs, duplicates, args.repo)

    with open(args.output, "w") as f:
        f.write(report)

    print(f"Report written to {args.output}")

    # Print summary to stdout
    from collections import defaultdict
    buckets = defaultdict(int)
    for pr in prs:
        kind, _ = classify(pr)
        buckets[kind] += 1
    for kind, count in sorted(buckets.items()):
        print(f"  {kind}: {count}")
    print(f"  duplicate clusters: {len(duplicates)}")


if __name__ == "__main__":
    main()
