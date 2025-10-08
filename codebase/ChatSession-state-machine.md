# ChatSession State Machine Analysis

## Overview

The ChatSession implementation is indeed a state machine where each state change results in output or initiating TUI input. The state machine is implemented using the `ChatState` enum and managed through the `next()` method which acts as the state transition controller.

## State Definitions

The `ChatState` enum defines the following states (lines 1106-1135):

```rust
pub enum ChatState {
    PromptUser { skip_printing_tools: bool },
    HandleInput { input: String },
    ValidateTools { tool_uses: Vec<AssistantToolUse> },
    ExecuteTools,
    HandleResponseStream(ConversationState),
    CompactHistory { prompt: Option<String>, show_summary: bool, strategy: CompactStrategy },
    RetryModelOverload,
    Exit,
}
```

## Entry Point (State0)

The entry point is the `spawn` method (line 1173), which:
1. Shows greeting/welcome messages (lines 1174-1220)
2. Sets initial state based on `initial_input` (lines 1245-1247):
   - If `initial_input` exists: `ChatState::HandleInput { input: user_input }`
   - Otherwise: `ChatState::default()` which is `PromptUser { skip_printing_tools: false }`
3. Enters main loop: `while !matches!(self.inner, Some(ChatState::Exit))` (line 1249)

## State Transition Controller

The `next()` method (line 753) acts as the state transition controller:
1. Takes current state with `self.inner.take()` (line 758)
2. Matches on state and calls appropriate handler
3. Sets new state with `self.inner = Some(state)` (line 826)
4. Handles errors and resets to `PromptUser` on error (line 1000)

## State Transitions and Code Mappings

### 1. PromptUser State
**Handler:** `prompt_user()` (line 1767)
**Transitions:**
- → `Exit`: When user input is None (line 1812)
- → `HandleInput`: When user provides input (line 1816)
- → `Exit`: In non-interactive mode with no tools (line 762)
- **Error:** `NonInteractiveToolApproval` in non-interactive mode with tools (line 766)

### 2. HandleInput State  
**Handler:** `handle_input()` (line 1819)
**Transitions:**
- → `HandleInput`: For file references (line 1826)
- → `PromptUser`: For slash commands and shell commands (lines 1850, 1965)
- → `Exit`, `HandleResponseStream`, `CompactHistory`: From slash command execution (line 1850)
- → `ExecuteTools`: When tool approved with 'y' or 't' (line 1999)
- → `HandleResponseStream`: For normal chat input (line 2043)

### 3. ValidateTools State
**Handler:** `validate_tools()` (line 2630)  
**Transitions:**
- → `HandleResponseStream`: When validation errors exist (line 2724)
- → `ExecuteTools`: When all tools are valid (line 2738)

### 4. ExecuteTools State
**Handler:** `tool_use_execute()` (line 2047)
**Transitions:**
- → `PromptUser`: When tool needs approval (line 2095)
- → `HandleResponseStream`: After successful tool execution (line 2318)

### 5. HandleResponseStream State
**Handler:** `handle_response()` (line 2330)
**Special State:** This is the "receiving response" state that constantly prints received data
**Transitions:**
- → `ValidateTools`: When response contains tool uses (line 2615)
- → `PromptUser`: When response has no tool uses (line 2624)

### 6. CompactHistory State
**Handler:** `compact_history()` (line 1260)
**Transitions:**
- → `PromptUser`: When history too short or after successful compaction (lines 1295, 1430)
- → `CompactHistory`: For retry with different strategy (lines 1370, 1380, 1390)
- → `HandleResponseStream`: When should retry after compaction (line 1425)

### 7. RetryModelOverload State
**Handler:** `retry_model_overload()` (line 2742)
**Transitions:**
- → `PromptUser`: When user doesn't select model (line 2750)
- → `HandleResponseStream`: When model selected (line 2760)

### 8. Exit State
**Handler:** Direct return in `next()` (line 822)
**Transitions:** None (terminal state)

## Error Handling and State Reset

When errors occur in `next()` (lines 830-1000):
1. Spinner is stopped and cleared (lines 835-842)
2. Error-specific handling for interrupts, context overflow, etc. (lines 844-980)
3. Conversation state is reset (lines 995-999)
4. State is reset to `PromptUser { skip_printing_tools: false }` (line 1000)

## Key State Machine Properties

1. **Single Active State:** Only one state is active at a time via `Option<ChatState>`
2. **Deterministic Transitions:** Each state handler returns exactly one next state
3. **Error Recovery:** All errors lead back to `PromptUser` state
4. **Ctrl+C Handling:** Most states handle interruption via `tokio::select!`
5. **Output Coupling:** Each state change triggers specific UI output or input collection
6. **Tool Approval Flow:** Special sub-machine for tool approval (`PromptUser` → `ExecuteTools` → `HandleResponseStream`)

## Special Receiving Response State

The `HandleResponseStream` state implements the continuous output behavior mentioned in the requirements:
- Receives streaming response chunks (line 2380)
- Immediately renders markdown and flushes to stdout (lines 2450-2470)  
- Uses 8ms delays for smooth typing animation (line 2470)
- Continues until stream ends, then transitions based on tool presence
