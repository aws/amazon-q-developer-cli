---
doc_meta:
  title: Model Refusal Alerts
  description: How Kiro CLI surfaces model content-policy refusals and content-filtered responses to users
  category: feature
  keywords: [refusal, content filter, content policy, model error, blocked, alert, stop reason]
  related: [model, classic-vs-tui]
  validated: 2026-07-12
  commit: 85d11e4fc
  status: validated
  testable_headless: false
---

## Overview

When a model refuses to continue a conversation due to content-policy restrictions or the response is content-filtered by the provider, Kiro CLI displays an error alert explaining what happened and how to recover.

Previously, content-filtered responses could cause the agent to stop silently mid-turn with no feedback. Now, the TUI detects these events and surfaces a temporary error notification so you always know why the conversation stopped. A permanent copy is also written to the conversation scrollback for reference.

## How It Works

The model provider may refuse a request or filter the response for several reasons:

- The conversation context triggers a content-safety policy
- The accumulated conversation history crosses a content threshold
- A specific prompt or tool output is flagged by the provider

When this occurs, Kiro CLI receives a `CONTENT_FILTERED` stop reason or refusal metadata from the provider and immediately displays an error alert in the TUI.

## What You See

When a refusal occurs, an error alert appears above your input area. The alert shows either:

1. **Provider explanation** — The specific reason given by the model provider, when available
2. **Default guidance** — If no explanation is provided:

```
The selected model cannot continue this conversation. Please select a different
model, or start a new conversation, or rewind the current conversation to an
earlier point and try a different approach.
```

The alert automatically hides after 8 seconds. A permanent copy of the refusal message is also written to the conversation scrollback, so you can scroll back to review it at any time.

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

You send a message and the model refuses. The TUI displays:

```
⚠ Error: This request was flagged by the model's content policy. The conversation
contains content that cannot be processed. Please start a new conversation or
switch to a different model.
```

### Example 2: Content-Filtered Stop with No Explanation

The model stops mid-response due to content filtering. The TUI displays the default message:

```
⚠ Error: The selected model cannot continue this conversation. Please select a
different model, or start a new conversation, or rewind the current conversation
to an earlier point and try a different approach.
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

- [/model](../slash-commands/model.md) — Switch AI models
- [/rewind](../slash-commands/rewind.md) — Rewind conversation to earlier point
- [Classic vs TUI](classic-vs-tui.md) — Differences between classic and TUI modes
