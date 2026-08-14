# GitHub write workflow

Load this reference only when the user asks to file an issue or comment.

## New issue

1. Search `search_github_issues` using the symptom, command, and exact error where available. Try a second focused query when the first is empty.
2. For a bug, verify the relevant source so documented behavior is not filed as a defect.
3. Show up to three close matches with number, title, state, and why each may match. If none match, show the proposed title and a concise sanitized body.
4. Wait for explicit confirmation that this is new and should be filed.
5. Invoke `create_github_issue` and wait for the Slack approval result.
6. After success, return the issue link. Never claim success before the tool returns it.

## Existing issue comment

Confirm the target issue and show the sanitized draft. Wait for explicit confirmation, invoke `comment_on_existing`, and wait for Slack approval before reporting success.

Never include tokens, internal hostnames, customer identifiers, or unrelated private Slack content. Taskei tools cannot create or update work items.
