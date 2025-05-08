# Clear Command

## Overview

The `/clear` command erases the conversation history for the current session. It provides a way to start fresh without exiting the application.

## Command Details

- **Name**: `clear`
- **Description**: Clear the conversation history
- **Usage**: `/clear`
- **Requires Confirmation**: No

## Functionality

The Clear command:

1. **Erases Conversation History**: Removes all previous messages from the current conversation.

2. **Maintains Context Files**: Unlike quitting and restarting, the clear command preserves any context files that have been added to the session.

3. **Resets Conversation State**: Resets the conversation state to its initial state, as if starting a new conversation.

## Implementation Details

The Clear command is implemented as a `ClearCommand` handler that implements the `CommandHandler` trait. Key implementation features include:

1. **No Confirmation Required**: The command executes immediately without requiring confirmation.

2. **Transcript Handling**: The command properly handles the conversation transcript, ensuring it's completely cleared.

3. **State Reset**: The conversation state is reset while maintaining other session settings.

## Example Usage

```
/clear
```

Output:
```
Conversation history cleared.
```

After execution, the conversation history is erased, and the user can start a fresh conversation while maintaining any context files and settings.

## Related Commands

- `/quit`: Exits the application completely
- `/compact`: Summarizes conversation history instead of clearing it completely

## Use Cases

- Start a new topic without the context of previous conversations
- Clear sensitive information from the conversation history
- Reset the conversation when it gets too long or goes off track
- Free up context window space without losing context files

## Notes

- The clear command does not remove context files
- Tool permissions and other settings are preserved
- This command is useful when you want to start fresh but don't want to exit and restart the application
