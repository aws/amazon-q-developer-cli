---
doc_meta:
  title: Error Recovery
  description: Automatic retry and error handling for model response failures
  category: feature
  keywords: [error, retry, empty response, failure, recovery, resilience]
  related: [exit-codes, chat]
  validated: 2026-06-02
  commit: a953a204b
  status: validated
  testable_headless: false
---

## Overview

Kiro automatically handles certain transient model response failures by retrying requests. This improves reliability without requiring manual intervention.

## Empty Response Handling

When the model returns an empty response (a valid stream with no content), Kiro:

1. Automatically retries the same request once
2. If the retry succeeds, continues normally
3. If the retry also returns empty, displays an error and ends the turn

### What Causes Empty Responses

Empty responses can occur when:
- Content filtering triggers on the model side
- Transient service issues prevent content generation
- Model capacity constraints cause incomplete responses

### User Experience

In most cases, the retry succeeds and you won't notice anything unusual. If the retry fails, you'll see:

```
Kiro failed to generate a response
```

The conversation continues and you can retry your request manually or rephrase it.

## Examples

### Successful Recovery

You send a prompt. The first attempt returns empty, but the retry succeeds:

```
> Explain the code in main.rs

The main.rs file contains the entry point for the application...
```

No error is shown because the retry succeeded transparently.

### Failed Recovery

Both attempts return empty:

```
> Generate something problematic

Kiro failed to generate a response
```

You can try rephrasing your request or asking something different.

### Scripting with Headless Mode

When using `--no-interactive` mode, empty response failures after retry result in exit code 1:

```bash
kiro-cli chat --no-interactive "Your prompt"
if [ $? -eq 1 ]; then
  echo "Request failed - may need manual retry"
fi
```

## Behavior Details

| Scenario | Behavior |
|----------|----------|
| First empty response | Silent retry with same request |
| Second consecutive empty response | Error shown, turn ends |
| Empty response after successful turn | Counts as first empty, retry triggered |
| Partial response then disconnect | Not treated as empty (content was received) |

## Troubleshooting

### "Kiro failed to generate a response"

This means two consecutive attempts returned no content.

**Try:**
- Rephrase your prompt
- Break complex requests into smaller parts
- Wait a moment and try again (transient issues may resolve)

### Frequent Empty Responses

If you see this error often:
- Check your network connectivity
- Verify your authentication is current with `kiro-cli whoami`
- The model service may be experiencing issues

## Related

- [Exit Codes](exit-codes.md) — Exit codes for automation and scripting
- [kiro-cli chat](../commands/chat.md) — Main chat command reference
