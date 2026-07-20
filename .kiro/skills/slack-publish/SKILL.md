---
name: slack-publish
description: Publishes structured messages to Slack using Block Kit via curl. Used by the PR reviewer bot to post summaries to #kiro-cli-pr-reviews.
---

# Slack Publish

Post structured messages to Slack channels using the bot token and Block Kit.

## Method

Use curl with the bot token from `$SLACK_BOT_TOKEN` env var. Never use @slack-mcp/post_message (that posts as a personal account).

If `$SLACK_BOT_TOKEN` is not set, skip Slack posting silently.

`#kiro-cli-pr-reviews` is a dedicated PR-triage channel: keep it scannable. Post **one one-line summary per PR to the channel**, then post all detail as **threaded replies** under it. This keeps the channel a clean list of PRs while the full review stays one click away.

Two steps — post the summary, capture its `ts`, then reply into the thread with `thread_ts`:

```bash
# 1. Post the one-line summary; capture the parent message ts
PARENT_TS=$(curl -s -X POST https://slack.com/api/chat.postMessage \
  -H "Authorization: Bearer $SLACK_BOT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '<parent payload, see "Channel summary line" below>' \
  | jq -r '.ts')

# 2. Post the detail as a reply in that thread (thread_ts = parent ts)
curl -s -X POST https://slack.com/api/chat.postMessage \
  -H "Authorization: Bearer $SLACK_BOT_TOKEN" \
  -H "Content-Type: application/json" \
  -d "$(jq -n --arg ts "$PARENT_TS" '{channel:"C0ARG53D0NL", thread_ts:$ts, unfurl_links:false, blocks:[ /* detail blocks */ ]}')"
```

If step 1 returns no `ts` (`ok=false`), skip step 2 and report the error — do not post detail as its own channel message.

## Target Channel

- `C0ARG53D0NL` — #kiro-cli-pr-reviews

## Channel summary line (parent message)

Exactly one line. Lets someone triage the channel without opening the thread: PR link, title, recommendation, author, size. The full review lives in the thread.

```json
{
  "channel": "C0ARG53D0NL",
  "unfurl_links": false,
  "blocks": [
    {"type": "section", "text": {"type": "mrkdwn", "text": ":label: <link|*#N*> TITLE — RECO_EMOJI *Recommendation* · _author_ · +A/-D"}}
  ]
}
```

`RECO_EMOJI`: `:white_check_mark:` Approve · `:speech_balloon:` Comment · `:wrench:` Request Changes · `:test_tube:` Testing Required. Keep TITLE short; truncate to ~80 chars so the line never wraps.

## Thread reply (detail)

Everything that was previously in the channel message goes here instead, as a reply with `thread_ts` set to the parent `ts`.

```json
{
  "channel": "C0ARG53D0NL",
  "thread_ts": "<PARENT_TS>",
  "unfurl_links": false,
  "blocks": [
    {"type": "context", "elements": [{"type": "mrkdwn", "text": "Author: X | Files: N | +A/-D | <link|#N>"}]},
    {"type": "section", "text": {"type": "mrkdwn", "text": ":memo: *Summary*\n..."}},
    {"type": "section", "text": {"type": "mrkdwn", "text": ":warning: *Watch for*\n..."}},
    {"type": "divider"},
    {"type": "section", "text": {"type": "mrkdwn", "text": "*Behavioral Concerns*\n\n• ..."}},
    {"type": "divider"},
    {"type": "section", "text": {"type": "mrkdwn", "text": ":white_check_mark: *Verified*\n• cargo clippy — result"}},
    {"type": "section", "text": {"type": "mrkdwn", "text": ":brain: *Memory Context*\n..."}},
    {"type": "section", "text": {"type": "mrkdwn", "text": ":bust_in_silhouette: *Suggested Reviewers*\n..."}},
    {"type": "context", "elements": [{"type": "mrkdwn", "text": "_AI Generated - ReviewerBot_"}]}
  ]
}
```

## Rules

- One summary line per PR in the channel; **all** detail goes in the thread. Never post detail as a second channel-level message.
- If a thread reply's text exceeds 3000 chars, split into multiple section blocks (Slack API limit); every extra block stays in the same thread (`thread_ts`).
- `context` blocks render as small gray text
- One divider between logical sections in the thread reply
- `unfurl_links: false` on both the parent and the reply to prevent link previews
- Step 1 response `ok=true` + `ts` = success; that `ts` is the thread anchor for step 2

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
