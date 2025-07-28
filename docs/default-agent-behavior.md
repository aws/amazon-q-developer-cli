# Default Agent Behavior

When no specific agent is configured or when the specified agent cannot be found, Q CLI follows a fallback hierarchy to determine which agent to use.

## Agent Selection Priority

Q CLI selects an agent in the following order of priority:

### 1. Command-Line Specified Agent
The agent specified via the `--agent` flag when starting Q CLI:

```bash
q chat --agent my-custom-agent
```

If this agent exists, it will be used. If not, Q CLI will display an error and fall back to the next option.

### 2. User-Defined Default Agent
The default agent configured via the settings system:

```bash
q settings chat.defaultAgent my-preferred-agent
```

This setting is stored in your Q CLI configuration and will be used across all sessions unless overridden by the `--agent` flag.

If the configured default agent cannot be found, Q CLI will display an error and fall back to the built-in default.

### 3. Built-in Default Agent
If no agent is specified or found, Q CLI uses a built-in default agent with the following configuration:

```json
{
  "name": "default",
  "description": "Default agent",
  "tools": ["*"],
  "allowedTools": ["fs_read"],
  "resources": [
    "file://AmazonQ.md",
    "file://README.md", 
    "file://.amazonq/rules/**/*.md"
  ],
  "useLegacyMcpJson": true
}
```

## Built-in Default Agent Details

The built-in default agent provides:

### Available Tools
- **All tools**: Uses `"*"` wildcard to include all built-in tools and MCP server tools

### Trusted Tools
- **fs_read only**: Only the `fs_read` tool is pre-approved and won't prompt for permission
- All other tools will require user confirmation before execution

### Default Resources
The agent automatically includes these files in its context (if they exist):
- `AmazonQ.md` - Amazon Q documentation or notes
- `README.md` - Project readme file
- `.amazonq/rules/**/*.md` - Any markdown files in the `.amazonq/rules/` directory and subdirectories

### Legacy MCP Support
- **Enabled**: The default agent includes MCP servers from the legacy global configuration file (`~/.aws/amazonq/mcp.json`)

## Error Messages

When agent fallback occurs, you'll see informative messages:

### Agent Not Found
```
Error: no agent with name my-agent found. Falling back to user specified default
```

### User Default Not Found
```
Error: user defined default my-default not found. Falling back to in-memory default
```

## Customizing Default Behavior

### Set a User Default Agent
To avoid using the built-in default, configure your preferred agent:

```bash
q settings chat.defaultAgent my-preferred-agent
```

### Override for Specific Sessions
Use the `--agent` flag to specify an agent for a particular session:

```bash
q chat --agent specialized-agent
```

### Create a Custom Default
You can create your own "default" agent by placing a file named `default.json` in either:
- `.aws/amazonq/agents/default.json` (local)
- `~/.aws/amazonq/agents/default.json` (global)

This will override the built-in default agent configuration.

## Best Practices

1. **Set a user default**: Configure a default agent that matches your typical workflow
2. **Use descriptive names**: Choose agent names that clearly indicate their purpose
3. **Test agent availability**: Ensure your default agent exists and is accessible
4. **Document team agents**: If sharing agents with a team, document which agents should be used for different tasks

## Example Workflow

```bash
# Set up a preferred default agent
q settings chat.defaultAgent development-helper

# Use default agent (development-helper)
q chat

# Override for specific task
q chat --agent aws-specialist

# If development-helper doesn't exist, falls back to built-in default
# with warning message
```
