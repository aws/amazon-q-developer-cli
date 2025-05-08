- Feature Name: internal_command_tool
- Start Date: 2025-03-28
- Implementation Status: Completed

# Summary

[summary]: #summary

This RFC proposes adding a new tool called `internal_command` to the Amazon Q Developer CLI that will enable the AI assistant to directly execute internal commands within the q chat system. This will improve user experience by handling vague or incorrectly typed requests more gracefully and providing more direct assistance with command execution.

# Motivation

[motivation]: #motivation

Currently, when users make vague requests or use incorrect syntax (e.g., typing "Bye" instead of "/quit"), the system responds with suggestions like "You can quit the application by typing /quit" but doesn't take action. This creates friction in the user experience as users must:

1. Read the suggestion
2. Manually type the correct command
3. Wait for execution

Additionally, users may not be familiar with all available internal commands, their syntax, or their capabilities, leading to frustration and reduced productivity.

# Guide-level explanation

[guide-level-explanation]: #guide-level-explanation

The `internal_command` tool allows the AI assistant to directly execute internal commands within the q chat system on behalf of the user. This creates a more natural and fluid interaction model where users can express their intent in natural language, and the AI can take appropriate action.

For example, instead of this interaction:

```
User: Bye
AI: You can quit the application by typing /quit
User: /quit
[Application exits]
```

The user would experience:

```
User: Bye
AI: I'll help you exit the application.
[AI executes /quit command]
[Application exits]
```

The tool supports various categories of internal commands:

1. **Slashcommands** - Direct execution of slash commands like `/quit`, `/clear`, `/help`, etc.
2. **Context Management** - Operations on conversation history like querying, pruning, or summarizing
3. **Tools Management** - Listing, enabling, disabling, or installing tools
4. **Settings Management** - Viewing or changing settings
5. **Controls** - Read-only access to system state

This feature makes the Amazon Q Developer CLI more intuitive and responsive to user needs, reducing the learning curve and improving overall productivity.

# Reference-level explanation

[reference-level-explanation]: #reference-level-explanation

## Tool Interface

The `internal_command` tool is implemented as part of the existing tools framework in the `q_chat` crate. It has the following interface:

```rust
pub struct InternalCommand {
    /// The command to execute (e.g., "quit", "context", "settings")
    pub command: String,
    
    /// Optional arguments for the command
    pub args: Vec<String>,
    
    /// Optional flags for the command
    pub flags: HashMap<String, String>,
}
```

## Implementation Details

The tool is implemented in the `q_chat` crate under `src/tools/internal_command/`. The implementation:

1. Parses the incoming request into the appropriate internal command format
2. Validates the command and arguments
3. Executes the command using the command registry infrastructure
4. Captures the output/results
5. Returns the results to the AI assistant

### Project Structure

The command-related code is organized as follows:

```
src/
├── commands/           # Directory for all command-related code
│   ├── mod.rs          # Exports the CommandHandler trait
│   ├── handler.rs      # CommandHandler trait definition
│   ├── quit.rs         # QuitCommand implementation
│   ├── clear.rs        # ClearCommand implementation
│   ├── help.rs         # HelpCommand implementation
│   ├── context/        # Context command and subcommands
│   ├── profile/        # Profile command and subcommands
│   └── tools/          # Tools command and subcommands
├── tools/              # Directory for tools
│   ├── mod.rs
│   ├── execute_bash.rs
│   ├── fs_read.rs
│   ├── fs_write.rs
│   ├── gh_issue.rs
│   ├── use_aws.rs
│   └── internal_command/  # New tool that uses the command registry
└── mod.rs
```

### Command-Centric Architecture

The implementation uses a command-centric architecture with a bidirectional relationship between Commands and Handlers:

1. **CommandHandler Trait**:
   - Includes a `to_command()` method that returns a `Command` enum with values
   - Has a default implementation of `execute` that delegates to `to_command`

2. **Command Enum**:
   - Includes a `to_handler()` method that returns the appropriate CommandHandler for a Command variant
   - Implements static handler instances for each command
   - Creates a bidirectional relationship between Commands and Handlers

3. **Static Handler Instances**:
   - Each command handler is defined as a static instance
   - These static instances are referenced by the Command enum's `to_handler()` method

This approach:
- Makes the command system more type-safe by using enum variants
- Separates command parsing from execution
- Creates a command-centric architecture with bidirectional relationships
- Reduces dependency on a central registry
- Ensures consistent behavior between direct command execution and tool-based execution

### Separation of Parsing and Output

A key architectural principle is the strict separation between parsing and output/display logic:

1. **Command Parsing**:
   - The `parse` method in the Command enum and the `to_command` method in CommandHandler implementations should only handle converting input strings to structured data.
   - These methods should not produce any output or display messages.
   - Error handling in parsing should focus on returning structured errors, not formatting user-facing messages.

2. **Command Execution**:
   - All output-related code (like displaying usage hints, deprecation warnings, or help text) belongs in the execution phase.
   - The `execute_command` method in CommandHandler implementations is responsible for displaying messages and producing output.
   - User-facing messages should be generated during execution, not during parsing.

This separation ensures:
- Clean, testable parsing logic that focuses solely on input validation and structure conversion
- Consistent user experience regardless of how commands are invoked (directly or via the internal_command tool)
- Centralized output handling that can be easily styled and formatted
- Better testability of both parsing and execution logic independently

### CommandHandler Trait

```rust
pub trait CommandHandler: Send + Sync {
    /// Returns the name of the command
    fn name(&self) -> &'static str;
    
    /// Returns a description of the command
    fn description(&self) -> &'static str;
    
    /// Returns usage information for the command
    fn usage(&self) -> &'static str;
    
    /// Returns detailed help text for the command
    fn help(&self) -> String;
    
    /// Returns a detailed description with examples for LLM tool descriptions
    fn llm_description(&self) -> String {
        // Default implementation returns the regular help text
        self.help()
    }
    
    /// Convert arguments to a Command enum
    fn to_command<'a>(&self, args: Vec<&'a str>) -> Result<Command>;
    
    /// Execute the command with the given arguments
    fn execute<'a>(
        &self, 
        args: Vec<&'a str>, 
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState>> + 'a>> {
        Box::pin(async move {
            let command = self.to_command(args)?;
            Ok(ChatState::ExecuteCommand {
                command,
                tool_uses,
                pending_tool_index,
            })
        })
    }
    
    /// Execute a command directly
    fn execute_command<'a>(
        &'a self, 
        command: &'a Command, 
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState>> + 'a>> {
        // Default implementation that returns an error for unexpected command types
        Box::pin(async move {
            Err(anyhow!("Unexpected command type for this handler"))
        })
    }
    
    /// Check if this command requires confirmation before execution
    fn requires_confirmation(&self, args: &[&str]) -> bool {
        false // Most commands don't require confirmation by default
    }
    
    /// Parse arguments for this command
    fn parse_args<'a>(&self, args: Vec<&'a str>) -> Result<Vec<&'a str>> {
        Ok(args)
    }
}
```

### Command Enum Enhancement

```rust
impl Command {
    // Get the appropriate handler for this command variant
    pub fn to_handler(&self) -> &'static dyn CommandHandler {
        match self {
            Command::Help { .. } => &HELP_HANDLER,
            Command::Quit => &QUIT_HANDLER,
            Command::Clear => &CLEAR_HANDLER,
            Command::Context { subcommand } => subcommand.to_handler(),
            Command::Profile { subcommand } => subcommand.to_handler(),
            Command::Tools { subcommand } => match subcommand {
                Some(sub) => sub.to_handler(),
                None => &TOOLS_LIST_HANDLER,
            },
            Command::Compact { .. } => &COMPACT_HANDLER,
            Command::Usage => &USAGE_HANDLER,
            // Other commands...
        }
    }

    // Parse a command string into a Command enum
    pub fn parse(command_str: &str) -> Result<Self> {
        // Skip the leading slash if present
        let command_str = command_str.trim_start();
        let command_str = if command_str.starts_with('/') {
            &command_str[1..]
        } else {
            command_str
        };
        
        // Split into command and arguments
        let mut parts = command_str.split_whitespace();
        let command_name = parts.next().ok_or_else(|| anyhow!("Empty command"))?;
        let args: Vec<&str> = parts.collect();
        
        // Match on command name and use the handler to parse arguments
        match command_name {
            "help" => HELP_HANDLER.to_command(args),
            "quit" => QUIT_HANDLER.to_command(args),
            "clear" => CLEAR_HANDLER.to_command(args),
            "context" => CONTEXT_HANDLER.to_command(args),
            "profile" => PROFILE_HANDLER.to_command(args),
            "tools" => TOOLS_HANDLER.to_command(args),
            "compact" => COMPACT_HANDLER.to_command(args),
            "usage" => USAGE_HANDLER.to_command(args),
            // Other commands...
            _ => Err(anyhow!("Unknown command: {}", command_name)),
        }
    }
    
    // Execute the command directly
    pub async fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Result<ChatState> {
        // Get the appropriate handler and execute the command
        let handler = self.to_handler();
        handler.execute_command(self, ctx, tool_uses, pending_tool_index).await
    }
    
    // Generate LLM descriptions for all commands
    pub fn generate_llm_descriptions() -> serde_json::Value {
        let mut descriptions = json!({});
        
        // Use the static handlers to generate descriptions
        descriptions["help"] = HELP_HANDLER.llm_description();
        descriptions["quit"] = QUIT_HANDLER.llm_description();
        descriptions["clear"] = CLEAR_HANDLER.llm_description();
        descriptions["context"] = CONTEXT_HANDLER.llm_description();
        descriptions["profile"] = PROFILE_HANDLER.llm_description();
        descriptions["tools"] = TOOLS_HANDLER.llm_description();
        descriptions["compact"] = COMPACT_HANDLER.llm_description();
        descriptions["usage"] = USAGE_HANDLER.llm_description();
        // Other commands...
        
        descriptions
    }
}
```

### Integration with the `InternalCommand` Tool

```rust
impl Tool for InternalCommand {
    async fn invoke(&self, context: &Context, output: &mut impl Write) -> Result<InvokeOutput> {
        // Format the command string for execution
        let command_str = self.format_command_string();
        let description = self.get_command_description();

        // Create a response with the command and description
        let response = format!("Executing command for you: `{}` - {}", command_str, description);

        // Parse the command string into a Command enum directly
        let command = Command::parse(&command_str)?;
        
        // Log the parsed command
        debug!("Parsed command: {:?}", command);

        // Return an InvokeOutput with the response and next state
        Ok(InvokeOutput {
            output: crate::tools::OutputKind::Text(response),
            next_state: Some(ChatState::ExecuteCommand {
                command,
                tool_uses: None,
                pending_tool_index: None,
            }),
        })
    }
    
    fn requires_acceptance(&self, _ctx: &Context) -> bool {
        // Get the command handler
        let cmd = self.command.trim_start_matches('/');
        if let Ok(command) = Command::parse(&format!("{} {}", cmd, self.args.join(" "))) {
            // Check if the command requires confirmation
            let handler = command.to_handler();
            let args: Vec<&str> = self.args.iter().map(|s| s.as_str()).collect();
            return handler.requires_confirmation(&args);
        }
        
        // For commands not in the registry, default to requiring confirmation
        true
    }
}
```

## Enhanced Security Considerations

To ensure security when allowing AI to execute commands:

1. **Default to Requiring Confirmation**: All commands executed through `internal_command` require user confirmation by default if they are mutative, or automatically proceed if read-only.
2. **Permission Persistence**: Users can choose to trust specific commands using the existing permission system
3. **Command Auditing**: All commands executed by the AI are logged for audit purposes
4. **Scope Limitation**: Commands only have access to the same resources as when executed directly by the user
5. **Input Sanitization**: All command arguments are sanitized to prevent injection attacks
6. **Execution Context**: Commands run in the same security context as the application

## Implemented Commands

The following commands have been successfully implemented:

1. **Basic Commands**
   - `/help` - Show the help dialogue
   - `/quit` - Quit the application
   - `/clear` - Clear the conversation history

2. **Context Management**
   - `/context add` - Add a file to the context
   - `/context rm` - Remove a file from the context
   - `/context clear` - Clear all context files
   - `/context show` - Show all context files
   - `/context hooks` - Manage context hooks

3. **Profile Management**
   - `/profile list` - List available profiles
   - `/profile create` - Create a new profile
   - `/profile delete` - Delete a profile
   - `/profile set` - Switch to a different profile
   - `/profile rename` - Rename a profile
   - `/profile help` - Show profile help

4. **Tools Management**
   - `/tools list` - List available tools
   - `/tools trust` - Trust a specific tool
   - `/tools untrust` - Untrust a specific tool
   - `/tools trustall` - Trust all tools
   - `/tools reset` - Reset all tool permissions
   - `/tools reset_single` - Reset a single tool's permissions
   - `/tools help` - Show tools help

5. **Additional Commands**
   - `/issue` - Report an issue (using the existing report_issue tool)
   - `/compact` - Summarize conversation history
   - `/editor` - Open an external editor for composing prompts
   - `/usage` - Display token usage statistics

## Security Considerations

To ensure security:

1. The tool will only execute predefined internal commands
2. File system access will be limited to the same permissions as the user
3. Potentially destructive operations will require confirmation
4. Command execution will be logged for audit purposes

## Implementation Plan

### Phase 1: Core Implementation

1. Create the basic tool structure in `src/tools/internal_command/`
2. Implement command parsing and validation
3. Implement execution for session management commands
4. Add unit tests for basic functionality

### Phase 2: Extended Command Support

1. Implement context management commands
2. Implement settings management commands
3. Implement tool management commands
4. Add comprehensive tests for all command types

### Phase 3: Integration and Refinement

1. Integrate with the AI assistant's response generation
2. Add natural language understanding for command intent
3. Implement confirmation flows for potentially destructive operations
4. Add telemetry to track usage patterns

### Phase 4: Command Registry Enhancements

1. Add `to_command` method to the CommandHandler trait
2. Implement `to_command` for all command handlers
3. Add `command_to_handler` function to convert Command enums to handlers
4. Update the internal_command tool to use these new methods
5. Add tests to verify bidirectional conversion between Commands and Handlers

### Phase 5: Separation of Parsing and Output

1. Remove all output-related code from parsing functions
2. Move output-related code to execution functions
3. Update tests to verify the separation
4. Document the separation principle in the codebase

# Drawbacks

[drawbacks]: #drawbacks

There are several potential drawbacks to this feature:

1. **Security Risks**: Allowing the AI to execute commands directly could introduce security vulnerabilities if not properly constrained.

2. **User Confusion**: Users might not understand what actions the AI is taking on their behalf, leading to confusion or unexpected behavior.

3. **Implementation Complexity**: The feature requires careful integration with the existing command infrastructure and robust error handling.

4. **Maintenance Burden**: As new commands are added to the system, the `internal_command` tool will need to be updated to support them.

5. **Potential for Misuse**: Users might become overly reliant on the AI executing commands, reducing their understanding of the underlying system.

# Rationale and alternatives

[rationale-and-alternatives]: #rationale-and-alternatives

## Why this design?

This design provides a balance between flexibility and security:

1. It leverages the existing command infrastructure rather than creating a parallel system
2. It provides a structured interface for the AI to interact with the system
3. It maintains clear boundaries around what commands can be executed
4. It captures output and errors for proper feedback to the user
5. It establishes a bidirectional relationship between Command enums and CommandHandlers

## Alternatives Considered

### Enhanced Command Suggestions

Instead of executing commands directly, enhance the suggestion system to provide more detailed guidance. This was rejected because it still requires manual user action.

### Custom Command Aliases

Implement a system of aliases for common commands. This was rejected because it doesn't address the core issue of natural language understanding.

### Guided Command Builder

Implement a step-by-step command builder UI. This was rejected due to increased complexity and potential disruption to the chat flow.

### Separate Command Parsing Logic

Maintain separate command parsing logic in the internal_command tool. This was rejected because it would lead to duplication and potential inconsistencies.

## Impact of Not Doing This

Without this feature:

1. Users will continue to experience friction when trying to use commands
2. The learning curve for new users will remain steeper
3. The AI assistant will appear less capable compared to competitors
4. User productivity will be limited by the need to manually execute commands

# Unresolved questions

[unresolved-questions]: #unresolved-questions

1. How should we handle ambiguous commands where the user's intent is unclear?
2. What level of confirmation should be required for potentially destructive operations?
3. How should we handle commands that require interactive input?
4. Should there be a way for users to disable this feature if they prefer to execute commands manually?

# Future possibilities

[future-possibilities]: #future-possibilities

1. **Command Chaining**: Allow the AI to execute sequences of commands to accomplish more complex tasks.
2. **Custom Command Creation**: Enable users to define custom commands that the AI can execute.
3. **Contextual Command Suggestions**: Use conversation history to suggest relevant commands proactively.
4. **Cross-Session Command History**: Maintain a history of successful commands across sessions to improve future recommendations.
5. **Integration with External Tools**: Extend the command execution capability to interact with external tools and services.
6. **Natural Language Command Builder**: Develop a more sophisticated natural language understanding system to convert complex requests into command sequences.
7. **Command Explanation**: Add the ability for the AI to explain what a command does before executing it, enhancing user understanding.
8. **Command Undo**: Implement the ability to undo commands executed by the AI.

# Implementation Status

The implementation has been completed with the following key differences from the original RFC:

1. **Command-Centric Architecture**: Instead of using a central CommandRegistry, the implementation uses a command-centric architecture with a bidirectional relationship between Commands and Handlers. This approach:
   - Makes the command system more type-safe by using enum variants
   - Separates command parsing from execution
   - Creates a command-centric architecture with bidirectional relationships
   - Reduces dependency on a central registry
   - Ensures consistent behavior between direct command execution and tool-based execution

2. **Static Handler Instances**: Each command handler is defined as a static instance, which is referenced by the Command enum's `to_handler()` method. This approach:
   - Eliminates the need for a separate CommandRegistry
   - Provides a single point of modification for adding new commands
   - Maintains separation of concerns with encapsulated command logic
   - Ensures type safety with enum variants for command parameters

3. **Bidirectional Relationship**: The implementation establishes a bidirectional relationship between Commands and Handlers:
   - `handler.to_command(args)` converts arguments to Command enums
   - `command.to_handler()` gets the appropriate handler for a Command

4. **Additional Commands**: The implementation includes several commands not explicitly mentioned in the original RFC:
   - `/compact` - Summarize conversation history
   - `/editor` - Open an external editor for composing prompts
   - `/usage` - Display token usage statistics

5. **Issue Command Implementation**: Instead of implementing a separate command handler for the `/issue` command, the implementation leverages the existing `report_issue` tool functionality. This approach:
   - Reuses existing code
   - Ensures consistent behavior
   - Reduces the maintenance burden

6. **Command Execution Flow**: The command execution flow has been simplified:
   - The `internal_command` tool parses the command string into a Command enum
   - The Command enum is passed to the ChatState::ExecuteCommand state
   - The Command's `to_handler()` method is used to get the appropriate handler
   - The handler's `execute_command()` method is called to execute the command

7. **Separation of Parsing and Output**: A strict separation between parsing and output/display logic has been implemented:
   - Parsing functions only handle converting input strings to structured data
   - Output-related code (like displaying usage hints or deprecation warnings) is moved to the execution phase
   - This ensures clean, testable parsing logic and consistent user experience
