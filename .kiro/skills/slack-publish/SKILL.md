---
name: slack-publish
description: Publishes structured messages to Slack using Block Kit via curl. Used by the PR reviewer bot to post summaries to #kiro-cli-pr-reviews.
---

# Slack Publish

Post structured messages to Slack channels using the bot token and Block Kit.

## Method

Use curl with the bot token from `$SLACK_BOT_TOKEN` env var. Never use @slack-mcp/post_message (that posts as a personal account).

```bash
curl -s -X POST https://slack.com/api/chat.postMessage \
  -H "Authorization: Bearer $SLACK_BOT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '<JSON payload with channel + blocks>'
```

If `$SLACK_BOT_TOKEN` is not set, skip Slack posting silently.

## Target Channel

- `C0ARG53D0NL` — #kiro-cli-pr-reviews

## Block Kit Template (PR Review)

```json
{
  "channel": "C0ARG53D0NL",
  "unfurl_links": false,
  "blocks": [
    {"type": "header", "text": {"type": "plain_text", "text": "PR #N: TITLE"}},
    {"type": "context", "elements": [{"type": "mrkdwn", "text": "Author: X | Files: N | +A/-D | <link|#N>"}]},
    {"type": "divider"},
    {"type": "section", "text": {"type": "mrkdwn", "text": ":memo: *Summary*\n..."}},
    {"type": "section", "text": {"type": "mrkdwn", "text": ":warning: *Watch for*\n..."}},
    {"type": "divider"},
    {"type": "section", "text": {"type": "mrkdwn", "text": "*Behavioral Concerns*\n\n• ..."}},
    {"type": "divider"},
    {"type": "section", "text": {"type": "mrkdwn", "text": ":white_check_mark: *Verified*\n• cargo clippy — result"}},
    {"type": "section", "text": {"type": "mrkdwn", "text": ":brain: *Memory Context*\n..."}},
    {"type": "section", "text": {"type": "mrkdwn", "text": ":bust_in_silhouette: *Suggested Reviewers*\n..."}},
    {"type": "divider"},
    {"type": "section", "text": {"type": "mrkdwn", "text": ":label: *Recommendation:* Approve | Comment | Testing Required"}},
    {"type": "context", "elements": [{"type": "mrkdwn", "text": "_AI Generated - ReviewerBot_"}]}
  ]
}
```

## Rules

- If a section's text exceeds 3000 chars, split into multiple section blocks (Slack API limit)
- `header` uses `plain_text` (no mrkdwn)
- `context` blocks render as small gray text
- One divider between every logical section
- `unfurl_links: false` to prevent link previews
- Response `ok=true` + `ts` = success

## Setup

The bot token must be available as `$SLACK_BOT_TOKEN`. In CI, set it as a GitHub Actions secret:

```bash
gh secret set SLACK_REVIEWER_BOT_TOKEN --repo kiro-team/kiro-cli
```

For local use, store in `~/.config/botctl/kiro-reviewer/secrets.toml`:

```toml
bot_token = "xoxb-..."
```

Extract locally with:

```bash
export SLACK_BOT_TOKEN=$(grep bot_token ~/.config/botctl/kiro-reviewer/secrets.toml | cut -d'"' -f2)
```
