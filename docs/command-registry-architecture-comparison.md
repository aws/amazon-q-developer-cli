# Command Registry Architecture Comparison

This document compares the architecture before and after implementing the Command Registry system as outlined in RFC 0002. It includes state transition diagrams, sequence diagrams, and detailed comparisons for each component.

## Table of Contents

1. [State Transition Diagrams](#state-transition-diagrams)
2. [Tool Execution Flow](#tool-execution-flow)
3. [Command Execution Flow](#command-execution-flow)
4. [Chat Loop Flow](#chat-loop-flow)
5. [Summary of Changes](#summary-of-changes)

## State Transition Diagrams

### Before: Chat State Transitions

```mermaid
stateDiagram-v2
    [*] --> PromptUser
    PromptUser --> HandleInput: User enters input
    HandleInput --> ExecuteTools: Tool execution requested
    HandleInput --> ValidateTools: Tool validation needed
    HandleInput --> DisplayHelp: Help command
    HandleInput --> Compact: Compact command
    HandleInput --> Exit: Quit command
    HandleInput --> HandleResponseStream: Ask question
    HandleResponseStream --> ValidateTools: AI suggests tools
    ValidateTools --> ExecuteTools: Tools validated
    ValidateTools --> PromptUser: Validation failed
    ExecuteTools --> PromptUser: Tools executed
    DisplayHelp --> PromptUser: Help displayed
    Compact --> HandleInput: Compact processed
    Exit --> [*]
```

### After: Chat State Transitions with Command Registry

```mermaid
stateDiagram-v2
    [*] --> PromptUser
    PromptUser --> HandleInput: User enters input
    HandleInput --> CommandRegistry: Command detected
    HandleInput --> ExecuteTools: Tool execution requested
    HandleInput --> ValidateTools: Tool validation needed
    HandleInput --> HandleResponseStream: Ask question
    CommandRegistry --> DisplayHelp: Help command
    CommandRegistry --> Compact: Compact command
    CommandRegistry --> Exit: Quit command
    CommandRegistry --> PromptUser: Other commands
    HandleResponseStream --> ValidateTools: AI suggests tools
    ValidateTools --> ExecuteTools: Tools validated
    ValidateTools --> PromptUser: Validation failed
    ExecuteTools --> PromptUser: Tools executed
    DisplayHelp --> PromptUser: Help displayed
    Compact --> HandleInput: Compact processed
    Exit --> [*]
```

### Comparison: State Transitions

The key difference in the state transition diagrams is the introduction of the CommandRegistry state. In the original architecture, commands were handled directly within the HandleInput state. The new architecture introduces a dedicated CommandRegistry state that processes all commands through a unified interface.

This change provides several benefits:
- Better separation of concerns
- More consistent command handling
- Easier addition of new commands
- Improved testability of command execution

The overall flow remains similar, but the command handling is now more structured and modular.

## Tool Execution Flow

### Before: Tool Execution Sequence

```mermaid
sequenceDiagram
    participant User
    participant ChatContext
    participant Tool
    participant ToolPermissions
    
    User->>ChatContext: Enter input
    ChatContext->>ChatContext: Parse input
    ChatContext->>ChatContext: Handle AI response
    ChatContext->>ChatContext: Detect tool use
    ChatContext->>ToolPermissions: Check if tool is trusted
    alt Tool is trusted
        ToolPermissions->>ChatContext: Tool is trusted
        ChatContext->>Tool: Execute tool directly
    else Tool requires confirmation
        ToolPermissions->>ChatContext: Tool needs confirmation
        ChatContext->>User: Request confirmation
        User->>ChatContext: Confirm (y/n/t)
        alt User confirms
            ChatContext->>Tool: Execute tool
        else User denies
            ChatContext->>ChatContext: Skip tool execution
        end
    end
    Tool->>ChatContext: Return result
    ChatContext->>User: Display result
```

### After: Tool Execution Sequence with internal_command

```mermaid
sequenceDiagram
    participant User
    participant ChatContext
    participant CommandRegistry
    participant InternalCommand
    participant Tool
    participant ToolPermissions
    
    User->>ChatContext: Enter input
    ChatContext->>ChatContext: Parse input
    ChatContext->>ChatContext: Handle AI response
    ChatContext->>ChatContext: Detect tool use
    
    alt Tool is internal_command
        ChatContext->>InternalCommand: Execute internal_command
        InternalCommand->>InternalCommand: Format command
        InternalCommand->>InternalCommand: Get description
        InternalCommand->>ChatContext: Return suggestion
        ChatContext->>User: Display command suggestion
        User->>ChatContext: Enter suggested command
        ChatContext->>CommandRegistry: Execute command
        CommandRegistry->>Tool: Execute appropriate tool
    else Other tool
        ChatContext->>ToolPermissions: Check if tool is trusted
        alt Tool is trusted
            ToolPermissions->>ChatContext: Tool is trusted
            ChatContext->>Tool: Execute tool directly
        else Tool requires confirmation
            ToolPermissions->>ChatContext: Tool needs confirmation
            ChatContext->>User: Request confirmation
            User->>ChatContext: Confirm (y/n/t)
            alt User confirms
                ChatContext->>Tool: Execute tool
            else User denies
                ChatContext->>ChatContext: Skip tool execution
            end
        end
    end
    
    Tool->>ChatContext: Return result
    ChatContext->>User: Display result
```

### Comparison: Tool Execution

The key differences in the tool execution flow are:

1. **Introduction of the InternalCommand Tool**:
   - The new architecture introduces a dedicated InternalCommand tool that handles command suggestions
   - Instead of executing commands directly, it formats and suggests commands to the user

2. **Command Registry Integration**:
   - When the user enters a suggested command, it's processed through the CommandRegistry
   - The CommandRegistry delegates to the appropriate command handler

3. **Two-Step Command Execution**:
   - In the new architecture, command execution becomes a two-step process:
     1. AI suggests a command via the InternalCommand tool
     2. User enters the suggested command, which is then executed

This approach provides several benefits:
- Better user control over command execution
- Clearer separation between AI suggestions and actual command execution
- More consistent handling of commands
- Improved security by requiring explicit user action for command execution

## Command Execution Flow

### Before: Command Execution Sequence

```mermaid
sequenceDiagram
    participant User
    participant ChatContext
    participant Command
    participant CommandHandler
    
    User->>ChatContext: Enter command
    ChatContext->>Command: Parse command
    Command->>ChatContext: Return parsed command
    
    alt Command is valid
        ChatContext->>ChatContext: Execute command directly
        ChatContext->>User: Display result
    else Command is invalid
        ChatContext->>User: Display error
    end
```

### After: Command Execution Sequence with Registry

```mermaid
sequenceDiagram
    participant User
    participant ChatContext
    participant Command
    participant CommandRegistry
    participant CommandHandler
    
    User->>ChatContext: Enter command
    ChatContext->>Command: Parse command
    Command->>ChatContext: Return parsed command
    
    alt Command is valid
        ChatContext->>CommandRegistry: Execute command
        CommandRegistry->>CommandRegistry: Look up handler
        CommandRegistry->>CommandHandler: Execute handler
        CommandHandler->>CommandRegistry: Return result
        CommandRegistry->>ChatContext: Return result
        ChatContext->>User: Display result
    else Command is invalid
        ChatContext->>User: Display error
    end
```

### Comparison: Command Execution

The key differences in the command execution flow are:

1. **Introduction of the CommandRegistry**:
   - The new architecture introduces a dedicated CommandRegistry that manages command handlers
   - Commands are no longer executed directly by the ChatContext

2. **Command Handler Delegation**:
   - The CommandRegistry delegates command execution to specific CommandHandler implementations
   - Each command has its own handler class that implements the CommandHandler trait

3. **Standardized Interface**:
   - All commands now follow a standardized interface defined by the CommandHandler trait
   - This ensures consistent behavior across all commands

This approach provides several benefits:
- Better separation of concerns
- More modular and maintainable code
- Easier addition of new commands
- Improved testability of command execution
- Consistent command behavior

## Chat Loop Flow

### Before: Chat Loop Sequence

```mermaid
sequenceDiagram
    participant User
    participant ChatContext
    participant CommandParser
    participant AIClient
    participant ToolExecutor
    
    User->>ChatContext: Start chat
    loop Chat Loop
        ChatContext->>User: Prompt for input
        User->>ChatContext: Enter input
        
        alt Input is a command
            ChatContext->>CommandParser: Parse command
            CommandParser->>ChatContext: Return command
            ChatContext->>ChatContext: Execute command directly
        else Input is a question
            ChatContext->>AIClient: Send question
            AIClient->>ChatContext: Return response
            
            alt Response includes tool use
                ChatContext->>ToolExecutor: Execute tool
                ToolExecutor->>ChatContext: Return result
            end
            
            ChatContext->>User: Display response
        end
    end
```

### After: Chat Loop Sequence with Command Registry

```mermaid
sequenceDiagram
    participant User
    participant ChatContext
    participant CommandParser
    participant CommandRegistry
    participant CommandHandler
    participant AIClient
    participant ToolExecutor
    
    User->>ChatContext: Start chat
    loop Chat Loop
        ChatContext->>User: Prompt for input
        User->>ChatContext: Enter input
        
        alt Input is a command
            ChatContext->>CommandParser: Parse command
            CommandParser->>ChatContext: Return command
            ChatContext->>CommandRegistry: Execute command
            CommandRegistry->>CommandHandler: Delegate to handler
            CommandHandler->>CommandRegistry: Return result
            CommandRegistry->>ChatContext: Return result
        else Input is a question
            ChatContext->>AIClient: Send question
            AIClient->>ChatContext: Return response
            
            alt Response includes tool use
                alt Tool is internal_command
                    ChatContext->>User: Display command suggestion
                    User->>ChatContext: Enter suggested command
                    ChatContext->>CommandRegistry: Execute command
                else Other tool
                    ChatContext->>ToolExecutor: Execute tool
                end
                ToolExecutor->>ChatContext: Return result
            end
            
            ChatContext->>User: Display response
        end
    end
```

### Comparison: Chat Loop

The key differences in the chat loop flow are:

1. **Command Registry Integration**:
   - Commands are now processed through the CommandRegistry instead of being executed directly
   - The CommandRegistry delegates to specific CommandHandler implementations

2. **Internal Command Tool**:
   - The chat loop now handles the internal_command tool specially
   - When the AI suggests a command, it's displayed to the user for manual execution

3. **Two-Step Command Execution**:
   - Command execution becomes a two-step process:
     1. AI suggests a command via the internal_command tool
     2. User enters the suggested command, which is then executed through the CommandRegistry

This approach provides several benefits:
- Better separation of concerns
- More consistent command handling
- Improved user control over command execution
- Enhanced security by requiring explicit user action for command execution

## Summary of Changes

The implementation of the Command Registry architecture as outlined in RFC 0002 introduces several key improvements:

1. **Better Separation of Concerns**:
   - Commands are now handled by dedicated CommandHandler implementations
   - The CommandRegistry manages command registration and execution
   - The ChatContext focuses on managing the chat flow rather than command execution

2. **More Modular and Maintainable Code**:
   - Each command has its own handler class
   - Adding new commands is as simple as implementing the CommandHandler trait
   - Command behavior is more consistent and predictable

3. **Enhanced Security**:
   - The internal_command tool suggests commands rather than executing them directly
   - Users have explicit control over command execution
   - Command permissions are managed more consistently

4. **Improved User Experience**:
   - Command suggestions provide better guidance to users
   - Command behavior is more consistent
   - Error handling is more robust

5. **Better Testability**:
   - Command handlers can be tested in isolation
   - The CommandRegistry can be tested with mock handlers
   - The chat loop can be tested with a mock CommandRegistry

These changes align with the goals of RFC 0002 to improve the command handling architecture while maintaining compatibility with the existing codebase. The suggestion-based approach allows for a smoother transition to the new command registry system.
