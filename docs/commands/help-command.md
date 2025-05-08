# Help Command

## Overview

The `/help` command displays information about available commands in the Amazon Q CLI. It provides users with a quick reference to understand what commands are available and how to use them.

## Command Details

- **Name**: `help`
- **Description**: Display help information about available commands
- **Usage**: `/help`
- **Requires Confirmation**: No (read-only command)

## Functionality

The Help command provides a general overview of all available commands with brief descriptions. It displays a formatted list of commands that can be used in the Amazon Q CLI.

## Implementation Details

The Help command is implemented as a `HelpCommand` handler that implements the `CommandHandler` trait. Key implementation features include:

1. **Trusted Command**: The help command is marked as trusted, meaning it doesn't require confirmation before execution.

2. **Static Help Text**: The help command uses a static help text constant that lists all available commands.

3. **Formatted Output**: The help text is formatted with colors and sections to improve readability.

## Example Usage

```
/help
```

Output:
```
Available commands:

/help                      Display this help message
/quit                      Exit the application
/clear                     Clear the conversation history
/context                   Manage context files
/profile                   Manage profiles
/tools                     Manage tool permissions
/compact                   Summarize conversation history
/usage                     Display token usage statistics
/issue                     Create a GitHub issue
```

## Related Commands

All other commands in the system are listed in the help output.

## Use Cases

- Learn about available commands
- Get a quick overview of command functionality
- Discover what commands are available in the system

## Notes

- The help command is always available and doesn't require any special permissions
- Help text is designed to be concise yet informative
- Color formatting is used to improve readability when supported by the terminal
