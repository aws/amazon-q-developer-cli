---
doc_meta:
  validated: 2026-01-05
  commit: a1d370b5
  status: validated
  testable_headless: true
  category: settings-group
  title: Chat Interface Settings
  description: Settings for chat interface behavior and appearance
  keywords: [settings, chat, interface, ui, notifications]
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

Enable desktop notifications for chat events.

### Overview

Controls whether Kiro CLI shows desktop notifications for important chat events like completion of long-running tasks or errors.

### Usage

```bash
kiro-cli settings chat.enableNotifications true
```

**Type**: Boolean  
**Default**: `false`

### Use Cases

- Long-running operations completion
- Error alerts
- Background task updates

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