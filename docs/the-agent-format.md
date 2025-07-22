# The Agent Format

The agent configuration file for each agent is called its _manifest_. It is written in JSON format. It contains metadata and configuration needed to instantiate and run the agent.

Every manifest file consists of the following sections:

- [`name`](#the-name-field) — The name of the agent.
- [`version`](#the-version-field) — The version of the agent.
- [`description`](#the-description-field) — A description of the agent.
- [`mcpServers`](#the-mcp-servers-field) — The MCP servers the agent has access to.
- [`tools`](#the-tools-field) — The tools available to the agent.
- [`allowedTools`](#the-allowed-tools-field) — Tools that can be used without prompting.
- [`toolsSettings`](#the-tools-settings-field) — Configuration for specific tools.
- [`resources`](#the-resources-field) — Resources available to the agent.
- [`hooks`](#the-hooks-field) — Commands ran at specific trigger points.

### The `name` field

The `name` field specifies the name of the agent. This is used for identification and display purposes.

The name must use only [alphanumeric] characters or `-`, and cannot be empty.

- Only ASCII characters are allowed.
- Do not use reserved names.
- Do not use special Windows names such as "nul".
- Use a maximum of 64 characters of length.

### The `version` field

The `version` field is formatted according to the [SemVer] specification:

Versions must have three numeric parts,
the major version, the minor version, and the patch version.

A pre-release part can be added after a dash such as `1.0.0-alpha`.
The pre-release part may be separated with periods to distinguish separate
components. Numeric components will use numeric comparison while
everything else will be compared lexicographically.
For example, `1.0.0-alpha.11` is higher than `1.0.0-alpha.4`.

### The `description` field

The `description` field provides a description of what the agent does to be read by both humans and machines. It's important that descriptions succinctly define an agent behavior, as these descriptions take up LLM context when used as tools.

### The `mcpServers` field

The `mcpServers` field specifies which MCP servers the agent has access to. MCP servers can be either local or remote.

**Local servers** are defined with a command and transport configuration:

```json
{
  "type": "object",
  "properties": {
    "command": {
      "type": "string",
      "description": "The command to execute to start the MCP server"
    },
    "args": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Arguments to pass to the command"
    }
  },
  "required": ["command", "transport"]
}
```

**Complete example:**

```json
{
  "mcpServers": {
    "fetch": {
      "command": "fetch",
      "args": [],
      "transport": "stdio"
    },
    "git": {
      "command": "git-mcp",
      "transport": "stdio"
    }
  }
}
```

### The `tools` field

The `tools` field lists all tools that the agent can potentially use. Tools from MCP servers are prefixed with `@`.

Native tools can be optionally prefixed with `@native`.

**Note: A list of all the native tools can be found [here](./tools.md).**

```json
{
  "tools": [
    "fs-read",
    "@native/execute-bash",
    "@git",
    "@my-enterprise-mcp/read-internal-website"
  ]
}
```

### The `allowedTools` field

The `allowedTools` field specifies which tools can be used without prompting the user for permission. Allowed tools can be a superset of tools, as this permission is checked on tool execution.

```json
{
  "allowedTools": ["fs-read", "@git/git-status", "@my-enterprise-mcp"]
}
```

### The `toolsSettings` field

The `toolsSettings` field provides configuration for specific tools. Each tool has a unique configuration that can only be known by checking documentation for the tool. For native tool configuration, please refer to [this section of the docs](./tools.md).

```json
{
  "toolsSettings": {
    "fs-write": {
      "allowedPaths": [".", "/var/www/**"]
    },
    "@my-enterprise-mcp.my-tool": {
      "some-configuration-value": true
    }
  }
}
```

### The `resources` field

The `resources` field gives an agent access to local resources.

Resources are data and content that your agent has access to. As of today, only local file resources are supported, however the intention is to support surfacing MCP resources to agents as well.

```json
{
  "resources": [
    "file://AmazonQ.md",
    "file://~/Documents/my-repo/README.md",
    "file:///tmp/my-application-logs.txt"
  ]
}
```

### The `hooks` field

The `hooks` field invokes functions at specific trigger points. The output of these functions is persisted into context according to the lifetime rules of the trigger. Hooks that run once, on `agentSpawn` for example, will be added to persistent context while shorter lived hooks like `userPromptSubmit` exist inside of the rolling context window.  

```json
{
  "hooks": {
    "agentSpawn": {
      "command": "git status",
    },
    "userPromptSubmit": {
      "command": "ls",
    }
  }
}
```

## Complete Example

Here's a complete example of an agent manifest:

```json
{
  "name": "rust-developer-agent",
  "version": "1.2.0",
  "description": "A specialized agent for Rust development tasks",
  "mcpServers": {
    "fetch": { "command": "fetch", "args": [], "transport": "stdio" },
    "git": { "command": "git-mcp", "transport": "stdio" },
    "rust-analyzer": { "command": "rust-analyzer-mcp", "transport": "stdio" }
  },
  "tools": [
    "fs-read",
    "fs-write",
    "execute-bash",
    "@git",
    "@rust-analyzer/check-code",
    "@rust-analyzer/format-code"
  ],
  "allowedTools": ["fs-read", "@git.git-status", "@rust-analyzer/check-code"],
  "toolsSettings": {
    "fs-write": {
      "allowedPaths": ["src/**", "tests/**", "Cargo.toml"]
    }
  }
}
```
