---
doc_meta:
  validated: 2026-01-27
  commit: 85403a86
  status: validated
  testable_headless: true
  category: settings-group
  title: Key Bindings Settings
  description: Settings for keyboard shortcuts and key bindings
  keywords: [settings, key, bindings, shortcuts, keyboard]
---

# Key Bindings Settings

Configure keyboard shortcuts and key bindings for various Kiro CLI features.

## chat.skimCommandKey

Key for fuzzy search command.

### Overview

Sets the keyboard shortcut key for triggering fuzzy search functionality. Used with Ctrl (Ctrl+key) to quickly search through files, commands, or history.

### Usage

```bash
kiro-cli settings chat.skimCommandKey "f"
```

**Type**: String (single character)  
**Default**: `"f"` (Ctrl+F)

---

## chat.autocompletionKey

Key for autocompletion hint acceptance.

### Overview

Sets the key used to accept autocompletion hints in the chat interface. When autocompletion suggestions appear, this key accepts the suggestion.

### Usage

```bash
kiro-cli settings chat.autocompletionKey "Tab"
```

**Type**: String  
**Default**: `"Tab"`

### Valid Keys

- `"Tab"` (default)
- `"Enter"`
- `"Space"`
- Single characters: `"a"`, `"b"`, etc.

---

## chat.delegateModeKey

Key for delegate command.

### Overview

Sets the keyboard shortcut key for triggering delegate mode. Used with Ctrl (Ctrl+key) to delegate tasks to specialized agents or tools.

### Usage

```bash
kiro-cli settings chat.delegateModeKey "d"
```

**Type**: String (single character)  
**Default**: `"d"` (Ctrl+D)

---

## chat.enableDelegate

Enable delegate tool for task delegation.

### Overview

Controls whether to enable the delegate tool for delegating tasks to specialized agents or external systems. When enabled, allows breaking down complex tasks and routing them to appropriate handlers.

### Usage

```bash
kiro-cli settings chat.enableDelegate true
```

**Type**: Boolean  
**Default**: `false`

### Examples

```bash
# Enable delegate tool
kiro-cli settings chat.enableDelegate true

# Check status
kiro-cli settings chat.enableDelegate

# Disable
kiro-cli settings chat.enableDelegate false
```