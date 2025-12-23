---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: setting
  title: chat.greetingEnabled
  description: Show or hide greeting message when starting chat sessions
  keywords: [setting, greeting, welcome, message]
---

# chat.greetingEnabled

Show or hide greeting message when starting chat sessions.

## Overview

Controls whether Kiro CLI displays a greeting message when starting new chat sessions.

## Usage

### Disable Greeting

```bash
kiro-cli settings chat.greetingEnabled false
```

### Enable Greeting

```bash
kiro-cli settings chat.greetingEnabled true
```

### Check Status

```bash
kiro-cli settings chat.greetingEnabled
```

## Value

**Type**: Boolean  
**Default**: `true`

## Examples

### Example 1: Disable Greeting

```bash
kiro-cli settings chat.greetingEnabled false
```

No greeting shown on chat start.

### Example 2: Re-enable

```bash
kiro-cli settings chat.greetingEnabled true
```

## Technical Details

**Scope**: User-wide setting

**Effect**: Applies to all new chat sessions
