---
doc_meta:
  title: chat.disableWrap
  description: Disable line wrapping in chat output for clean copy-paste of long lines
  category: setting
  keywords: [wrap, wrapping, line-wrap, copy, paste, output, formatting]
  related: [chat-interface-settings, disable-markdown-rendering]
  validated: 2026-04-29
  commit: 624cfc69
  status: validated
  testable_headless: true
---

# chat.disableWrap

Disable line wrapping in chat output for clean copy-paste of long lines.

## Overview

Controls whether Kiro CLI wraps long lines in chat output. When disabled, long lines soft-wrap visually (they appear to wrap on screen) but remain as single logical lines. This makes copy-paste cleaner since you get the original line without inserted newlines.

**Type**: Boolean  
**Default**: `false` (wrapping enabled)

## Usage

### Check Current Value

```bash
kiro-cli settings chat.disableWrap
```

### Enable (Disable Wrapping)

```bash
kiro-cli settings chat.disableWrap true
```

### Disable (Enable Wrapping)

```bash
kiro-cli settings chat.disableWrap false
```

### Reset to Default

```bash
kiro-cli settings --reset chat.disableWrap
```

## Examples

### Example 1: Copy Long Commands

With wrapping enabled (default), a long command might copy as:

```
aws s3 cp s3://my-bucket/path/to/file.txt /local/destination/
path/file.txt --recursive
```

With wrapping disabled, the same command copies as a single line:

```
aws s3 cp s3://my-bucket/path/to/file.txt /local/destination/path/file.txt --recursive
```

### Example 2: Copy Code Snippets

When copying code from chat output, disabled wrapping preserves the original formatting without artificial line breaks.

### Example 3: Terminal Width Independence

With wrapping disabled, resizing your terminal doesn't change where logical line breaks occur in copied text.

## Troubleshooting

### Issue: Lines Still Appear Wrapped

**Symptom**: Long lines still wrap visually after enabling the setting  
**Cause**: This is expected - the setting affects logical lines, not visual display  
**Solution**: The visual soft-wrap is intentional. Copy the text to see that it's a single logical line.

### Issue: Horizontal Scrolling

**Symptom**: Need to scroll horizontally to see full lines  
**Cause**: Some terminals show unwrapped lines with horizontal scroll  
**Solution**: This is terminal-dependent behavior. Most terminals soft-wrap visually.

## Related

- [Chat Interface Settings](chat-interface-settings.md) - Other chat UI settings
- [disable-markdown-rendering](disable-markdown-rendering.md) - Control markdown rendering

## Limitations

- Visual soft-wrapping behavior depends on your terminal emulator
- Does not affect how the AI generates responses, only how they display
- Setting applies to new output only, not existing chat history

## Technical Details

**Setting Key**: `chat.disableWrap`  
**Scope**: User preference (not workspace-overridable)  
**Storage**: User settings database
