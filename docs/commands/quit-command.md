# Quit Command

## Overview

The `/quit` command allows users to exit the Amazon Q CLI application. It provides a clean way to terminate the current session.

## Command Details

- **Name**: `quit`
- **Description**: Exit the Amazon Q CLI application
- **Usage**: `/quit`
- **Requires Confirmation**: Yes

## Functionality

The Quit command:

1. **Prompts for Confirmation**: Before exiting, the command asks the user to confirm they want to quit.

2. **Terminates the Application**: If confirmed, the application exits cleanly, closing the current session.

## Implementation Details

The Quit command is implemented as a `QuitCommand` handler that implements the `CommandHandler` trait. Key implementation features include:

1. **Confirmation Required**: The command requires user confirmation before execution to prevent accidental exits.

2. **Clean Termination**: The command ensures a clean termination of the application by setting the appropriate exit state.

## Example Usage

```
/quit
```

Output:
```
Are you sure you want to quit? [y/N]: 
```

If the user enters 'y' or 'Y', the application exits. Otherwise, the command is cancelled.

## Related Commands

- `/clear`: Clears the conversation history without exiting the application

## Use Cases

- End the current Amazon Q CLI session
- Exit the application when finished using it
- Terminate the program cleanly

## Notes

- The quit command always requires confirmation to prevent accidental exits
- Alternative ways to exit (like Ctrl+C or Ctrl+D) may also be available depending on the terminal
- The command ensures a clean exit, properly closing any open resources
