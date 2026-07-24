---
doc_meta:
  title: chat.showThinkingTips
  description: Show or hide the feature tip below the thinking indicator while waiting
  category: setting
  keywords: [setting, thinking, tips, indicator, waiting, display, hint]
  related: [show-thinking, settings]
  validated: 2026-07-24
  commit: 28dbf3ee9
  status: validated
  testable_headless: true
---

# chat.showThinkingTips

Show or hide the feature tip below the thinking indicator while waiting.

## Overview

The `chat.showThinkingTips` setting controls whether a short feature tip appears below the thinking indicator while the agent is reasoning. Tips surface useful shortcuts and features (e.g., "Use /compact to free up context") and rotate each time you wait.

When disabled, the thinking indicator still appears but no tip text is shown beneath it. This reduces visual noise if you already know the tips.

## Default

`true` (enabled)

**Type**: Boolean  
**Scope**: Global and workspace  
**Key**: `chat.showThinkingTips`

## Usage

### Disable via CLI

```bash
kiro-cli settings chat.showThinkingTips false
```

### Enable via CLI

```bash
kiro-cli settings chat.showThinkingTips true
```

### Toggle via Display Settings panel

```
/settings display
```

Then toggle "Thinking tips" on or off.

### Check current value

```bash
kiro-cli settings chat.showThinkingTips
```

## Examples

### Example 1: Disable Thinking Tips

```bash
kiro-cli settings chat.showThinkingTips false
```

The thinking indicator still shows "Thinking..." or "Thought for Ns" but no tip appears below it.

### Example 2: Re-enable Thinking Tips

```bash
kiro-cli settings chat.showThinkingTips true
```

A rotating feature tip appears below the thinking indicator after a short delay.

### Example 3: Configure in Workspace Settings

Edit `.kiro/settings/cli.json` in your project:

```json
{
  "chat.showThinkingTips": false
}
```

This disables tips for that workspace only.

## Behavior

- Tips appear after a short delay once the agent starts reasoning — not instantly.
- A new tip is selected each time the agent enters a thinking phase.
- The setting is read when the thinking indicator mounts. Toggling the setting takes effect on the next agent turn.
- This setting is independent of `chat.showThinking`. Even with thinking set to `off` (which hides the entire indicator), the tips setting has no visible effect since the indicator itself is hidden.

## Troubleshooting

### Issue: Tips Still Appear After Disabling

**Symptom**: You set `chat.showThinkingTips` to `false` but a tip still appeared  
**Cause**: The setting is read when the thinking indicator mounts (start of each agent turn)  
**Solution**: The change takes effect on the next agent turn. Send another message and the tip will no longer appear.

### Issue: No Tips Appear Even When Enabled

**Symptom**: Setting is `true` but no tip shows during thinking  
**Cause**: Tips only appear after a delay. Short reasoning phases may complete before the tip triggers.  
**Solution**: This is expected. Tips surface only when the agent reasons long enough for the delay to elapse.

### Issue: Thinking Indicator Itself Is Hidden

**Symptom**: Neither indicator nor tip is visible  
**Cause**: `chat.showThinking` may be set to `off`  
**Solution**: Set `chat.showThinking` to `collapsed` or `expanded` to see the indicator and tips.

## Related

- [chat.showThinking](show-thinking.md) — Control reasoning block display mode
- [kiro-cli settings](../commands/settings.md) — Manage all settings
