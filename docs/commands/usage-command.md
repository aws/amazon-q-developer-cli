# Usage Command

## Overview

The `/usage` command provides users with a visual representation of their token usage in the conversation. It helps users understand how much of the context window is being utilized and when they might need to use the `/compact` command to free up space.

## Command Details

- **Name**: `usage`
- **Description**: Display token usage statistics
- **Usage**: `/usage`
- **Requires Confirmation**: No (read-only command)

## Functionality

The Usage command calculates and displays:

1. **Token usage for conversation history**: Shows how many tokens are used by the conversation history and what percentage of the maximum capacity this represents.

2. **Token usage for context files**: Shows how many tokens are used by context files and what percentage of the maximum capacity this represents.

3. **Total token usage**: Shows the combined token usage and percentage of maximum capacity.

4. **Remaining and maximum capacity**: Shows how many tokens are still available and the total capacity.

## Visual Representation

The command uses color-coded progress bars to visually represent token usage:

- **Green**: Less than 50% usage
- **Yellow**: Between 50-75% usage
- **Red**: Over 75% usage

## Example Output

```
📊 Token Usage Statistics

Conversation History: █████████░░░░░░░░░░░░░░░░░░░░░ 1234 tokens (30.0%)
Context Files:       ███░░░░░░░░░░░░░░░░░░░░░░░░░░░ 456 tokens (10.0%)
Total Usage:         ████████████░░░░░░░░░░░░░░░░░░ 1690 tokens (40.0%)

Remaining Capacity:   2310 tokens
Maximum Capacity:     4000 tokens
```

If usage is high (over 75%), the command also displays a tip:

```
Tip: Use /compact to summarize conversation history and free up space.
```

## Implementation Details

The Usage command is implemented as a `UsageCommand` handler that implements the `CommandHandler` trait. Key implementation features include:

1. **Token Calculation**: Uses the TOKEN_TO_CHAR_RATIO (3 characters per token) to convert character counts to token counts.

2. **Progress Bar Formatting**: Uses Unicode block characters to create visual progress bars.

3. **Color Coding**: Applies different colors based on usage percentages to provide visual cues about usage levels.

## Related Commands

- `/compact`: Use this command to summarize conversation history and free up space when token usage is high.
- `/context`: Manage context files, which contribute to token usage.

## Use Cases

- Check how much of the context window is being used
- Determine if you need to compact the conversation
- Understand the impact of adding context files
- Troubleshoot when responses seem truncated due to context limits

## Notes

- The context window has a fixed size limit
- When the window fills up, older messages may be summarized or removed
- Adding large context files can significantly reduce available space
- Use `/compact` to summarize conversation history and free up space
