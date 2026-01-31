---
doc_meta:
  validated: 2026-01-30
  commit: 080f4235
  status: validated
  testable_headless: false
  category: slash_command
  title: /help
  description: Switch to the Help Agent to ask questions about Kiro CLI features and commands
  keywords: [help, agent, documentation, commands, features, questions, legacy]
  related: [help-agent, introspect, tools]
---

# /help

Switch to the Help Agent to ask questions about Kiro CLI features and commands.

## Overview

The `/help` command switches to the built-in Help Agent, which can answer questions about Kiro CLI features, commands, tools, and configuration. The Help Agent uses the `introspect` tool to search documentation and can also create/modify configuration files in `.kiro/` directories.

## Usage

### Switch to Help Agent

```
/help
```

Switches to the Help Agent and shows a welcome message.

### Ask a Question Directly

```
/help <question>
```

Switches to the Help Agent and immediately asks your question.

### Return to Previous Agent

```
/help
```

When already in the Help Agent, running `/help` again toggles back to your previous agent.

### Show Classic Help Text

```
/help --legacy
```

Shows the classic help text listing all slash commands instead of switching to the Help Agent.

## Examples

### Example 1: Switch to Help Agent

```
/help
```

**Output**:
```
✔ Switched to agent: kiro_help

Welcome to Kiro CLI Help!

I can answer questions about Kiro CLI and help you configure it:
• Slash commands (/agent, /context, /tools, etc.)
• Built-in tools (fs_read, code, grep, etc.)
• Configuration settings
• Features like MCP, Tangent Mode, Code Intelligence
• Create/modify agents, prompts, and LSP configs in .kiro/

Just ask me anything about Kiro CLI!

Common questions:
• "How do I save a conversation?"
• "What tools are available?"
• "How does the code tool work?"
• "Create a new agent for me"

Tip: Use /help to return to your previous agent
For the classic help text, use /help --legacy

[help] > 
```

### Example 2: Ask Question Directly

```
/help How do I save a conversation?
```

**Output**:
```
✔ Switched to agent: kiro_help

[help] > How do I save a conversation?

To save a conversation, use the `/save` command:
...
```

### Example 3: Return to Previous Agent

When already in the Help Agent:

```
[help] > /help
```

**Output**:
```
✔ Switched to agent: kiro_default

> 
```

### Example 4: Ask Another Question While in Help Agent

```
[help] > /help What tools are available?
```

When already in the Help Agent, `/help <question>` just asks the question without switching agents.

### Example 5: Show Classic Help Text

```
/help --legacy
```

**Output**:
```
Usage: /[COMMAND]

Commands:
  agent      Manage agents
  changelog  Show recent changes
  ...
```

## Troubleshooting

### Issue: Help Agent Not Responding

**Symptom**: No response after asking question  
**Cause**: Network or service issue  
**Solution**: Check connection and try again

### Issue: Can't Find Information

**Symptom**: Help Agent says feature isn't documented  
**Cause**: Feature may be new or undocumented  
**Solution**: Try rephrasing question or check `/tools` for available tools

## Related Features

- [Help Agent](../features/help-agent.md) - Full Help Agent documentation
- [introspect](../tools/introspect.md) - Tool used by Help Agent
- [/tools](tools.md) - View available tools

## Technical Details

**Agent Name**: `kiro_help`

**Tools Available**: `introspect` (documentation search) and `fs_write` (restricted to `.kiro/` directories)

**Prompt Indicator**: `[help]` shown in purple (brand color)
