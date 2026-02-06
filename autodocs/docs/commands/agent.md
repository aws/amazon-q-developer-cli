---
doc_meta:
  validated: 2026-02-05
  commit: adc1a97a
  status: validated
  testable_headless: true
  category: command
  title: kiro-cli agent
  description: Manage agent configurations including list, validate, create, edit, migrate, and set-default operations
  keywords: [agent, config, manage, validate, create, schema, description]
  related: [slash-agent, agent-config]
---

# kiro-cli agent

Manage agent configurations including list, validate, create, edit, migrate, and set-default operations.

## Overview

The agent command manages agent configuration files. List available agents, validate configurations, create new configs, edit existing ones, migrate profiles, and set default agents. Agents stored in `.kiro/agents/` (local) or `~/.kiro/agents/` (global).

## Usage

### Basic Usage

```bash
kiro-cli agent list
```

## Options

| Option | Short | Description |
|--------|-------|-------------|
| `--verbose` | `-v` | Increase logging verbosity (can be repeated) |
| `--help` | `-h` | Print help information |

### Common Use Cases

#### Use Case 1: List All Agents

```bash
kiro-cli agent list
```

**What this does**: Shows all available agents from local and global directories.

#### Use Case 2: Validate Agent Config

```bash
kiro-cli agent validate --path ~/.kiro/agents/my-agent.json
```

**What this does**: Checks if agent configuration is valid JSON and matches schema.

#### Use Case 3: Create New Agent

```bash
kiro-cli agent create --name example-agent
```

**What this does**: Creates new agent configuration file with specified name.

#### Use Case 4: Edit Existing Agent

```bash
kiro-cli agent edit --name my-agent
```

**What this does**: Opens agent configuration in default editor for modification.

#### Use Case 5: Set Default Agent

```bash
kiro-cli agent set-default --name rust-expert
```

**What this does**: Sets specified agent as default for new chat sessions.

## Subcommands

### list

List all available agents.

```bash
kiro-cli agent list
```

**Output**: Names of all agents in local and global directories.

### validate

Validate agent configuration.

```bash
kiro-cli agent validate --path <PATH>
```

**Parameters**:
- `--path <PATH>`: Path to agent configuration file

**Checks**:
- Valid JSON syntax
- Matches agent schema
- Required fields present
- Tool references valid

### create

Create new agent configuration.

```bash
kiro-cli agent create --name <AGENT_NAME> [--directory <DIR>] [--from <TEMPLATE>]
```

**Parameters**:
- `--name, -n`: Name for new agent (required)
- `--directory, -d`: Directory to save agent (optional, defaults to global)
- `--from, -f`: Template agent to copy from (optional)

**Creates**: Agent configuration file with specified name.

### edit

Edit existing agent configuration.

```bash
kiro-cli agent edit [--name <AGENT_NAME>] [--path <PATH>]
```

**Parameters**:
- `--name, -n`: Name of agent to edit (defaults to current agent)
- `--path`: Path to agent configuration file

**Opens**: Agent configuration in default editor.

**Note**: Built-in agents cannot be edited. Attempting to edit a built-in agent returns an error suggesting to create a new agent instead.

### migrate

Migrate profiles to agent format.

```bash
kiro-cli agent migrate
```

**Warning**: Potentially destructive to existing agents in global directories.

### set-default

Set default agent for new chat sessions.

```bash
kiro-cli agent set-default --name <AGENT_NAME>
```

**Parameters**:
- `--name, -n`: Name of agent to set as default (required)

### help

Show help for agent command or subcommands.

```bash
kiro-cli agent help [SUBCOMMAND]
```

## Examples

### Example 1: List Agents

```bash
kiro-cli agent list
```

**Expected Output**:
```
Workspace: ~/project/.kiro/agents
Global:    ~/.kiro/agents

* rust-expert       Workspace     Rust development with cargo and clippy
  code-reviewer     Workspace     Code review agent focused on security and best practices
  python-dev        Global        Python development assistant
  kiro_default      (Built-in)    Default agent
  kiro_help         (Built-in)    Help agent that answers questions about Kiro CLI features
  kiro_planner      (Built-in)    Specialized planning agent for implementation plans
```

Active agent marked with `*`. Shows agent name, source (Workspace/Global/Built-in), and description. Long descriptions wrap based on terminal width.

### Example 2: Validate Config

```bash
kiro-cli agent validate --path ~/.kiro/agents/rust-expert.json
```

**Expected Output**:
```
✓ Agent 'rust-expert' is valid
```

### Example 3: Create New Agent

```bash
kiro-cli agent create --name code-reviewer
```

**Expected Output**:
```
✓ Created agent configuration: .kiro/agents/code-reviewer.json
```

### Example 4: Edit Agent

```bash
kiro-cli agent edit
```

**Expected Output**: Opens current agent's configuration in default editor.

```bash
kiro-cli agent edit --name rust-expert
```

**Expected Output**: Opens `rust-expert` agent configuration in default editor.

### Example 5: Attempt to Edit Built-in Agent

```bash
kiro-cli agent edit --name kiro_default
```

**Expected Output**:
```
Cannot edit built-in agent 'kiro_default'. Create a new agent with 'kiro-cli agent create'
```

### Example 5: Set Default Agent

```bash
kiro-cli agent set-default --name python-dev
```

**Expected Output**:
```
✓ Set 'python-dev' as default agent
```

## Agent File Locations

**Local (workspace-specific)**: `.kiro/agents/` in current directory  
**Global (user-wide)**: `~/.kiro/agents/` in home directory

Local agents take precedence over global agents with same name.

## Troubleshooting

### Issue: Agent Not Found

**Symptom**: "Agent not found" error  
**Cause**: Agent file doesn't exist  
**Solution**: Check spelling. Use `kiro-cli agent list` to see available agents.

### Issue: Validation Fails

**Symptom**: Schema mismatch error  
**Cause**: Invalid agent configuration  
**Solution**: Check error message for specific issue. Use `kiro-cli agent schema` to see required format.

### Issue: Can't Generate Agent

**Symptom**: Permission denied or directory not found  
**Cause**: `.kiro/agents/` directory doesn't exist  
**Solution**: Create directory: `mkdir -p .kiro/agents/`

## Related Features

- [/agent](../slash-commands/agent-switch.md) - Switch agents in chat
- [/agent generate](../slash-commands/agent-generate.md) - Generate agent in chat
- [Agent Configuration](../features/agent-configuration.md) - Complete agent format guide

## Limitations

- Agent names must match filename (without .json)
- Local agents override global agents with same name
- No agent inheritance or composition
- Changes require restarting chat session

## Technical Details

**Agent Resolution**: Local (`.kiro/agents/`) checked first, then global (`~/.kiro/agents/`).

**File Format**: JSON files with `.json` extension.

**Validation**: Uses JSON schema validation against agent format specification.

**Example Agent Tools**: fs_read, fs_write, execute_bash, use_aws, gh_issue, introspect, knowledge, thinking, todo_list, delegate, grep, glob.
