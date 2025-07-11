# The Agent Format

The agent configuration file for each agent is called its _manifest_. It is written in JSON format. It contains metadata and configuration needed to instantiate and run the agent.

Every manifest file consists of the following sections:

- [`name`](#the-name-field) --- The name of the agent.
- [`version`](#the-version-field) --- The version of the agent.
- [`description`](#the-description-field) --- A description of the agent.
- [`model`](#the-model-field) --- The model the agent uses.
- [`inputSchema`](#the-input-schema-field) --- The input schema of the agent.
- [`mcpServers`](#the-mcp-servers-field) --- The MCP servers the agent has access to.
- [`tools`](#the-tools-field) --- The tools available to the agent.
- [`allowedTools`](#the-allowed-tools-field) --- Tools that can be used without prompting.
- [`toolsSettings`](#the-tools-settings-field) --- Configuration for specific tools.
- [`resources`](#the-resources-field) --- Declarative resources and context.

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

### The `model` field

The `model` field specifies which language model the agent should use.

Model identifiers can be found on the [AWS Bedrock documentation](https://docs.aws.amazon.com/bedrock/latest/userguide/models-supported.html).

As of now, the two supported models are

1. `anthropic.claude-3-7-sonnet-20250219-v1:0`
2. `anthropic.claude-sonnet-4-20250514-v1:0`

### The `inputSchema` field

The `inputSchema` field defines the input parameters when the agent is used as a tool. This schema takes the form of a JSON schema. Because agents can be executed as MCP tools, this aligns directly with the input schema of MCP tools themselves. Input parameters can be templated in the agent using the following syntax:

```
${my-input-parameter}
```

Note that this is the same syntax used for environment variables, in this way, environment variables act as user-defined inputs while input parameters service as machine inputs.

```json
{
  "inputSchema": {
    "type": "object",
    "properties": {
      "model-to-use": {
        "type": "string",
        "description": "The model to use for this agent invocation."
      }
    },
    "required": ["model-to-use"]
  }
}
```

Then you might do something like this:

```json
{
  "name": "my-agent",
  "model": ${model-to-use},
}
```

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
    },
    "transport": {
      "type": "string",
      "enum": ["stdio", "streamable-http"],
      "description": "The transport protocol to use"
    },
    "env": {
      "type": "object",
      "description": "Environment variables to set for the server"
    }
  },
  "required": ["command", "transport"]
}
```

**Remote servers** are defined with a URL:

```json
{
  "type": "object",
  "properties": {
    "url": {
      "type": "string",
      "format": "uri",
      "description": "The URL of the remote MCP server"
    },
    "headers": {
      "type": "object",
      "description": "HTTP headers to include in requests"
    },
    "auth": {
      "type": "object",
      "description": "Authentication configuration"
    }
  },
  "required": ["url"]
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
      "transport": "streamable-http",
      "env": {
        "GIT_CONFIG_GLOBAL": "/dev/null"
      }
    },
    "github-mcp": {
      "url": "https://api.githubcopilot.com/mcp",
      "headers": {
        "Authorization": "Bearer ${GITHUB_TOKEN}"
      }
    }
  }
}
```

### The `tools` field

The `tools` field lists all tools that the agent can potentially use. Tools from MCP servers are prefixed with `@`, while agents themselves are prefixed with `#`.

Native tools can be optionally prefixed with `@native`.

**Note: A list of all the native tools can be found [here](./tools.md).**

```json
{
  "tools": [
    "fs-read",
    "@native/execute-bash",
    "@git",
    "@my-enterprise-mcp/read-internal-website",
    "#my-workspace-agent"
  ]
}
```

### The `allowedTools` field

The `allowedTools` field specifies which tools can be used without prompting the user for permission. Allowed tools can be a superset of tools, as this permission is checked on tool execution.

```json
{
  "allowedTools": [
    "fs-read",
    "@git/git-status",
    "@my-enterprise-mcp",
    "#my-workspace-agent"
  ]
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

The `resources` field defines declarative resources that provide context to the agent. At this time, these resources are handled uniquely by individual client implementations.

Resources can be simple strings to enable lightweight prompting, but otherwise align with MCP resources. For more information on MCP resources, please refer to [the MCP documentation](https://modelcontextprotocol.io/docs/concepts/resources).

```json
{
  "resources": [
    "You are a principal engineer who writes rust backend code using the write_code and git tools.",
    "file://my-excellent-prompt.md",
    "file://${workspace}",
    "file://my-mcp-resource.json@builder-mcp"
  ]
}
```

## Complete Example

Here's a complete example of an agent manifest:

```json
{
  "name": "rust-developer-agent",
  "version": "1.2.0",
  "description": "A specialized agent for Rust development tasks",
  "model": "anthropic.claude-sonnet-4-20250514-v1:0",
  "mcpServers": {
    "fetch": { "command": "fetch", "args": [], "transport": "stdio" },
    "git": { "command": "git-mcp", "transport": "streamable-http" },
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
  },
  "resources": [
    "You are a principal Rust engineer who writes safe, efficient backend code.",
    "file://rust-style-guide.md",
    "file://${workspace}/README.md",
    "file://project-context.md@workspace-mcp"
  ],
  "inputSchema": {
    "type": "object",
    "properties": {
      "task": {
        "type": "string",
        "description": "The development task to perform"
      },
      "context": {
        "type": "object",
        "description": "Additional context for the task"
      }
    },
    "required": ["task"]
  }
}
```
