# RFC: Model Context Protocol (MCP) Lifecycle Support for Amazon Q CLI

- Name: MCP Lifecycle Support for Amazon Q CLI
- Start Date: 2025-04-01

## Summary

This RFC proposes adding MCP server management commands to the Amazon Q Developer CLI. The proposal includes commands for adding, removing, listing, and updating MCP servers, with appropriate scoping and security considerations.

## Motivation

Amazon Q Developer CLI currently provides powerful AI assistance for developers, but its capabilities are limited to built-in tools. It also provides initial support for MCP servers by requiring users to directly modify the `mcp.json` file to configure the MCP servers:

This enhancement will improve the overal configuration for the Q CLI but providing a set of commands to manage MCP servers.

## Concepts

- **MCP Server** is an external service that provides additional capabilities to Amazon Q. A preview support for MCP servers is offered in v1.7.2 of Amazon Q CLI.

- **Scope**: Determines where MCP server configurations are stored:
  - `local`: Available only in the current project
  - `global`: Available to the current user across all projects

### Command Structure

We will add a new subcommand to the Amazon Q CLI:

```
q mcp [subcommand] [options]
```

#### Subcommands

1. **add**: Add a new MCP server
   ```
   q mcp add [--name NAME] [--url URL] [--scope SCOPE] [--env KEY=VALUE...]
   ```

2. **remove**: Remove an MCP server
   ```
   q mcp remove [--name NAME] [--scope SCOPE]
   ```

3. **list**: List configured MCP servers
   ```
   q mcp list [--scope SCOPE]
   ```

4. **update**: Update an existing MCP server
   ```
   q mcp update [--name NAME] [--url URL] [--scope SCOPE] [--env KEY=VALUE...]
   ```

5. **import**: Import MCP servers from JSON configuration
   ```
   q mcp import [--file FILE] [--scope SCOPE]
   ```

6. **status**: Check status of MCP servers
   ```
   q mcp status [--name NAME]
   ```

### Configuration Storage

MCP server configurations will be stored in:

- `local` scope: `.amazonq/mcp.json` in the current directory
- `global` scope: `~/.aws/amazonq/mcp.json` in the user's home directory

The configuration file format will be JSON:

```json
{
  "servers": [
    {
      "name": "aws-cli",
      "url": "http://localhost:8080",
      "env": {
        "AWS_ACCESS_KEY_ID": "AKIAIOSFODNN7EXAMPLE_DUMMY_KEY_ID",
        "AWS_SECRET_ACCESS_KEY": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY_DUMMY_SECRET",
        "AWS_REGION": "us-west-2"
      }
    }
  ]
}
```

### Security Considerations

1. **Environment Variables**: Sensitive information like API keys can be provided as environment variables
2. **Timeout Configuration**: Users can configure MCP server startup timeout using the `Q_MCP_TIMEOUT` environment variable
3. **Scope Precedence**: Local-scoped servers take precedence over global-scoped servers with the same name

### Integration with Chat

MCP servers will be automatically available in chat sessions:

```
$ q chat
> Use the aws-cli server to list my S3 buckets

Using aws-cli MCP server...
Executing AWS CLI command: aws s3 ls
2023-01-15 14:32:12 example-bucket-1
2023-02-20 09:15:45 example-bucket-2
2023-03-10 16:08:30 example-bucket-3
...
```

Users can check MCP server status during a chat session using the `/mcp` command.

## Examples

### Adding an AWS CLI MCP Server

```bash
$ q mcp add --name aws-cli --url http://localhost:8080 --env AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE_DUMMY_KEY_ID --env AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY_DUMMY_SECRET --env AWS_REGION=us-west-2
✓ Added MCP server 'aws-cli' to local scope
```

### Listing Available MCP Servers

```bash
$ q mcp list
  aws-cli (http://localhost:8080)
```

### Using MCP in Chat

```bash
$ q chat
> Can you show me the objects in my S3 bucket example-bucket-1?

I'll use the aws-cli MCP server to get that information for you.

Contents of S3 bucket 'example-bucket-1':

| Last Modified          | Size     | Key                    |
|------------------------|----------|------------------------|
| 2025-03-15 10:30:45    | 2.5 MB   | documents/report.pdf   |
| 2025-03-16 14:22:10    | 1.2 MB   | images/logo.png        |
| 2025-03-18 09:15:33    | 4.7 MB   | backups/data.zip       |
| 2025-03-20 16:45:12    | 512 KB   | config/settings.json   |

Would you like me to help you analyze or download any of these files?
```

### Importing MCP Servers from Configuration

```bash
$ q mcp import --file mcp-servers.json --scope global
✓ Imported 3 MCP servers to global scope
```

### Checking MCP Server Status in Chat

```bash
$ q chat
> /mcp aws-cli

MCP Server: aws-cli (http://localhost:8080)
Status: Connected
Uptime: 1h 23m
Available Tools: 12
Last Request: 2m ago

Available actions:
- restart: Restart the server
- stop: Stop the server
- logs: View recent logs
```

## Unresolved Questions

1. How should we handle version compatibility between Amazon Q CLI and MCP servers?
2. What security measures should be in place for approving third-party MCP servers?
3. Should we provide a marketplace or registry for discovering public MCP servers?

## Future Work

1. Develop a library of common MCP servers for popular developer tools
2. Create a simplified MCP server creation framework for AWS services
3. Implement a discovery mechanism for MCP servers
4. Add support for MCP server authentication methods beyond environment variables
