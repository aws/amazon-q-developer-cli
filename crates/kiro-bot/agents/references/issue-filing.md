# GitHub write workflow

Load this reference only when the user asks to file an issue or comment.

## New issue

1. Search `search_github_issues` using sanitized symptom and command terms.
   Omit secrets, customer identifiers, internal hostnames, and unrelated
   context. Try a second focused query when the first is empty.
2. For a bug, verify the relevant source so documented behavior is not filed as a defect.
3. Show the best close match with number, title, state, and why it may match.
   Show up to three only when ambiguity affects the decision. If none match,
   show the proposed title and a concise sanitized body.
4. Ensure the user requested this exact mutation.
5. Invoke `create_github_issue`; the host's Slack approval is the final gate.
6. After success, return the issue link. Never claim success before the tool
   returns it.

## Existing issue comment

Confirm the target issue and show the sanitized draft. Ensure the user
requested this exact mutation, invoke `comment_on_existing`, and rely on the
host's Slack approval as the final gate before reporting success.

Never include tokens, internal hostnames, customer identifiers, or unrelated private Slack content. Taskei tools cannot create or update work items.
