---
name: gh-issues
description: Track and analyze top GitHub issues for the Kiro repo. Use when the user asks about open issues, bug reports, feature requests, community feedback, or issue trends for kiro-cli.
---

# GitHub Issues Tracker

Fetches open issues from `kirodotdev/Kiro` using the `gh` CLI, filters for CLI-relevant issues (excluding Kiro IDE), and ranks by 👍 reactions.

## How to fetch issues

Use the GitHub search API to paginate through all open issues:

```bash
gh api "search/issues?q=repo:kirodotdev/Kiro+is:issue+is:open+created:>YYYY-MM-DD&sort=reactions-%2B1&order=desc&per_page=100&page=N"
```

- Default date range: last 30 days from current date
- Paginate until no more items are returned (up to 10 pages)
- The `reactions.+1` field in each item gives the thumbs-up count

## Filtering rules

- **Exclude** issues with labels: `ide`, `theme:ide-performance`, `theme:ide`
- **Exclude** issues labeled `duplicate` unless the user asks for them
- **Include** issues with labels like `cli`, `chat`, `mcp`, `hooks`, `specs`, `sub-agents`, `skills`, `trusted-commands`, `powers`, `auth`, `usability`
- When ambiguous (no CLI/IDE label), include the issue — let the user decide

## Default output

- Top 10 issues sorted by 👍 (descending), then by issue number (descending) for ties
- Show: rank, thumbs-up count, issue number (linked), title, creation date
- Summarize key themes at the end

## User can customize

- Time range (e.g., "last week", "last 3 months")
- Number of results
- Filter by state (open/closed/all — default open)
- Filter by label
- Include/exclude duplicates
