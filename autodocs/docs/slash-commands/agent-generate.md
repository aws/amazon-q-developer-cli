---
doc_meta:
  validated: 2025-12-23
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /agent generate
  description: Create new agent configuration interactively with AI assistance
  keywords: [agent, generate, create, ai, interactive]
  related: [agent-swap, cmd-agent, agent-config]
---

# /agent generate

Create new agent configuration interactively with AI assistance.

## Overview

The `/agent generate` command uses AI to help create a new agent configuration. Interactive process asks about agent purpose, tools needed, and generates valid JSON configuration file.

## Usage

```
/agent generate
```

Starts interactive agent generation process.

## How It Works

1. AI asks about agent purpose and requirements
2. You describe what the agent should do
3. AI suggests tools, settings, and configuration
4. Generates agent JSON file in `.kiro/agents/`
5. Agent immediately available for use

## Examples

### Example 1: Generate Code Review Agent

```
/agent generate
```

**Interaction**:
```
AI: What would you like this agent to do?
You: Review code for security issues and best practices

AI: I'll create a code-review agent with these tools:
- fs_read: Read code files
- grep: Search for patterns
- code: Analyze code structure

[Generates .kiro/agents/code-review.json]

✔ Created agent: code-review
```

### Example 2: Generate Data Analysis Agent

```
/agent generate
```

**Interaction**:
```
You: Analyze CSV files and generate reports

AI: Creating data-analysis agent with:
- fs_read: Read data files
- execute_bash: Run analysis scripts
- fs_write: Generate reports

[Generates configuration]
```

## Generated Configuration

AI creates complete agent JSON with:
- Agent name and description
- Appropriate tools for task
- Tool permissions (allowedTools)
- Tool settings (toolsSettings)
- Optional prompt/context

## Troubleshooting

### Issue: Generation Fails

**Symptom**: Error during generation  
**Cause**: Invalid configuration or file write error  
**Solution**: Check `.kiro/agents/` directory exists and is writable

### Issue: Agent Not Available After Generation

**Symptom**: Can't find newly generated agent  
**Cause**: File not created or invalid JSON  
**Solution**: Check `.kiro/agents/` directory for file. Validate with `kiro-cli agent validate <name>`

### Issue: AI Suggests Wrong Tools

**Symptom**: Generated agent has inappropriate tools  
**Cause**: Unclear description  
**Solution**: Be specific about agent's purpose. Edit generated file manually if needed.

## Related Features

- [/agent](agent-swap.md) - Switch to generated agent
- [kiro-cli agent create](../commands/agent.md) - CLI version
- [Agent Configuration](../agent-config/overview.md) - Manual agent creation
- [/tools](tools.md) - See available tools

## Limitations

- Generates in local `.kiro/agents/` only (not global)
- Requires interactive input (not available in headless mode)
- AI suggestions may need manual refinement
- Can't modify existing agents (only create new)

## Technical Details

**Output Location**: `.kiro/agents/<name>.json` in current directory

**Validation**: Generated configuration validated before saving

**Tool Selection**: AI selects from available built-in tools based on description

**Format**: Standard agent JSON format with all required fields

**Immediate Availability**: Agent usable immediately after generation with `/agent swap <name>`
