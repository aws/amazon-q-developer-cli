---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: tool
  title: introspect
  description: Self-awareness tool providing information about Kiro CLI capabilities and documentation
  keywords: [introspect, help, documentation, capabilities, features]
  related: [slash-help, tangent-mode]
---

# introspect

Self-awareness tool providing information about Kiro CLI capabilities and documentation.

## Overview

The introspect tool enables Kiro CLI to answer questions about itself. It provides access to embedded documentation, command help, settings, and feature information. Automatically activates when you ask questions like "How do I save conversations?" or "What tools are available?"

## How It Works

When you ask Kiro CLI questions about its own features, the introspect tool automatically activates. It searches embedded documentation (README, docs/*.md files, settings list, changelog) and returns relevant information. Documentation is embedded at compile time for offline access.

## Usage

### Basic Usage

Simply ask Kiro CLI questions about itself:

```
> How do I save my conversation?
> What experimental features are available?
> Can you read files?
```

No explicit tool invocation needed - introspect activates automatically.

### Common Use Cases

#### Use Case 1: Learning Commands

```
> What slash commands are available?
```

**What this does**: Returns list of all slash commands with descriptions from built-in help.

#### Use Case 2: Understanding Settings

```
> How do I enable tangent mode?
```

**What this does**: Searches documentation for tangent mode and provides configuration command.

#### Use Case 3: Tool Discovery

```
> What tools can you use to work with files?
```

**What this does**: Identifies file-related tools (fs_read, fs_write, glob, grep) and explains capabilities.

#### Use Case 4: Feature Information

```
> What is code intelligence?
```

**What this does**: Returns information from code-intelligence.md documentation.

## Configuration

### Auto-Tangent Mode

Enable automatic tangent mode for introspect queries:

```bash
kiro-cli settings chat.introspectTangentMode true
```

When enabled, introspect questions automatically enter tangent mode, keeping help separate from main conversation.

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `chat.introspectTangentMode` | boolean | `false` | Auto-enter tangent mode for introspect questions |

## Embedded Documentation

Introspect includes:

- **README.md** - Project overview
- **docs/built-in-tools.md** - Tool documentation
- **docs/experiments.md** - Experimental features
- **docs/agent-file-locations.md** - Agent configuration
- **docs/tangent-mode.md** - Tangent mode guide
- **docs/web-search.md** - Web search feature (if enabled)
- **docs/code-intelligence.md** - Code intelligence (if enabled)
- **docs/planning-agent.md** - Planning agent
- **docs/introspect-tool.md** - This tool's documentation
- **docs/todo-lists.md** - TODO list feature
- **docs/hooks.md** - Hooks system
- **docs/knowledge-management.md** - Knowledge base
- **Settings list** - All available settings with descriptions
- **Changelog** - Recent version changes
- **CONTRIBUTING.md** - Contribution guidelines

## Examples

### Example 1: Getting Started

```
> How do I get started with Kiro CLI?
```

**Expected Output**: Information from README about installation and basic usage.

### Example 2: Checking Settings

```
> What settings can I configure?
```

**Expected Output**: List of all settings with descriptions and types.

### Example 3: Understanding Features

```
> How do hooks work?
```

**Expected Output**: Information from docs/hooks.md about hook system.

### Example 4: Tool Permissions

```
> How do tool permissions work?
```

**Expected Output**: Explanation of allowedTools, toolsSettings, and permission prompts.

## Troubleshooting

### Issue: Incomplete Information

**Symptom**: Answer doesn't include expected details  
**Cause**: Information not in embedded documentation  
**Solution**: Check if feature is documented in docs/ directory. Introspect can only provide documented information.

### Issue: Outdated Information

**Symptom**: Documentation doesn't match current behavior  
**Cause**: Documentation out of sync with code  
**Solution**: Verify against actual behavior. File issue if documentation incorrect.

### Issue: Can't Find Setting

**Symptom**: Introspect doesn't mention a setting  
**Cause**: Setting may be new or not yet documented  
**Solution**: Use `kiro-cli settings list` to see all settings directly.

### Issue: Too Much Context

**Symptom**: Introspect responses clutter main conversation  
**Cause**: Not using tangent mode  
**Solution**: Enable `chat.introspectTangentMode` to auto-enter tangent mode for help questions.

## Related Features

- [Tangent Mode](../features/tangent-mode.md) - Isolate help conversations
- [/help](../slash-commands/help.md) - Built-in help command
- [Settings](../commands/settings.md) - Configuration management

## Limitations

- Only provides information from embedded documentation
- Documentation embedded at compile time (not dynamically updated)
- Cannot access external documentation or web resources
- May not include very recent features added after last build
- Relies on LLM interpretation which may occasionally be inaccurate
- No access to user-specific configuration or state

## Technical Details

**Aliases**: `introspect`

**Documentation Embedding**: Uses `include_str!()` macros to embed documentation at compile time.

**Conditional Docs**: Some docs only included if feature flags enabled (web-search, code-intelligence).

**Dynamic Content**: Settings list generated dynamically from Setting enum at runtime.

**Response Format**: Returns JSON with:
- `built_in_help`: Rendered slash command help
- `documentation`: Concatenated embedded docs
- `query_context`: Original user query
- `recommendations`: Tool recommendations (currently unused)

**Tangent Mode Integration**: If `chat.introspectTangentMode` enabled and tangent mode experiment enabled, automatically adds footer instructing to end responses with tangent mode reminder.

**Permissions**: Trusted by default, no configuration needed.
