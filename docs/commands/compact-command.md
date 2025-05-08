# Compact Command

## Overview

The `/compact` command summarizes the conversation history to free up context space while preserving essential information. This is useful for long-running conversations that may eventually reach memory constraints.

## Command Details

- **Name**: `compact`
- **Description**: Summarize conversation history to free up context space
- **Usage**: `/compact [prompt] [--summary]`
- **Requires Confirmation**: No

## Functionality

The Compact command:

1. **Summarizes Conversation**: Creates a concise summary of the conversation history.

2. **Preserves Essential Information**: Maintains the key points and context from the conversation.

3. **Frees Up Context Space**: Reduces the token count used by the conversation history, allowing for longer conversations.

4. **Optional Custom Guidance**: Accepts an optional prompt parameter to guide the summarization process.

5. **Summary Display Option**: Can show the generated summary when the `--summary` flag is used.

## Implementation Details

The Compact command is implemented as a `CompactCommand` handler that implements the `CommandHandler` trait. Key implementation features include:

1. **AI-Powered Summarization**: Uses the AI model to generate a meaningful summary of the conversation.

2. **Conversation State Management**: Properly updates the conversation state with the summary.

3. **Optional Parameters**: Supports custom prompts and flags to control the summarization process.

## Example Usage

### Basic Usage

```
/compact
```

Output:
```
Summarizing conversation history...
Conversation history has been summarized.
```

### With Custom Prompt

```
/compact Focus on the technical aspects of our discussion
```

Output:
```
Summarizing conversation history with custom guidance...
Conversation history has been summarized.
```

### With Summary Display

```
/compact --summary
```

Output:
```
Summarizing conversation history...
Conversation history has been summarized.

Summary:
In this conversation, we discussed the implementation of the command registry system. 
We covered the migration of basic commands (help, quit, clear) and the implementation 
of the usage command with visual progress bars. We also decided to leverage the existing 
report_issue tool for the issue command rather than creating a separate handler.
```

## Related Commands

- `/clear`: Completely erases conversation history instead of summarizing it
- `/usage`: Shows token usage statistics to help decide when compacting is needed

## Use Cases

- Continue a long conversation that's approaching token limits
- Preserve key information while reducing context size
- Free up space for adding more context files
- Maintain conversation flow without starting over

## Notes

- Compacting is more space-efficient than clearing when you want to maintain context
- The quality of the summary depends on the AI model's summarization capabilities
- Custom prompts can help focus the summary on specific aspects of the conversation
- The `--summary` flag is useful to verify what information has been preserved
