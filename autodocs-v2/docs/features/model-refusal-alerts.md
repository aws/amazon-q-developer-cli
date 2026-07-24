---
doc_meta:
  title: Model Refusal Alerts
  description: How Kiro CLI surfaces model content-policy refusals and content-filtered responses to users
  category: feature
  keywords: [refusal, content filter, content policy, model error, blocked, alert, stop reason]
  related: [model, classic-vs-tui]
  validated: 2026-07-24
  commit: 989ca47a4
  status: validated
  testable_headless: false
---

## Overview

When a model refuses to continue a conversation due to content-policy restrictions or the response is content-filtered by the provider, Kiro CLI writes an error message into the conversation scrollback explaining what happened and how to recover.

Previously, content-filtered responses could cause the agent to stop silently mid-turn with no feedback. Now, the TUI detects these events and writes an error message directly into the conversation scrollback so you always know why the conversation stopped.

## How It Works

The model provider may refuse a request or filter the response for several reasons:

- The conversation context triggers a content-safety policy
- The accumulated conversation history crosses a content threshold
- A specific prompt or tool output is flagged by the provider

When this occurs, Kiro CLI receives a `CONTENT_FILTERED` stop reason or refusal metadata from the provider and writes an error message into the conversation scrollback.

## What You See

When a refusal occurs, an error message appears in-line in the conversation scrollback at the point where the turn stopped. The message shows either:

1. **Provider explanation** — The specific reason given by the model provider, when available
2. **Default guidance** — If no explanation is provided:

```
The selected model couldn't process this request. Try a different model with
/model, rewind with /rewind, or start a new session with /chat new.
```

The message stays in the scrollback permanently, so you can scroll back to review it at any time.

## Recovery Options

When you encounter a model refusal, you have several options:

### Switch Models

Use `/model` to select a different model that may handle your request:

```
/model claude-sonnet-4
```

Different models have different content policies, so switching may resolve the issue.

### Start a New Conversation

If the accumulated conversation history is triggering the filter, starting fresh can help:

```
/chat new
```

### Rewind the Conversation

Use `/rewind` to go back to an earlier point before the problematic content was introduced:

```
/rewind
```

This lets you retry with a different approach without losing earlier conversation context.

### Rephrase Your Request

If a specific message triggered the refusal, try rephrasing your request to avoid the flagged content.

## Examples

### Example 1: Refusal with Provider Explanation

You send a message and the model refuses. A system message appears in the conversation:

```
⚠ This request was flagged by the model's content policy. The conversation
contains content that cannot be processed. Please start a new conversation or
switch to a different model.
```

### Example 2: Content-Filtered Stop with No Explanation

The model stops mid-response due to content filtering. A system message appears with the default guidance:

```
⚠ The selected model couldn't process this request. Try a different model with
/model, rewind with /rewind, or start a new session with /chat new.
```

### Example 3: Recovery by Switching Models

After seeing a refusal alert:

```
/model claude-sonnet-4
```

Then retry your message. The new model may process the request successfully.

## Troubleshooting

### Alert appears unexpectedly

Content policies can sometimes be triggered by code snippets, log outputs, or tool results that contain flagged patterns. If you believe the refusal is incorrect:

1. Try `/rewind` to remove the problematic turn
2. Rephrase your request or ask the agent to approach the task differently
3. Switch to a different model with `/model`

### Alert keeps appearing after switching models

If the conversation history itself contains flagged content, switching models alone may not help. Use `/chat new` to start fresh, or `/rewind` to a point before the flagged content was introduced.

### No explanation provided

Not all providers include a detailed explanation with refusals. When only a `CONTENT_FILTERED` stop reason is received without additional details, the default guidance message is shown.

## Limitations

- This feature is available only in the TUI (V2 interface). Classic mode does not display refusal alerts.
- The refusal explanation depends on what the model provider returns — some providers give detailed reasons, others only a stop code.
- The recommended model suggestion (when provided) is informational; availability depends on your region and account.

## Related

- [Rate Limit and Capacity Errors](rate-limit-errors.md) — Throttling, model overload, and monthly limit errors
- [/model](../slash-commands/model.md) — Switch AI models
- [/rewind](../slash-commands/rewind.md) — Rewind conversation to earlier point
- [Classic vs TUI](classic-vs-tui.md) — Differences between classic and TUI modes
