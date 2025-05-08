# Amazon Q CLI Commands

This section documents the commands available in the Amazon Q CLI. These commands help you interact with the CLI and manage your conversation context, profiles, and tools.

## Available Commands

| Command | Description |
|---------|-------------|
| `/help` | Display help information about available commands |
| `/quit` | Exit the Amazon Q CLI application |
| `/clear` | Clear the current conversation history |
| `/context` | Manage context files for the conversation |
| `/profile` | Manage Amazon Q profiles |
| `/tools` | Manage tool permissions and settings |
| `/issue` | Create a GitHub issue with conversation context |
| `/compact` | Summarize conversation history to free up context space |
| `/usage` | Display token usage statistics |
| `/editor` | Open an external editor for input |

## Command Registry

The Amazon Q CLI uses a command registry system to manage commands. This architecture provides several benefits:

1. **Consistent Behavior**: Commands behave the same whether invoked directly or through natural language
2. **Extensibility**: New commands can be added easily by implementing the `CommandHandler` trait
3. **Separation of Concerns**: Each command's logic is encapsulated in its own handler
4. **Natural Language Support**: Commands can be invoked using natural language through the `internal_command` tool

## Using Commands

Commands can be invoked in two ways:

1. **Direct Invocation**: Type the command directly in the CLI, e.g., `/usage`
2. **Natural Language**: Ask Amazon Q to perform the action, e.g., "Show me my token usage"

## Command Documentation

Each command has its own documentation page with details on:

- Command syntax and arguments
- Examples of usage
- Implementation details
- Related commands
- Use cases and best practices
