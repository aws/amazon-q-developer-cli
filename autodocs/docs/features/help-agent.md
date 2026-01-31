---
doc_meta:
  validated: 2026-01-30
  commit: 080f4235
  status: validated
  testable_headless: false
  category: feature
  title: Help Agent
  description: Built-in agent that answers questions about Kiro CLI features using documentation
  keywords: [help, agent, documentation, introspect, questions, features]
  related: [slash-help, introspect, planning-agent]
---

# Help Agent

The Help Agent is a built-in agent that answers questions about Kiro CLI features, commands, tools, and configuration. It uses the `introspect` tool to search comprehensive documentation and can create/modify configuration files in `.kiro/` directories.

## Getting Started

**Accessing the Help Agent**

1. **Slash Command**: Use `/help` to switch to the Help Agent
```
> /help
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
2. **With Immediate Question**: Use `/help` with your question
```
> /help How do I configure MCP servers?
```
3. **Classic Help Text**: Use `/help --legacy` for traditional command listing

**Visual Indicators:**
- `[help]` indicator in purple (brand color)
- Welcome message when switching to Help Agent

## What You Can Ask

The Help Agent can answer questions about:

- **Slash Commands**: `/save`, `/agent`, `/context`, `/tools`, etc.
- **Built-in Tools**: `fs_read`, `fs_write`, `code`, `grep`, `glob`, etc.
- **CLI Commands**: `kiro-cli chat`, `kiro-cli settings`, `kiro-cli agent`, etc.
- **Configuration Settings**: All settings available via `kiro-cli settings`
- **Features**: Tangent Mode, Hooks, MCP, Code Intelligence, etc.
- **Keyboard Shortcuts**: Available shortcuts and how to use them
- **Workflows**: Best practices and common usage patterns

The Help Agent can also create and modify configuration files:

- **Agents**: Create custom agents in `.kiro/agents/`
- **Prompts**: Create prompt templates in `.kiro/prompts/`
- **LSP Configs**: Configure language servers in `.kiro/`

## Examples

### Example 1: Ask About a Command

```
[help] > How do I save a conversation?

Use the `/save` command to save your current conversation:

**Basic save:**
/save

**Save with custom name:**
/save my-session-name

Saved conversations are stored in `~/.kiro/sessions/` and can be loaded 
later with `/load`.
```

### Example 2: Ask About a Tool

```
[help] > What does the code tool do?

The `code` tool provides code intelligence features:

• **search_symbols** - Find symbol definitions by name
• **lookup_symbols** - Get details for specific symbols
• **get_document_symbols** - List all symbols in a file
• **pattern_search** - AST-based structural search
• **pattern_rewrite** - AST-based code transformation

It uses AST parsing for accurate code understanding across multiple languages.
```

### Example 3: Ask About Configuration

```
[help] > How do I enable tangent mode?

Enable Tangent Mode with:

kiro-cli settings set chat.enableTangentMode true

Or use the `/tangent` command during a chat session to toggle it.

Tangent Mode lets you explore side questions without losing your main 
conversation context.
```

## Returning to Previous Agent

Use `/help` again to return to your previous agent:

```
[help] > /help

✔ Switched to agent: kiro_default

> 
```

Or use `/agent swap` to switch to a specific agent.

## Key Features

**Configuration Assistance**: The Help Agent can create and modify files in `.kiro/` directories, helping you set up agents, prompts, and LSP configurations.

**Accurate Answers**: Responses are based on actual Kiro CLI documentation, not general knowledge.

**Context Preservation**: Your conversation history is preserved when switching between agents.

**Toggle Behavior**: Running `/help` while in the Help Agent returns you to your previous agent.

**Legacy Mode**: Use `/help --legacy` to see the classic help text listing all slash commands.

## Troubleshooting

### Issue: Help Agent Can't Answer Question

**Symptom**: "I don't have information about that"  
**Cause**: Feature may be undocumented or question unclear  
**Solution**: Try rephrasing or ask about a related feature

### Issue: Outdated Information

**Symptom**: Answer doesn't match current behavior  
**Cause**: Documentation may need updating  
**Solution**: Report via `/issue` command

## Related Features

- [/help](../slash-commands/help.md) - Slash command to access Help Agent
- [introspect](../tools/introspect.md) - Tool used by Help Agent
- [Planning Agent](planning-agent.md) - Another built-in agent for planning

## Technical Details

**Agent Name**: `kiro_help`

**Tools**: 
- `introspect` - Search Kiro CLI documentation
- `fs_write` - Create/modify files (restricted to `.kiro/**` and `~/.kiro/**`)

**MCP Servers**: Disabled (`includeMcpJson: false`)

**Prompt Indicator**: `[help]` displayed in purple (brand color)
