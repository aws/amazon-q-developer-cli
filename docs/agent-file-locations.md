# Agent File Locations

Agent configuration files can be placed in two different locations, allowing for both workspace-specific and user-wide agent configurations.

## Local Agents (Workspace-Specific)

Local agents are stored in the current working directory under:

```
.aws/amazonq/agents/
```

These agents are specific to the current workspace or project and are only available when running Q CLI from that directory or its subdirectories.

**Example structure:**
```
my-project/
├── .aws/
│   └── amazonq/
│       └── agents/
│           ├── dev-agent.json
│           └── aws-specialist.json
└── src/
    └── main.py
```

## Global Agents (User-Wide)

Global agents are stored in your home directory under:

```
~/.aws/amazonq/agents/
```

These agents are available from any directory when using Q CLI.

**Example structure:**
```
~/.aws/amazonq/agents/
├── general-assistant.json
├── code-reviewer.json
└── documentation-writer.json
```

## Agent Precedence

When Q CLI looks for an agent, it follows this precedence order:

1. **Local first**: Checks `.aws/amazonq/agents/` in the current working directory
2. **Global fallback**: If not found locally, checks `~/.aws/amazonq/agents/` in the home directory

## Naming Conflicts

If both local and global directories contain agents with the same name, the **local agent takes precedence**. When this happens, Q CLI will display a warning message:

```
WARNING: Agent conflict for my-agent. Using workspace version.
```

The global agent with the same name will be ignored in favor of the local version.

## Best Practices

### Use Local Agents For:
- Project-specific configurations
- Agents that need access to specific project files or tools
- Development environments with unique requirements
- Sharing agent configurations with team members via version control

### Use Global Agents For:
- General-purpose agents used across multiple projects
- Personal productivity agents
- Agents that don't require project-specific context
- Commonly used development tools and workflows

## Example Usage

To create a local agent for your current project:

```bash
mkdir -p .aws/amazonq/agents
cat > .aws/amazonq/agents/project-helper.json << 'EOF'
{
  "description": "Helper agent for this specific project",
  "tools": ["fs_read", "fs_write", "execute_bash"],
  "resources": [
    "file://README.md",
    "file://docs/**/*.md"
  ]
}
EOF
```

To create a global agent available everywhere:

```bash
mkdir -p ~/.aws/amazonq/agents
cat > ~/.aws/amazonq/agents/general-helper.json << 'EOF'
{
  "description": "General purpose assistant",
  "tools": ["*"],
  "allowedTools": ["fs_read"]
}
EOF
```

## Directory Creation

Q CLI will automatically create the global agents directory (`~/.aws/amazonq/agents/`) if it doesn't exist. However, you need to manually create the local agents directory (`.aws/amazonq/agents/`) in your workspace if you want to use local agents.
