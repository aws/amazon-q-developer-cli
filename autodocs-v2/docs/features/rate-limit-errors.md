---
doc_meta:
  title: Rate Limit and Capacity Errors
  description: How Kiro CLI surfaces throttling, model overload, and monthly usage limit errors
  category: feature
  keywords: [rate limit, throttling, model overloaded, monthly limit, capacity, error, transient alert]
  related: [model-refusal-alerts, model]
  validated: 2026-07-15
  commit: a40272a56
  status: validated
  testable_headless: false
---

## Overview

When a model request fails because of throttling, model capacity, or a monthly usage limit, Kiro CLI ends the current turn and shows a transient alert with recovery guidance.

| Error | Message |
|-------|---------|
| Throttling | "Rate limit exceeded. Please wait a moment before trying again." |
| Model overloaded | "The model you've selected is temporarily unavailable. Please use '/model' to select a different model and try again." |
| Monthly limit reached | "The monthly usage limit has been reached" |

## How It Works

The backend classifies the streaming error and sends a rate-limit notification to the TUI. The TUI displays the notification for five seconds without adding it to chat scrollback.

Notifications are not shown for follow-up model errors after a goal has already completed.

## Recovery

### Throttling

Wait briefly, then retry the message.

### Model Overloaded

Use `/model` to select a different model, then retry the message.

### Monthly Limit Reached

The limit is enforced by the service and cannot be cleared from the CLI. Retry after access becomes available, or contact your administrator if the limit is unexpected.

## Limitations

- These alerts are available only in the TUI.
- Alerts disappear after five seconds and are not added to chat scrollback.
- The CLI displays the service-provided monthly-limit message but does not report when the limit will reset.

## Related

- [Model Refusal Alerts](model-refusal-alerts.md) - Content-policy refusal handling
- [/model](../slash-commands/model.md) - Switch AI models
