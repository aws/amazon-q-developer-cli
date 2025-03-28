- Feature Name: compact_command
- Start Date: 2025-03-26

# Summary

[summary]: #summary

Add a `/compact` command to the Amazon Q Developer CLI chat interface that allows users to compact their conversation history to save context space while preserving the essential information.

# Motivation

[motivation]: #motivation

Large language models like those powering Amazon Q have context limits that restrict how much conversation history can be maintained. As conversations grow longer, they consume more of this limited context space, which can lead to:

1. Degraded response quality as the model loses access to earlier parts of the conversation
2. Increased token usage and associated costs
3. Slower response times due to processing larger context windows

The `/compact` command addresses these issues by providing users with a way to reduce the size of their conversation history while preserving the most important information. This allows for longer, more productive conversations without hitting context limits.

# Guide-level explanation

[guide-level-explanation]: #guide-level-explanation

## Basic Usage

Users can type `/compact` during a chat session to trigger the compaction process. The command will:

1. Analyze the current conversation
2. Create a condensed summary of previous exchanges
3. Replace the verbose history with this summary
4. Inform the user about how much context space was saved

Example interaction:

```
User: /compact
Amazon Q: I've compacted our conversation history, preserving the key points while reducing the context size by approximately 65%. We can continue our discussion with more context space available.
```

## Options

The `/compact` command supports optional parameters:

- `/compact` - Default behavior, balances preserving previous messages and summarization
- `/compact --aggressive [--summary]` - Maximizes context savings with more aggressive summarization
- `/compact --conservative [--summary]` - Preserves more detail with lighter summarization
- `/compact --summary` - Shows what the compacted messages look like, without actually performing the operation

## Understanding Context Usage

When users run `/usage` (new command to be added), they will see:
- Current context usage (tokens/percentage)
- Available context space
- Estimated conversation turns remaining

This helps users decide when to use the `/compact` command.

# Reference-level explanation

[reference-level-explanation]: #reference-level-explanation

## Implementation Details

The `/compact` command will be integrated into the existing chat module structure in `q_cli/src/cli/chat/`, specifically:

1. Add command handling in `command.rs`:
```rust
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    // ... existing commands ...
    Compact {
        mode: CompactMode,
        show_only: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactMode {
    Default,
    Aggressive,
    Conservative,
}
```


2. Extend `ConversationState` in `conversation_state.rs`:
```rust
impl ConversationState {
    /// Compacts the conversation history based on the specified mode
    pub fn compact(&mut self, mode: CompactMode) -> Result<CompactionStats> {
        let original_size = self.estimate_token_count();
        
        match mode {
            CompactMode::Default => self.compact_default(),
            CompactMode::Aggressive => self.compact_aggressive(),
            CompactMode::Conservative => self.compact_conservative(),
        }
    }

}
```

3. Add compaction strategies and configuration:
```rust
/// Configuration for conversation compaction
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Number of recent messages to keep unchanged
    pub keep_recent: usize,
    /// Level of detail to preserve in the summary
    pub detail_level: DetailLevel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailLevel {
    Minimal,   // Very concise, focus only on critical information
    Standard,  // Balanced approach with moderate detail
    Detailed,  // Preserve more nuance and context
}

impl ConversationState {
    fn compact_default(&mut self) -> Result<CompactionStats> {
        // Keep last 5 exchanges, use standard summarization
        self.compact_with_config(CompactionConfig {
            keep_recent: 5,
            detail_level: DetailLevel::Standard,
        })
    }

    fn compact_aggressive(&mut self) -> Result<CompactionStats> {
        // Keep last 3 exchanges, use minimal summarization
        self.compact_with_config(CompactionConfig {
            keep_recent: 3,
            detail_level: DetailLevel::Minimal,
        })
    }

    fn compact_conservative(&mut self) -> Result<CompactionStats> {
        // Keep last 8 exchanges, use detailed summarization
        self.compact_with_config(CompactionConfig {
            keep_recent: 8,
            detail_level: DetailLevel::Detailed,
        })
    }
}

## Compaction Implementation

The core of the implementation is the `compact_with_config` method, which:

1. Preserves recent messages based on the configuration
2. Identifies and preserves messages with tool uses/results
3. Summarizes the remaining messages using Amazon Q
4. Creates a new conversation history with the summary and preserved messages
5. Returns statistics about the compaction process

The implementation will carefully handle edge cases such as empty conversations and ensure that important context (like tool executions) is never lost during compaction.

## Summarization Approach

The summarization uses different detail levels to control how concise or comprehensive the summary should be:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailLevel {
    Minimal,   // Very concise, focus only on critical information
    Standard,  // Balanced approach with moderate detail
    Detailed,  // Preserve more nuance and context
}
```

Each detail level corresponds to different instructions sent to the model:

- **Minimal**: "Be extremely concise, focusing only on the most critical information. Prioritize brevity over detail, but ensure all key points are captured."

- **Standard**: "Create a balanced summary that preserves important details while being concise. Include key questions, answers, and decisions."

- **Detailed**: "Create a comprehensive summary that preserves nuance and context. Include most significant details while still condensing the conversation."

All prompts include instructions to focus on:
1. Key questions asked by the user
2. Important information provided by the assistant
3. Any decisions or conclusions reached
4. Specific technologies, services, or code concepts
5. Preserving essential code snippets


4. Handle special content:
```rust
impl ConversationState {
    fn should_preserve_message(&self, msg: &ChatMessage) -> bool {
        match msg {
            ChatMessage::UserInputMessage(m) => {
                // Preserve messages with tool results
                if let Some(ctx) = &m.user_input_message_context {
                    if let Some(results) = &ctx.tool_results {
                        return !results.is_empty();
                    }
                }
                false
            }
            ChatMessage::AssistantResponseMessage(m) => {
                // Preserve messages with tool uses
                if let Some(uses) = &m.tool_uses {
                    return !uses.is_empty();
                }
                false
            }
        }
    }
}
```

## Integration with Existing Components

The implementation leverages several existing components:

1. Uses the `MAX_CONVERSATION_STATE_HISTORY_LEN` constant (100) for context limits
2. Integrates with the existing message history management in `ConversationState`
3. Preserves tool usage history and results
4. Maintains compatibility with context files system

## Compaction Algorithm

The compaction process follows these steps:

1. Identify messages to preserve:
   - Recent exchanges (configurable number)
   - Messages with tool uses/results
   - Critical information (code snippets, errors)

2. Group remaining messages by topic/thread:
   ```rust
   struct ConversationThread {
       topic: String,
       messages: Vec<ChatMessage>,
       importance_score: f32,
   }
   ```

3. Generate summaries:
   - Use Amazon Q to create concise summaries
   - Preserve code blocks and important details
   - Include reference to original message count

4. Update conversation history:
   - Replace old messages with summaries
   - Maintain chronological order
   - Update metadata and tracking

## Error Handling

The implementation includes comprehensive error handling:

```rust
#[derive(Debug, thiserror::Error)]
pub enum CompactionError {
    #[error("Failed to generate summary: {0}")]
    SummaryGenerationFailed(String),
    
    #[error("Invalid compaction mode")]
    InvalidMode,
    
    #[error("No messages to compact")]
    EmptyHistory,
}
```

# Drawbacks

[drawbacks]: #drawbacks

1. **Information Loss**: Compaction inherently involves some loss of detail from the original conversation
2. **Additional Complexity**: Adds more commands and options for users to learn
3. **Processing Overhead**: Generating summaries requires additional API calls to the Amazon Q service
4. **Potential Confusion**: Users might be confused about what information is retained after compaction

# Rationale and alternatives

[rationale-and-alternatives]: #rationale-and-alternatives

## Why This Design

This design aligns with the existing codebase by:

1. Following the established command pattern in `command.rs`
2. Integrating with `ConversationState` management
3. Preserving tool execution history
4. Supporting the context files system
5. Maintaining consistent error handling patterns

## Alternatives Considered

1. **Automatic Compaction**: Automatically compact when reaching `MAX_CONVERSATION_STATE_HISTORY_LEN`
   - Pro: No user action required
   - Con: Users lose control over when and how compaction occurs
   - Rationale for not choosing: Users should have explicit control over their conversation history

2. **Selective Deletion**: Allow users to delete specific messages
   - Pro: More precise control over what's removed
   - Con: More complex UI and user effort required
   - Rationale for not choosing: Too cumbersome for most use cases

3. **Context Windowing**: Only keep a fixed window of recent messages
   - Pro: Simple implementation
   - Con: Loses important context from earlier in the conversation
   - Rationale for not choosing: Too blunt an approach that discards potentially valuable information

## Impact of Not Doing This

Without this feature:
1. Users will continue to hit context limits in longer conversations
2. The utility of the chat interface will be limited for complex, multi-turn interactions
3. Users may need to manually start new conversations and repeat context, leading to frustration

# Unresolved questions

[unresolved-questions]: #unresolved-questions

1. What is the optimal default compaction strategy that balances context savings with information preservation?
2. How should we handle code snippets and other structured content during compaction?
3. Should we provide visual indicators in the chat UI to show which parts of the conversation have been compacted?
4. What metrics should we collect to evaluate the effectiveness of the compaction feature?
5. How do we handle compaction when the conversation includes tool usage and their outputs?

# Future possibilities

[future-possibilities]: #future-possibilities

1. **Smart Compaction**: Use ML to identify which parts of the conversation are most important to preserve
2. **Scheduled Compaction**: Allow users to set automatic compaction rules (e.g., compact after X messages or Y minutes)
3. **Hierarchical Summaries**: Create multi-level summaries that users can expand/collapse
4. **Topic-Based Compaction**: Group and compact conversation segments by topic
5. **Exportable Summaries**: Allow users to export a compacted version of their conversation
6. **Visual Timeline**: Provide a visual representation of the conversation with indicators for compacted sections
7. **Customizable Retention Rules**: Let users specify patterns or content types that should never be compacted
8. **Integration with Other Commands**: Allow compaction to be combined with other commands
