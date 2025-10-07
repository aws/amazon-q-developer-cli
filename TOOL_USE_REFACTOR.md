# Tool Use System Refactoring Plan

This document outlines the refactoring of the tool use system to separate UI concerns from core logic, enabling clean ACP integration without affecting existing console functionality.

## Problem Statement

The current tool execution system in `crates/chat-cli/src/cli/chat/mod.rs` has three concerns tightly coupled together:

1. **Permission Evaluation** - Pure logic determining if tools are allowed
2. **UI Interaction** - Console output, user prompts, colored formatting
3. **Flow Control** - State machine with `ChatState::PromptUser` and `pending_tool_index`

This coupling creates several issues:
- **ACP Integration Blocker**: Console I/O prevents use in protocol-based sessions
- **Complex State Machine**: Scattered control flow across multiple state transitions
- **Testing Difficulty**: UI side effects make unit testing challenging
- **Code Clarity**: Mixed concerns make the logic hard to follow

## Current Architecture Issues

### Console I/O Coupling
```rust
// Lines 2280-2291 in tool_use_execute()
execute!(
    self.stderr,
    style::SetForegroundColor(Color::Red),
    style::Print("Command "),
    style::Print(&tool.name),
    style::Print(" is rejected..."),
)?;
```

### State Machine Complexity
```rust
// Permission needed → return special state
self.pending_tool_index = Some(i);
return Ok(ChatState::PromptUser { skip_printing_tools: false });

// Later, somehow resume from pending state...
```

### Mixed Concerns in Single Method
The `tool_use_execute()` method handles:
- Permission evaluation logic
- Terminal formatting and output
- User input handling
- Tool execution
- State management

## Refactoring Solution

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    tool_use_execute()                       │
│                   (Clean async flow)                       │
└─────────────────┬───────────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────────┐
│               Permission Evaluation                        │
│                 (Pure functions)                           │
└─────────────────┬───────────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────────┐
│              PermissionInterface                           │
│                 (Async trait)                              │
└─────────────┬─────────────────────────────┬─────────────────┘
              │                             │
┌─────────────▼─────────────┐    ┌─────────▼─────────────────┐
│  ConsolePermissionInterface│    │  AcpPermissionInterface  │
│    (Terminal I/O)          │    │   (Protocol messages)    │
└────────────────────────────┘    └───────────────────────────┘
```

### Key Components

#### 1. Pure Permission Evaluation
```rust
// New file: crates/chat-cli/src/cli/chat/permission.rs
pub fn evaluate_tool_permissions(
    tools: &[QueuedTool],
    agents: &AgentState,
    os: &Os,
) -> Vec<ToolPermissionResult>;

pub enum ToolPermissionResult {
    Allowed,
    RequiresConfirmation { tool_index: usize, tool_name: String },
    Denied { tool_index: usize, rules: Vec<String> },
}
```

#### 2. Permission Interface Abstraction
```rust
#[async_trait]
pub trait PermissionInterface {
    async fn request_permission(
        &mut self,
        tool: &QueuedTool,
        context: &PermissionContext,
    ) -> Result<PermissionDecision>;
    
    async fn show_denied_tool(
        &mut self,
        tool: &QueuedTool,
        rules: Vec<String>,
    ) -> Result<()>;
    
    async fn show_tool_execution(
        &mut self,
        tool: &QueuedTool,
        allowed: bool,
    ) -> Result<()>;
}

pub enum PermissionDecision {
    Approved,
    Rejected,
    Cancelled,
}
```

#### 3. Console Implementation
```rust
// New file: crates/chat-cli/src/cli/chat/permission/console.rs
pub struct ConsolePermissionInterface<'a> {
    stdout: &'a mut dyn Write,
    stderr: &'a mut dyn Write,
    stdin: Box<dyn BufRead>,
}

#[async_trait]
impl PermissionInterface for ConsolePermissionInterface<'_> {
    async fn request_permission(&mut self, tool: &QueuedTool, context: &PermissionContext) -> Result<PermissionDecision> {
        // Existing console behavior:
        // - Show colored tool description
        // - Play notification bell
        // - Read user input from stdin
        // - Return decision
    }
}
```

#### 4. Refactored Main Flow
```rust
async fn tool_use_execute(&mut self, os: &mut Os) -> Result<ChatState, ChatError> {
    // 1. Pure permission evaluation
    let permission_results = evaluate_tool_permissions(&self.tool_uses, &self.conversation.agents, os);
    
    // 2. Create appropriate permission interface
    let mut permission_interface = self.create_permission_interface();
    
    // 3. Handle permissions with clean async flow
    for result in permission_results {
        match result {
            ToolPermissionResult::RequiresConfirmation { tool_index, .. } => {
                let tool = &self.tool_uses[tool_index];
                let decision = permission_interface.request_permission(tool, &context).await?;
                match decision {
                    PermissionDecision::Approved => {
                        self.tool_uses[tool_index].accepted = true;
                    }
                    PermissionDecision::Rejected => {
                        return Ok(ChatState::HandleInput { 
                            input: format!("Tool {} was rejected", tool.name) 
                        });
                    }
                }
            }
            ToolPermissionResult::Denied { tool_index, rules } => {
                let tool = &self.tool_uses[tool_index];
                permission_interface.show_denied_tool(tool, rules).await?;
                return Ok(ChatState::HandleInput { 
                    input: format!("Tool {} was denied", tool.name) 
                });
            }
            ToolPermissionResult::Allowed => {
                // Tool already approved
            }
        }
    }
    
    // 4. Execute tools (existing logic, unchanged)
    self.execute_approved_tools(os).await
}
```

## Benefits

### 1. Eliminates State Machine Complexity
- **Before**: Complex state transitions with `ChatState::PromptUser` and `pending_tool_index`
- **After**: Straightforward async flow that reads naturally top-to-bottom

### 2. Enables Clean ACP Integration
- **Console Interface**: Preserves existing terminal behavior exactly
- **ACP Interface**: Routes permission requests through protocol messages
- **Swappable**: Same core logic, different UI implementations

### 3. Improves Testability
- **Pure Functions**: Permission evaluation can be unit tested without UI
- **Mockable Interfaces**: Permission interfaces can be mocked for testing
- **Isolated Concerns**: Each component can be tested independently

### 4. Maintains Backward Compatibility
- **Zero Behavior Changes**: Existing console functionality identical
- **Same External API**: No changes to public interfaces
- **Incremental Migration**: Can be implemented step-by-step

## Implementation Plan

### Phase 1: Extract Permission Evaluation
1. Create `crates/chat-cli/src/cli/chat/permission.rs`
2. Move permission logic from `tool_use_execute()` into pure functions
3. Define `ToolPermissionResult` enum for structured results
4. Update `tool_use_execute()` to use extracted functions

### Phase 2: Create Permission Interface
1. Define `PermissionInterface` trait with async methods
2. Create `PermissionContext` for passing context data
3. Define `PermissionDecision` enum for results

### Phase 3: Implement Console Interface
1. Create `crates/chat-cli/src/cli/chat/permission/console.rs`
2. Move existing console I/O logic into `ConsolePermissionInterface`
3. Implement all trait methods preserving current behavior
4. Handle stdin reading asynchronously

### Phase 4: Refactor Main Flow
1. Update `tool_use_execute()` to use new interfaces
2. Add `create_permission_interface()` factory method
3. Remove old permission handling code
4. Remove `pending_tool_index` and related state machine logic

### Phase 5: Testing and Validation
1. Add unit tests for permission evaluation functions
2. Add integration tests for console interface
3. Verify identical behavior with existing system
4. Performance testing to ensure no regressions

## Future ACP Integration

Once this refactoring is complete, ACP integration becomes straightforward:

```rust
// Future ACP implementation
pub struct AcpPermissionInterface {
    client: AcpClientHandle,
    session_id: SessionId,
}

#[async_trait]
impl PermissionInterface for AcpPermissionInterface {
    async fn request_permission(&mut self, tool: &QueuedTool, context: &PermissionContext) -> Result<PermissionDecision> {
        // Send protocol message
        let request = acp::RequestPermissionRequest {
            tool_name: tool.name.clone(),
            tool_args: tool.tool.get_args(),
            reason: context.reason.clone(),
        };
        
        let response = self.client.request_permission(request).await?;
        
        Ok(match response.decision {
            acp::PermissionResult::Approved => PermissionDecision::Approved,
            acp::PermissionResult::Denied => PermissionDecision::Rejected,
        })
    }
}
```

The ACP session actor can then create an `AcpPermissionInterface` instead of `ConsolePermissionInterface`, routing all permission requests through the protocol without changing any core tool logic.

## File Structure

```
crates/chat-cli/src/cli/chat/
├── mod.rs                          # Updated tool_use_execute()
├── permission.rs                   # Pure permission evaluation
└── permission/
    ├── mod.rs                      # PermissionInterface trait
    ├── console.rs                  # Console implementation
    └── acp.rs                      # Future ACP implementation
```

## Risk Mitigation

### Behavior Preservation
- **Identical Console Flow**: All existing terminal interactions preserved exactly
- **Same Error Messages**: Error formatting and messaging unchanged
- **Performance Parity**: No significant performance impact

### Testing Strategy
- **Side-by-Side Testing**: Run old and new implementations in parallel
- **Integration Tests**: Full tool execution scenarios
- **Manual Verification**: Interactive testing of permission flows

### Rollback Plan
- **Incremental Changes**: Each phase can be implemented and tested independently
- **Feature Flags**: Can gate new implementation behind feature flag if needed
- **Clean Separation**: Old code can be preserved during transition

This refactoring provides a clean foundation for ACP tool integration while maintaining full backward compatibility with existing console-based tool execution.