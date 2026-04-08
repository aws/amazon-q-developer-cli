---
doc_meta:
  validated: 2026-03-25
  commit: e0e72e43
  status: validated
  testable_headless: true
  category: settings-group
  title: Chat Interface Settings
  description: Settings for chat interface behavior and appearance
  keywords: [settings, chat, interface, ui, notifications, bell, osc9]
---

# Chat Interface Settings

Configure chat interface behavior, appearance, and user experience.

## chat.editMode

Enable edit mode for chat interface.

### Overview

Controls whether edit mode is enabled in the chat interface, allowing users to modify their input before sending messages.

### Usage

```bash
kiro-cli settings chat.editMode true
```

**Type**: Boolean  
**Default**: `false`

---

## chat.enableNotifications

Enable terminal notifications for chat events.

### Overview

Controls whether Kiro CLI sends terminal notifications when:
- A response completes (no pending tool calls)
- Permission is required for a tool action

### Usage

```bash
kiro-cli settings chat.enableNotifications true
```

**Type**: Boolean  
**Default**: `false`

---

## chat.notificationMethod

Set the notification method for terminal alerts.

### Overview

Controls how Kiro CLI sends terminal notifications. By default, the method is auto-detected based on your terminal.

### Usage

```bash
kiro-cli settings chat.notificationMethod "osc9"
```

**Type**: String  
**Default**: `"auto"`  
**Values**: `"auto"`, `"bel"`, `"osc9"`

### Methods

- **auto**: Auto-detect based on terminal (recommended)
- **bel**: ASCII BEL character (`\x07`) - works in most terminals
- **osc9**: OSC 9 escape sequence - shows system notification with message

### Terminal Support

**OSC 9** (auto-detected via `TERM_PROGRAM`):
- Ghostty
- iTerm2
- WezTerm
- Windows Terminal

**BEL** (auto-detected via `TERM`):
- xterm, xterm-256color
- tmux, screen
- Alacritty
- Konsole, GNOME Terminal
- Emacs eat

### Examples

Force BEL for a terminal not auto-detected:
```bash
kiro-cli settings chat.notificationMethod "bel"
```

Use OSC 9 for rich notifications in supported terminals:
```bash
kiro-cli settings chat.notificationMethod "osc9"
```

---

## chat.disableAutoCompaction

Disable automatic conversation summarization.

### Overview

Controls whether Kiro CLI automatically compacts long conversations by summarizing older messages to save context window space. When disabled, conversations may hit context limits faster but preserve full detail.

### Usage

```bash
kiro-cli settings chat.disableAutoCompaction true
```

**Type**: Boolean  
**Default**: `false`

### Trade-offs

**Disabled (true)**:
- Full conversation detail preserved
- May hit context limits faster
- Better for debugging/review

**Enabled (false)**:
- Longer conversations possible
- Older messages summarized
- More efficient context usage

---

## chat.enableHistoryHints

Show conversation history hints.

### Overview

Controls whether to display hints about conversation history, such as when messages have been compacted or when approaching context limits.

### Usage

```bash
kiro-cli settings chat.enableHistoryHints true
```

**Type**: Boolean  
**Default**: `false`

### Examples

Shows hints like "Previous messages summarized" or "Context 80% full".

---

## chat.uiMode

Set UI variant for chat interface.

### Overview

Controls which UI variant to use for the chat interface. Different modes provide different layouts and interaction patterns.

### Usage

```bash
kiro-cli settings chat.uiMode "compact"
```

**Type**: String  
**Default**: `"default"`  
**Values**: `"default"`, `"compact"`, `"minimal"`

### UI Modes

- **default**: Standard interface with full features
- **compact**: Reduced spacing and smaller elements
- **minimal**: Minimal interface for focused work