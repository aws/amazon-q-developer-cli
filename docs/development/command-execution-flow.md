# Command Execution Flow

This document describes the command execution flow in the Amazon Q CLI, focusing on how commands are processed from user input to execution, particularly with the `internal_command` tool integration (previously called `use_q_command`).

## Overview

The Amazon Q CLI supports two primary methods for executing commands:

1. **Direct Command Execution**: User types a command directly in the CLI (e.g., `/help`)
2. **AI-Assisted Command Execution**: User expresses intent in natural language, and the AI uses the `internal_command` tool to execute the appropriate command

Both paths ultimately use the same command handlers, ensuring consistent behavior regardless of how a command is invoked.

## State Transition Diagrams

### Chat State Transitions with Command Registry

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
    CommandRegistry --> ExecuteCommand: internal_command tool
    CommandRegistry --> PromptUser: Other commands
    HandleResponseStream --> ValidateTools: AI suggests tools
    ValidateTools --> ExecuteTools: Tools validated
    ValidateTools --> PromptUser: Validation failed
    ExecuteTools --> PromptUser: Tools executed
    ExecuteCommand --> PromptUser: Command executed
    DisplayHelp --> PromptUser: Help displayed
    Compact --> HandleInput: Compact processed
    Exit --> [*]
```

## Direct Command Execution Flow

```mermaid
sequenceDiagram
    participant User
    participant CLI as CLI Interface
    participant Parser as Command Parser
    participant Registry as Command Registry
    participant Handler as Command Handler
    participant State as Chat State

    User->>CLI: Enter command (/command args)
    CLI->>Parser: Parse input
    Parser->>Registry: Lookup command
    
    alt Command exists
        Registry->>Handler: Get handler
        Handler->>Handler: Parse arguments
        
        alt Requires confirmation
            Handler->>User: Prompt for confirmation
            User->>Handler: Confirm (Y/n)
        end
        
        Handler->>Handler: Execute command
        Handler->>State: Return new state
        State->>CLI: Update UI based on state
    else Command not found
        Registry->>CLI: Return error
        CLI->>User: Display error message
    end
```

## AI-Mediated Command Execution Flow

```mermaid
sequenceDiagram
    participant User
    participant CLI as CLI Interface
    participant AI as AI Assistant
    participant Tool as internal_command Tool
    participant Registry as Command Registry
    participant Handler as Command Handler
    participant State as Chat State

    User->>CLI: Enter natural language request
    CLI->>AI: Process request
    
    alt AI recognizes command intent
        AI->>Tool: Invoke internal_command
        Tool->>Tool: Format command string
        Tool->>State: Return ExecuteCommand state
        State->>CLI: Execute command directly
        CLI->>Registry: Lookup command
        
        alt Command exists
            Registry->>Handler: Get handler
            
            alt Requires confirmation
                Handler->>User: Prompt for confirmation
                User->>Handler: Confirm (Y/n)
            end
            
            Handler->>Handler: Execute command
            Handler->>State: Return new state
            State->>CLI: Update UI based on state
            CLI->>User: Display command result
        else Command not found
            Registry->>CLI: Return error
            CLI->>User: Display error message
        end
    else AI handles as regular query
        AI->>CLI: Generate normal response
        CLI->>User: Display AI response
    end
```

## Tool Execution Flow

### Tool Execution Sequence with internal_command

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
        InternalCommand->>ChatContext: Return ExecuteCommand state
        ChatContext->>CommandRegistry: Execute command directly
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

## Command Registry Architecture

```mermaid
classDiagram
    class CommandRegistry {
        -commands: HashMap<String, Box<dyn CommandHandler>>
        +new() CommandRegistry
        +global() &'static CommandRegistry
        +register(name: &str, handler: Box<dyn CommandHandler>)
        +get(name: &str) Option<&dyn CommandHandler>
        +command_exists(name: &str) bool
        +command_names() Vec<&String>
        +parse_and_execute(input: &str, ctx: &Context, tool_uses: Option<Vec<QueuedTool>>, pending_tool_index: Option<usize>) Result<ChatState>
        -parse_command(input: &str) Result<(&str, Vec<&str>)>
    }
    
    class CommandHandler {
        <<trait>>
        +name() &'static str
        +description() &'static str
        +usage() &'static str
        +help() String
        +execute(args: Vec<&str>, ctx: &Context, tool_uses: Option<Vec<QueuedTool>>, pending_tool_index: Option<usize>) Result<ChatState>
        +requires_confirmation(args: &[&str]) bool
        +parse_args(args: Vec<&str>) Result<Vec<&str>>
    }
    
    class QuitCommand {
        +new() QuitCommand
    }
    
    class HelpCommand {
        +new() HelpCommand
    }
    
    class ClearCommand {
        +new() ClearCommand
    }
    
    class ContextCommand {
        +new() ContextCommand
    }
    
    CommandHandler <|.. QuitCommand
    CommandHandler <|.. HelpCommand
    CommandHandler <|.. ClearCommand
    CommandHandler <|.. ContextCommand
    
    CommandRegistry o-- CommandHandler : contains
```

## Command Execution Flow Diagram

```mermaid
graph TD
    A[User Input] -->|Direct Command| B[Command Parser]
    A -->|Natural Language| C[AI Assistant]
    C -->|internal_command tool| D[InternalCommand]
    B --> E[CommandRegistry]
    D --> F[ExecuteCommand State]
    F --> E
    E --> G[Command Handler]
    G --> H[Command Execution]
    H --> I[Result]
    I --> J[User Output]
    
    subgraph "Command Registry"
    E
    end
    
    subgraph "Command Handlers"
    G
    end
```

## internal_command Tool Flow

```mermaid
sequenceDiagram
    participant AI as AI Assistant
    participant Tool as internal_command Tool
    participant ChatContext as Chat Context
    participant Registry as Command Registry
    participant Handler as Command Handler
    participant User

    AI->>Tool: Invoke with command parameters
    Tool->>Tool: Validate parameters
    Tool->>Tool: Construct command string
    Tool->>Tool: Create response with command suggestion
    Tool->>ChatContext: Return ExecuteCommand state
    ChatContext->>Registry: Execute command directly
    Registry->>Handler: Get handler
    
    alt Requires confirmation
        Handler->>User: Prompt for confirmation
        User->>Handler: Confirm (Y/n)
    end
    
    Handler->>Handler: Execute command
    Handler->>ChatContext: Return result
    ChatContext->>User: Display result
```

## Chat Loop Flow

### Chat Loop Sequence with Command Registry

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
                    ChatContext->>ChatContext: Execute command directly
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

## Detailed Flow

### 1. User Input Processing

#### Direct Command Path

- User enters a command with the `/` prefix (e.g., `/help`)
- The command parser identifies this as a command and extracts:
  - Command name (e.g., `help`)
  - Subcommand (if applicable)
  - Arguments (if any)

#### AI-Assisted Path

- User expresses intent in natural language (e.g., "Show me the available commands")
- The AI assistant recognizes the intent and invokes the `internal_command` tool
- The tool constructs a command with:
  - Command name (e.g., `help`)
  - Subcommand (if applicable)
  - Arguments (if any)
- The tool returns an `ExecuteCommand` state with the formatted command string
- The chat context executes the command directly

### 2. Command Registry

Both paths converge at the `CommandRegistry`, which:

- Validates the command exists
- Retrieves the appropriate command handler
- Passes the command, subcommand, and arguments to the handler

### 3. Command Handler

The command handler:

- Validates arguments
- Checks if user confirmation is required
- Performs the command's action
- Returns a result indicating success or failure

### 4. Command Execution

Based on the handler's result:

- Updates the chat state if necessary
- Formats output for the user
- Handles any errors that occurred

### 5. User Output

The result is presented to the user:

- Success message or command output
- Error message if something went wrong
- Confirmation prompt if required

## Security Considerations

The command execution flow includes several security measures:

### Command Validation

All commands are validated before execution to ensure they are recognized internal commands. Unknown commands are rejected with an error message.

### User Confirmation

Commands that modify state or perform destructive actions require user confirmation:

```mermaid
graph TD
    A[Command Received] --> B{Requires Confirmation?}
    B -->|Yes| C[Prompt User]
    B -->|No| D[Execute Command]
    C --> E{User Confirms?}
    E -->|Yes| D
    E -->|No| F[Cancel Command]
    D --> G[Return Result]
    F --> G
```

### Trust System

The CLI implements a trust system for tools and commands:

- Users can trust specific commands to execute without confirmation
- Trust can be granted for a single session or permanently
- Trust can be revoked at any time

## Command Handler Interface

All command handlers implement the `CommandHandler` trait:

```rust
pub trait CommandHandler: Send + Sync {
    /// Execute the command with the given arguments
    async fn execute(
        &self,
        args: &[&str],
        context: &Context,
        input: Option<&str>,
        output: Option<&mut dyn Write>,
    ) -> Result<ChatState>;

    /// Check if the command requires confirmation before execution
    fn requires_confirmation(&self, args: &[&str]) -> bool;

    /// Get the name of the command
    fn name(&self) -> &'static str;

    /// Get a description of the command
    fn description(&self) -> &'static str;

    /// Get a description of the command for the LLM
    fn llm_description(&self) -> &'static str;
}
```

## internal_command Tool Integration

The `internal_command` tool provides a bridge between natural language processing and command execution:

```rust
pub struct InternalCommand {
    /// The command to execute (e.g., "help", "context", "profile")
    pub command: String,
    
    /// Optional subcommand (e.g., "add", "remove", "list")
    pub subcommand: Option<String>,
    
    /// Optional arguments for the command
    pub args: Option<Vec<String>>,
}
```

When invoked, the tool:

1. Constructs a command string from the provided parameters
2. Creates a response with the command suggestion
3. Returns an `ExecuteCommand` state with the formatted command string
4. The chat context executes the command directly

## Testing Strategy

The command execution flow is tested at multiple levels:

### Unit Tests

- Test individual command handlers in isolation
- Verify argument parsing and validation
- Check confirmation requirements

### Integration Tests

- Test the complete flow from command string to execution
- Verify both direct and AI-assisted paths produce identical results
- Test error handling and edge cases

### End-to-End Tests

- Test the complete system with real user input
- Verify AI recognition of command intents
- Test complex scenarios with multiple commands

## Example: Help Command Execution

### Direct Path

1. User types `/help`
2. Command parser extracts command name `help`
3. `CommandRegistry` retrieves the `HelpCommand` handler
4. `HelpCommand::execute` is called with empty arguments
5. Help text is displayed to the user

### AI-Assisted Path

1. User asks "What commands are available?"
2. AI recognizes intent and calls `internal_command` with `command: "help"`
3. `InternalCommand` constructs command string `/help`
4. `InternalCommand` returns an `ExecuteCommand` state with the command string
5. Chat context executes the command directly
6. `CommandRegistry` retrieves the `HelpCommand` handler
7. `HelpCommand::execute` is called with empty arguments
8. Help text is displayed to the user

## Implementation Considerations

1. **Command Validation**: All commands should be validated before execution, both in direct and AI-mediated flows.

2. **Confirmation Handling**: Commands that require confirmation should prompt the user in both flows.

3. **Error Handling**: Errors should be properly propagated and displayed to the user in a consistent manner.

4. **State Management**: The chat state should be updated consistently regardless of how the command was invoked.

5. **Security**: Commands executed through the AI should have the same security checks as direct commands.

6. **Telemetry**: Track command usage patterns for both direct and AI-mediated execution.

7. **Testing**: Test both execution paths thoroughly to ensure consistent behavior.

## Summary of Changes

The implementation of the Command Registry architecture introduces several key improvements:

1. **Better Separation of Concerns**:
   - Commands are now handled by dedicated CommandHandler implementations
   - The CommandRegistry manages command registration and execution
   - The ChatContext focuses on managing the chat flow rather than command execution

2. **More Modular and Maintainable Code**:
   - Each command has its own handler class
   - Adding new commands is as simple as implementing the CommandHandler trait
   - Command behavior is more consistent and predictable

3. **Enhanced Security**:
   - The internal_command tool executes commands directly through the ExecuteCommand state
   - Command permissions are managed more consistently

4. **Improved User Experience**:
   - Command execution is more seamless
   - Command behavior is more consistent
   - Error handling is more robust

5. **Better Testability**:
   - Command handlers can be tested in isolation
   - The CommandRegistry can be tested with mock handlers
   - The chat loop can be tested with a mock CommandRegistry

## Conclusion

The command execution flow in Amazon Q CLI provides a consistent and secure way to execute commands, whether they are entered directly by the user or through AI assistance. The unified path through the `CommandRegistry` ensures that commands behave identically regardless of how they are invoked, while the security measures protect against unintended actions.
