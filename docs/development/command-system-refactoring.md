# Command System Refactoring Plan

## Overview

This document outlines the plan for refactoring the command system to use a Command enum with embedded CommandHandlers. This approach will reduce the number of places that need modification when adding new commands while maintaining separation of concerns.

## Implementation Steps

### Phase 1: Design and Planning

1. **Document Current Architecture**
   - Map out the current Command enum structure
   - Document existing CommandHandler implementations
   - Identify dependencies and integration points

2. **Design New Architecture**
   - Design the enhanced Command enum with handler access
   - Define the static handler pattern
   - Design the simplified CommandRegistry interface

3. **Create Migration Plan**
   - Identify commands to migrate
   - Prioritize commands based on complexity and usage
   - Create test cases for each command

### Phase 2: Core Implementation

1. **Implement Command Enum Enhancement**
   - Add `get_handler()` method to Command enum
   - Add `to_args()` method to convert enum variants to argument lists
   - Add `execute()` method that delegates to the handler

2. **Implement Static Handlers**
   - Create static instances of each CommandHandler
   - Ensure thread safety and proper initialization
   - Link handlers to Command enum variants

3. **Update Subcommand Enums**
   - Add `get_handler()` method to each subcommand enum
   - Add `to_args()` method to convert subcommands to argument lists
   - Link subcommand handlers to subcommand enum variants

### Phase 3: CommandRegistry Simplification

1. **Simplify CommandRegistry**
   - Remove the HashMap-based storage of handlers
   - Update `parse_and_execute()` to use Command enum methods
   - Update `generate_llm_descriptions()` to use Command enum methods

2. **Update Integration Points**
   - Update the internal_command tool to work with the new architecture
   - Update any code that directly accesses the CommandRegistry
   - Ensure backward compatibility where needed

### Phase 4: Command Migration

1. **Migrate Basic Commands**
   - Help command
   - Quit command
   - Clear command

2. **Migrate Complex Commands**
   - Context command and subcommands
   - Profile command and subcommands
   - Tools command and subcommands

3. **Migrate Newer Commands**
   - Compact command
   - Usage command
   - Editor command

### Phase 5: Testing and Refinement

1. **Comprehensive Testing**
   - Test each command individually
   - Test command combinations and sequences
   - Test edge cases and error handling

2. **Performance Optimization**
   - Profile command execution performance
   - Optimize handler lookup and execution
   - Reduce memory usage where possible

3. **Documentation Update**
   - Update developer documentation
   - Document the new architecture
   - Provide examples for adding new commands

## Implementation Details

### Enhanced Command Enum

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
    
    // Convert command to arguments for the handler
    fn to_args(&self) -> Vec<&str> {
        // Implementation for each command variant
    }
    
    // Generate LLM descriptions for all commands
    pub fn generate_llm_descriptions() -> serde_json::Value {
        // Implementation that collects descriptions from all handlers
    }
}
```

### Simplified CommandRegistry

```rust
pub struct CommandRegistry;

impl CommandRegistry {
    pub fn global() -> &'static Self {
        static INSTANCE: OnceLock<CommandRegistry> = OnceLock::new();
        INSTANCE.get_or_init(|| CommandRegistry)
    }
    
    pub async fn parse_and_execute(
        &self,
        command_str: &str,
        ctx: &mut CommandContextAdapter,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Result<ChatState> {
        let command = Command::parse(command_str)?;
        command.execute(ctx, tool_uses, pending_tool_index).await
    }
    
    pub fn generate_llm_descriptions(&self) -> serde_json::Value {
        Command::generate_llm_descriptions()
    }
}
```

## Benefits of This Approach

1. **Single Point of Modification**: When adding a new command, you primarily modify the Command enum and add a new static handler

2. **Separation of Concerns**: Each command's logic is still encapsulated in its own handler

3. **Type Safety**: Command parameters are directly encoded in the enum variants

4. **Reuse Existing Handlers**: You can reuse your existing CommandHandler implementations

5. **Consistent Behavior**: Commands behave the same whether invoked directly or through the tool

6. **LLM Integration**: The llm_description() method in each handler is still used for generating tool descriptions

## Timeline

- **Phase 1**: 1 week
- **Phase 2**: 2 weeks
- **Phase 3**: 1 week
- **Phase 4**: 2 weeks
- **Phase 5**: 1 week

Total: 7 weeks

## Success Metrics

- Reduced number of places that need modification when adding a new command
- Consistent behavior between direct command execution and tool-based execution
- Improved code maintainability and readability
- Successful execution of all existing commands with the new architecture
- Comprehensive test coverage for all commands
