---
doc_meta:
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: true
  category: tool
  title: introspect
  description: Self-awareness tool providing information about Kiro CLI capabilities and documentation
  keywords: [introspect, help, documentation, capabilities, features, search]
  related: [guide, help]
---

## Overview

> **Note**: This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions about Kiro CLI naturally, and the assistant will use this tool to provide answers.

The introspect tool enables Kiro CLI to answer questions about itself. It searches embedded documentation using hybrid search (semantic + BM25) and returns relevant results. Automatically activates when you ask questions like "How do I save conversations?" or "What tools are available?"

## How It Works

Introspect has three modes:

1. **No query** — Returns the doc index (metadata for all documented features)
2. **With query** — Performs hybrid semantic + BM25 search across all docs, returns ranked results
3. **With doc_path** — Returns the full content of a specific document

Documentation is embedded at compile time for offline access.

## Usage

Simply ask Kiro CLI questions about itself:

```
> How do I save my conversation?
> What slash commands are available?
> Can you read files?
```

No explicit tool invocation needed — introspect activates automatically.

### Common Use Cases

#### Learning Commands

```
> What slash commands are available?
```

Returns list of all slash commands with descriptions.

#### Understanding Settings

```
> How do I change the default model?
```

Searches documentation and provides the configuration command.

#### Tool Discovery

```
> What tools can you use to work with files?
```

Identifies file-related tools (read, write, glob, grep) and explains capabilities.

#### Feature Information

```
> What is code intelligence?
```

Returns information from the code intelligence documentation.

## Embedded Documentation

Introspect includes documentation for:

- **tools/** — All built-in tools (read, write, shell, code, grep, glob, etc.)
- **slash-commands/** — All slash commands (/help, /guide, /agent, /chat, etc.)
- **commands/** — CLI commands (chat, agent, settings, mcp, etc.)
- **features/** — Features (code intelligence, hooks, knowledge, MCP, etc.)
- **settings/** — Configuration options (default model, default agent, knowledge, etc.)

## Examples

### Example 1: Getting Started

```
> How do I get started with Kiro CLI?
```

Returns overview information about installation and basic usage.

### Example 2: Checking Settings

```
> What settings can I configure?
```

Returns list of available settings with descriptions and types.

### Example 3: Understanding Features

```
> How do hooks work?
```

Returns information about the hook system from hooks documentation.

### Example 4: Tool Permissions

```
> How do tool permissions work?
```

Returns explanation of trust settings and permission prompts.

## Troubleshooting

### Incomplete information

**Symptom**: Answer doesn't include expected details
**Cause**: Information not in embedded documentation
**Solution**: Check if feature is documented. Introspect can only provide documented information.

### Outdated information

**Symptom**: Documentation doesn't match current behavior
**Cause**: Documentation out of sync with code
**Solution**: Verify against actual behavior. File feedback if documentation is incorrect.

### Can't find a setting

**Symptom**: Introspect doesn't mention a setting
**Cause**: Setting may be new or not yet documented
**Solution**: Use `kiro-cli settings list` to see all settings directly.

## Related

- [/guide](../slash-commands/guide.md) — Switch to guide agent for interactive help
- [/help](../slash-commands/help.md) — List all slash commands
- [Settings](../commands/settings.md) — Configuration management

## Limitations

- Only provides information from embedded documentation
- Documentation embedded at compile time (not dynamically updated)
- Cannot access external documentation or web resources
- May not include very recent features added after last build
