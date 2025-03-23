# History Overflow Handler Specification

## Problem Statement

Currently, when the conversation history exceeds the maximum length (`MAX_CONVERSATION_STATE_HISTORY_LEN - 2`) and no suitable user message is found to keep, the system automatically clears the entire history without user consent. This can lead to a poor user experience as important context might be lost without warning.

## Proposed Solution

Modify the `fix_history()` method to give users control over how to handle history overflow. Instead of automatically clearing the history, we'll present users with options to:

1. **Compact**: Summarize the history and continue
2. **Reset**: Clear the history and start fresh
3. **Continue**: Keep the history as is (with a warning)

We'll leverage and rename the existing `large_tool_result_handler.rs` to `history_overflow_handler.rs` to implement this functionality.

## Implementation Details

### 1. Rename and Refactor `large_tool_result_handler.rs`

Rename `large_tool_result_handler.rs` to `history_overflow_handler.rs` and refactor it to handle both tool result overflow and history overflow scenarios:

```rust
// history_overflow_handler.rs
pub struct HistoryOverflowHandler<'a, W: Write> {
    // Same fields as before
}

impl<'a, W: Write> HistoryOverflowHandler<'a, W> {
    // Constructor remains the same
    
    // Method for handling large tool results (keep existing functionality)
    pub async fn handle_large_tool_result(...) -> Result<ChatState, ChatError> {
        // Existing implementation
    }
    
    // New method for handling history overflow
    pub async fn handle_history_overflow(
        &mut self,
        conversation_state: &mut ConversationState,
    ) -> Result<ChatState, ChatError> {
        // Present options to the user
        // Process their choice
    }
    
    // Implementation of the three strategies (compact, reset, continue)
    // These can be shared between tool result and history overflow handling
}
```

### 2. Modify `fix_history()` in `conversation_state.rs`

Update the `fix_history()` method to return a boolean indicating if history overflow handling is needed:

```rust
pub fn fix_history(&mut self) -> bool {
    // Return value indicates if history overflow handling is needed
    let mut needs_handling = false;
    
    if self.history.len() > MAX_CONVERSATION_STATE_HISTORY_LEN - 2 {
        match self
            .history
            .iter()
            .enumerate()
            // Skip the first message which should be from the user.
            .skip(1)
            .find(|(_, m)| -> bool {
                match m {
                    ChatMessage::UserInputMessage(m) => {
                        matches!(
                            m.user_input_message_context.as_ref(),
                            Some(ctx) if ctx.tool_results.as_ref().is_none_or(|v| v.is_empty())
                        ) && !m.content.is_empty()
                    },
                    ChatMessage::AssistantResponseMessage(_) => false,
                }
            })
            .map(|v| v.0)
        {
            Some(i) => {
                debug!("removing the first {i} elements in the history");
                self.history.drain(..i);
            },
            None => {
                // Instead of automatically clearing, set the flag to indicate handling is needed
                debug!("no valid starting user message found in the history, needs handling");
                needs_handling = true;
            },
        }
    }
    
    // Rest of the method remains the same
    // ...
    
    needs_handling
}
```

### 3. Update `as_sendable_conversation_state()` in `conversation_state.rs`

Modify the method to check if history overflow handling is needed:

```rust
pub async fn as_sendable_conversation_state(&mut self) -> Result<FigConversationState, HistoryOverflowError> {
    debug_assert!(self.next_message.is_some());
    
    // Check if history overflow handling is needed
    if self.fix_history() {
        return Err(HistoryOverflowError);
    }
    
    // Rest of the method remains the same
    // ...
    
    Ok(FigConversationState { ... })
}
```

### 4. Add a New Error Type

Add a new error type to indicate history overflow:

```rust
#[derive(Debug)]
pub struct HistoryOverflowError;
```

### 5. Update the Main Chat Loop in `chat/mod.rs`

Modify the chat loop to handle history overflow:

```rust
// When sending a message
match self.conversation_state.as_sendable_conversation_state().await {
    Ok(state) => {
        // Proceed with sending the message
        // ...
    },
    Err(HistoryOverflowError) => {
        // Handle history overflow
        let mut handler = HistoryOverflowHandler::new(
            &self.ctx,
            &mut self.output,
            &mut self.input_source,
            self.interactive,
            &mut self.spinner,
            &self.client
        );
        
        return handler.handle_history_overflow(&mut self.conversation_state).await;
    }
}
```

## User Experience

When history overflow is detected, the user will see a message like:

```
⚠️ Your conversation history is getting too large.
Choose how you'd like to proceed:

1. Compact - Summarize the history and continue
2. Reset - Clear the history and start fresh
3. Continue - Keep the history as is (may cause issues)

Enter your choice (1-3):
```

### Option 1: Compact

- Displays a spinner with "Summarizing conversation..."
- Creates a temporary conversation state with the current history
- Sends a request to the model asking it to summarize the conversation
- Processes the response to extract the summary
- Clears the original conversation history
- Adds the summary as context (user message followed by assistant message)
- Continues the conversation with this compacted history

### Option 2: Reset

- Displays a message "Clearing conversation history..."
- Calls the existing `clear()` method on the conversation state
- Adds a new message explaining what happened
- Continues the conversation with a fresh history

### Option 3: Continue

- Displays a warning about potential issues with large history
- Keeps the history as is
- Continues the conversation normally

## Testing Plan

1. **Unit Tests**: Create unit tests for each of the new methods
2. **Integration Tests**: Create integration tests that simulate history overflow scenarios
3. **Manual Testing**: Test the feature with actual large conversation histories

## Implementation Timeline

1. Rename and refactor `large_tool_result_handler.rs` to `history_overflow_handler.rs`: 1 day
2. Modify `fix_history()` and related methods: 1 day
3. Update the main chat loop: 0.5 day
4. Testing and refinement: 1.5 days

Total estimated time: 4 days
