---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: setting
  title: chat.disableMarkdownRendering
  description: Disable markdown formatting in chat output for plain text display
  keywords: [setting, markdown, rendering, format, plain]
---

# chat.disableMarkdownRendering

Disable markdown formatting in chat output for plain text display.

## Overview

Controls whether AI responses are rendered with markdown formatting (bold, italic, code blocks, etc.) or displayed as plain text.

## Usage

### Disable Markdown

```bash
kiro-cli settings chat.disableMarkdownRendering true
```

### Enable Markdown

```bash
kiro-cli settings chat.disableMarkdownRendering false
```

## Value

**Type**: Boolean  
**Default**: `false` (markdown enabled)

## Examples

### Example 1: Disable Formatting

```bash
kiro-cli settings chat.disableMarkdownRendering true
```

Responses shown as plain text without formatting.

### Example 2: Re-enable

```bash
kiro-cli settings chat.disableMarkdownRendering false
```

## Use Cases

- Terminal doesn't support formatting
- Prefer plain text output
- Copying text without formatting
- Accessibility requirements

## Technical Details

**Scope**: User-wide setting

**Effect**: Applies to all chat sessions

**Formatting Disabled**: Bold, italic, code blocks, headers, lists
