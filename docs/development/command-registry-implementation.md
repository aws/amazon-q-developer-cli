# Command Registry Implementation

This document provides detailed information about the implementation of the Command Registry system in the Amazon Q CLI.

## Implementation Phases

### Phase 1: Command Registry Infrastructure ✅

#### Command Registry Structure
We created a new directory structure for commands:

```
crates/q_chat/src/
├── commands/           # Directory for all command-related code
│   ├── mod.rs          # Exports the CommandRegistry and CommandHandler trait
│   ├── registry.rs     # CommandRegistry implementation
│   ├── handler.rs      # CommandHandler trait definition
│   ├── context_adapter.rs # CommandContextAdapter implementation
│   ├── quit.rs         # QuitCommand implementation
│   ├── clear.rs        # ClearCommand implementation
│   ├── help.rs         # HelpCommand implementation
│   ├── compact.rs      # CompactCommand implementation
│   ├── context/        # Context command and subcommands
│   │   └── mod.rs      # ContextCommand implementation
│   ├── profile/        # Profile command and subcommands
│   │   └── mod.rs      # ProfileCommand implementation
│   └── tools/          # Tools command and subcommands
│       └── mod.rs      # ToolsCommand implementation
├── tools/              # Tool implementations
│   ├── mod.rs          # Tool trait and registry
│   ├── internal_command/ # Internal command tool
│   │   ├── mod.rs      # Tool definition and schema
│   │   ├── tool.rs     # Tool implementation
│   │   └── schema.rs   # Schema definition
│   ├── fs_read.rs      # File system read tool
│   ├── fs_write.rs     # File system write tool
│   └── ...             # Other tools
```

#### CommandHandler Trait
The CommandHandler trait defines the interface for all command handlers:

```rust
pub trait CommandHandler: Send + Sync {
    /// Returns the name of the command
    fn name(&self) -> &'static str;

    /// Returns a short description of the command for help text
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

    /// Execute the command with the given arguments
    fn execute<'a>(
        &'a self,
        args: Vec<&'a str>,
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState>> + Send + 'a>>;

    /// Check if this command requires confirmation before execution
    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        true // Most commands require confirmation by default
    }

    /// Parse arguments for this command
    fn parse_args<'a>(&self, args: Vec<&'a str>) -> Result<Vec<&'a str>> {
        Ok(args)
    }
}
```

### Phase 2: internal_command Tool Implementation ✅

The `internal_command` tool enables the AI assistant to directly execute internal commands within the q chat system, improving user experience by handling vague or incorrectly typed requests more gracefully.

#### Tool Schema
The tool schema defines the parameters for the internal_command tool:

```rust
{
  "command": "The command to execute (without the leading slash)",
  "subcommand": "Optional subcommand for commands that support them",
  "args": ["Optional arguments for the command"],
  "flags": {"Optional flags for the command"}
}
```

#### Security Measures

1. **Command Validation**: All commands are validated before execution to ensure they are recognized internal commands.

2. **User Acceptance**: Command acceptance requirements are based on the nature of the command:
   - Read-only commands (like `/help`, `/context show`, `/profile list`) do not require user acceptance
   - Mutating/destructive commands (like `/quit`, `/clear`, `/context rm`) require user acceptance before execution

### Phase 3: Command Implementation ✅

We implemented handlers for all basic commands and many complex commands:

1. **Basic Commands**:
   - `/help`: Display help information
   - `/quit`: Exit the application
   - `/clear`: Clear conversation history

2. **Complex Commands**:
   - `/context`: Manage context files (add, rm, clear, show)
   - `/compact`: Summarize conversation history
   - `/usage`: Display token usage statistics
   - `/issue`: Create GitHub issues (using existing report_issue tool)

### Phase 4: Integration and Security ✅

1. **Security Measures**:
   - Added confirmation prompts for potentially destructive operations
   - Implemented permission persistence for trusted commands
   - Added command auditing for security purposes

2. **AI Integration**:
   - Enhanced tool schema with detailed descriptions and examples
   - Added natural language examples to help AI understand when to use commands

3. **Natural Language Understanding**:
   - Added examples of natural language queries that should trigger commands
   - Improved pattern matching for command intent detection

## Command Result Approach

After evaluating various options for integrating the `internal_command` tool with the existing command execution flow, we selected a streamlined approach that leverages the existing `Command` enum and command execution logic:

1. The `internal_command` tool parses input parameters into the existing `Command` enum structure
2. The tool returns a `CommandResult` containing the parsed command
3. The chat loop extracts the command from the result and executes it using existing command execution logic

### CommandResult Structure

```rust
/// Result of a command execution from the internal_command tool
#[derive(Debug, Serialize, Deserialize)]
pub struct CommandResult {
    /// The command to execute
    pub command: Command,
}

impl CommandResult {
    /// Create a new command result with the given command
    pub fn new(command: Command) -> Self {
        Self { command }
    }
}
```

## Command Migration Status

| Command | Subcommands | Status | Notes |
|---------|-------------|--------|-------|
| help | N/A | ✅ Completed | Help command is now trusted and doesn't require confirmation |
| quit | N/A | ✅ Completed | Simple command with confirmation requirement |
| clear | N/A | ✅ Completed | Simple command without confirmation |
| context | add, rm, clear, show, hooks | 🟡 In Progress | Complex command with file operations |
| profile | list, create, delete, set, rename | ⚪ Not Started | Complex command with state management |
| tools | list, trust, untrust, trustall, reset | ⚪ Not Started | Complex command with permission management |
| issue | N/A | ✅ Completed | Using existing report_issue tool |
| compact | N/A | ✅ Completed | Command for summarizing conversation history |
| editor | N/A | ⚪ Not Started | Requires new handler implementation |
| usage | N/A | ✅ Completed | New command for displaying context window usage |

## Future Refactoring Plan

For future refactoring, we plan to implement a Command enum with embedded CommandHandlers:

```rust
pub enum Command {
    Help { help_text: Option<String> },
    Quit,
    Clear,
    Context { subcommand: ContextSubcommand },
    Profile { subcommand: ProfileSubcommand },
    Tools { subcommand: Option<ToolsSubcommand> },
    Compact { prompt: Option<String>, show_summary: bool, help: bool },
    Usage,
    // New commands would be added here
}

impl Command {
    // Get the appropriate handler for this command
    pub fn get_handler(&self) -> &dyn CommandHandler {
        match self {
            Command::Help { .. } => &HELP_HANDLER,
            Command::Quit => &QUIT_HANDLER,
            Command::Clear => &CLEAR_HANDLER,
            Command::Context { subcommand } => subcommand.get_handler(),
            Command::Profile { subcommand } => subcommand.get_handler(),
            Command::Tools { subcommand } => match subcommand {
                Some(sub) => sub.get_handler(),
                None => &TOOLS_LIST_HANDLER,
            },
            Command::Compact { .. } => &COMPACT_HANDLER,
            Command::Usage => &USAGE_HANDLER,
        }
    }
    
    // Execute the command using its handler
    pub async fn execute<'a>(
        &'a self,
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Result<ChatState> {
        let handler = self.get_handler();
        let args = self.to_args();
        handler.execute(args, ctx, tool_uses, pending_tool_index).await
    }
}
```

This approach will reduce the number of places that need modification when adding new commands while maintaining separation of concerns.

## Benefits of the Command Registry System

1. **Consistent Behavior**: Commands behave the same whether invoked directly or through the tool
2. **Separation of Concerns**: Each command's logic is encapsulated in its own handler
3. **Extensibility**: New commands can be added easily by implementing the CommandHandler trait
4. **Natural Language Support**: Commands can be invoked using natural language through the internal_command tool
5. **Improved User Experience**: Users can interact with the CLI using natural language
