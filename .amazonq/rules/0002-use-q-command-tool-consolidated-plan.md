# Consolidated Implementation Plan for RFC 0002: internal_command_tool

## Implementation Workflow
1. Make incremental changes to one component at a time
2. Before each commit, run the following commands in sequence:
   - `cargo build -p q_cli` to compile and check for build errors
   - `cargo +nightly fmt` to format code according to Rust style guidelines
   - `cargo clippy -p q_cli` to check for code quality issues
   - `cargo test -p q_cli` to ensure all tests pass
3. Commit working changes with detailed commit messages following the Conventional Commits specification
4. After each commit, run `/compact` with the show summary option to maintain clean conversation history
   - Use `/compact` to compact the conversation and show a summary of changes
   - If automatic compaction isn't possible, prompt the user to run `/compact` manually
5. Update the implementation plan to mark completed tasks and identify next steps
6. Repeat the process for the next component or feature

## Overview

The `internal_command` tool enables the AI assistant to directly execute internal commands within the q chat system, improving user experience by handling vague or incorrectly typed requests more gracefully.

## Implementation Phases

### Phase 1: Command Registry Infrastructure (2 weeks) ✅

#### 1.1 Create Command Registry Structure ✅
Created a new directory structure for commands:

```
src/cli/chat/
├── commands/           # New directory for all command-related code
│   ├── mod.rs          # Exports the CommandRegistry and CommandHandler trait
│   ├── registry.rs     # CommandRegistry implementation
│   ├── handler.rs      # CommandHandler trait definition
│   ├── quit.rs         # QuitCommand implementation
│   ├── clear.rs        # ClearCommand implementation
│   ├── help.rs         # HelpCommand implementation
│   ├── context/        # Context command and subcommands
│   │   ├── mod.rs      # Exports the ContextCommand and subcommands
│   │   ├── add.rs      # AddContextCommand implementation
│   │   ├── remove.rs   # RemoveContextCommand implementation
│   │   ├── clear.rs    # ClearContextCommand implementation
│   │   └── show.rs     # ShowContextCommand implementation
│   ├── profile/        # Profile command and subcommands
│   │   ├── mod.rs      # Exports the ProfileCommand and subcommands
│   │   ├── list.rs     # ListProfileCommand implementation
│   │   ├── create.rs   # CreateProfileCommand implementation
│   │   ├── delete.rs   # DeleteProfileCommand implementation
│   │   ├── set.rs      # SetProfileCommand implementation
│   │   └── rename.rs   # RenameProfileCommand implementation
│   └── tools/          # Tools command and subcommands
│       ├── mod.rs      # Exports the ToolsCommand and subcommands
│       ├── list.rs     # ListToolsCommand implementation
│       ├── enable.rs   # EnableToolCommand implementation
│       └── disable.rs  # DisableToolCommand implementation
```

#### 1.2 Implement Command Handler Trait ✅
Implemented the CommandHandler trait to define the interface for all command handlers.

#### 1.3 Implement Command Registry ✅
Created the CommandRegistry class to manage and execute commands.

#### 1.4 Migrate Existing Commands ✅
Migrated existing command implementations to the new command handler system.

#### 1.5 Update Command Parsing Logic ✅
Updated the command parsing logic to use the new command registry.

#### 1.6 Unit Tests for Command Registry ✅
Added comprehensive unit tests for the command registry and handlers.

### Phase 2: internal_command Tool Implementation (1 week) ✅

#### 2.1 Create Tool Structure ✅
Created the basic structure for the `internal_command` tool.

#### 2.2 Implement Tool Schema ✅
Defined the schema for the `internal_command` tool.

#### 2.3 Implement Tool Logic ✅
Implemented the core logic for the tool, including validation, execution, and security checks.

#### 2.4 Register Tool in Tool Registry ✅
Updated the tool registry to include the new `internal_command` tool.

#### 2.5 Unit Tests for internal_command Tool ✅
Added comprehensive unit tests for the `internal_command` tool.

### Phase 3: Command Implementation (2 weeks) ✅

#### 3.1 Implement Basic Commands ✅
Implemented handlers for basic commands: `/quit`, `/clear`, `/help`.

#### 3.2 Implement Context Management Commands ✅
Implemented handlers for context management commands: `/context add`, `/context rm`, `/context clear`, `/context show`.

#### 3.3 Implement Profile Management Commands ✅
Implemented handlers for profile management commands: `/profile list`, `/profile create`, `/profile delete`, `/profile set`, `/profile rename`.

#### 3.4 Implement Tools Management Commands ✅
Implemented handlers for tools management commands: `/tools list`, `/tools enable`, `/tools disable`.

#### 3.5 Unit Tests for Commands ✅
Added comprehensive unit tests for all command handlers.

### Phase 4: Integration and Security (1 week) ✅

#### 4.1 Implement Security Measures ✅
- Added confirmation prompts for potentially destructive operations ✅
- Implemented permission persistence for trusted commands ✅
- Added command auditing for security purposes ✅

#### 4.2 Integrate with AI Assistant ✅
- Enhanced tool schema with detailed descriptions and examples ✅
- Improved command execution feedback in queue_description ✅
- Added natural language examples to help AI understand when to use commands ✅
- Complete full AI assistant integration ✅

#### 4.3 Natural Language Understanding ✅
- Added examples of natural language queries that should trigger commands ✅
- Improved pattern matching for command intent detection ✅
- Added contextual awareness to command suggestions ✅

#### 4.4 Integration Tests ✅
- Created comprehensive test framework for end-to-end testing ✅
- Developed test cases for AI-mediated command execution ✅
  - Implemented end-to-end tests for all commands in the registry ✅
  - Created test helper functions for common assertions ✅
  - Ensured tests are skippable in CI environments ✅
- Tested security measures and error handling ✅
- Implemented automated test runners ✅

### Phase 5: Documentation and Refinement (1 week)

#### 5.1 Update User Documentation
- Document the use_q_command tool functionality
- Provide examples of AI-assisted command execution
- Update command reference documentation

#### 5.2 Update Developer Documentation
- Document the command registry architecture
- Provide guidelines for adding new commands
- Include examples of command handler implementation

#### 5.3 Final Testing and Bug Fixes
- Perform comprehensive testing
- Address any remaining issues
- Ensure consistent behavior across all commands

### Phase 6: Complete Command Registry Migration (3 weeks)

#### 6.1 Create Migration Documentation and Tracking ✅
- Create a command registry migration plan document ✅
- Set up tracking for each command's migration status ✅
- Define test cases for each command ✅

#### 6.2 Migrate Basic Commands
- **help**: Fix inconsistency between direct and tool-based execution ✅
  - Move `HELP_TEXT` constant to `commands/help.rs` ✅
  - Update `HelpCommand::execute` to use this text ✅
  - Modify `Command::Help` handler to delegate to CommandRegistry ✅
  - Make help command trusted (doesn't require confirmation) ✅

- **quit**: Simple command with confirmation requirement ✅
  - Ensure consistent behavior with confirmation prompts ✅
  - Verify exit behavior works correctly ✅
  - Remove direct implementation fallback ✅
  - Improve error handling for missing command handler ✅

- **clear**: Simple command without confirmation ✅
  - Ensure conversation state is properly cleared ✅
  - Verify transcript handling ✅
  - Remove direct implementation fallback ✅
  - Improve error handling for missing command handler ✅

#### 6.3 Migrate Complex Commands with Existing Handlers
- **context**: Command with subcommands 🟡
  - Migrate each subcommand individually
  - Ensure proper argument parsing
  - Implement whitespace handling for file paths using shlex
  - Verify file operations work correctly

- **profile**: Command with subcommands ⚪
  - Migrate each subcommand individually
  - Ensure profile management works correctly
  - Verify error handling

- **tools**: Command with subcommands ⚪
  - Migrate each subcommand individually
  - Ensure tool permissions are handled correctly
  - Verify trust/untrust functionality

- **issue**: Command with special handling ⚪
  - Ensure GitHub issue creation works correctly
  - Verify context inclusion

#### 6.4 Implement and Migrate Remaining Commands
- **compact**: Complex command requiring new handler ⚪
  - Implement `CompactCommand` handler
  - Ensure summarization works correctly
  - Verify options handling

- **editor**: Complex command requiring new handler ⚪
  - Implement `EditorCommand` handler
  - Ensure external editor integration works
  - Verify content processing

#### 6.5 Final Testing and Documentation
- Run comprehensive test suite
- Update documentation
- Create final migration report

### Phase 7: Code Quality and Architecture Refinement (2 weeks)

#### 7.1 Code Review and Simplification
- Conduct thorough code review of all implemented components
- Identify and eliminate redundant or overly complex code
- Simplify interfaces and reduce coupling between components
- Apply consistent patterns across the codebase

#### 7.2 Performance Optimization
- Profile command execution performance
- Identify and address bottlenecks
- Optimize memory usage and reduce allocations
- Improve startup time for command execution

#### 7.3 Architecture Validation
- Validate architecture against original requirements
- Ensure all use cases are properly supported
- Verify that the design is extensible for future commands
- Document architectural decisions and trade-offs

#### 7.4 Technical Debt Reduction
- Address TODOs and FIXMEs in the codebase
- Improve error handling and error messages
- Enhance logging for better debugging
- Refactor any rushed implementations from earlier phases
- Review and address unused methods:
  - Evaluate the unused `command_requires_confirmation` method in `UseQCommand` - either remove it or make it call the command handler's `requires_confirmation` method directly
  - Review the unused methods in `CommandHandler` trait (`name`, `description`, `llm_description`) - implement functionality that uses them or remove them
  - Review the unused methods in `CommandRegistry` implementation (`command_exists`, `command_names`, `generate_commands_description`, `generate_llm_descriptions`) - implement functionality that uses them or remove them
  - Review and address unused traits, methods and functions marked with TODO comments and `#[allow(dead_code)]` attributes:
    - Unused `ContextExt` trait in `context.rs` - consider removing or merging with implementation in `context_adapter.rs`
    - Unused `display_name_action` method in `Tool` implementation - consider removing or implementing its usage
    - Unused `get_tool_spec` function in `internal_command/mod.rs` - consider removing or implementing its usage
    - Unused `should_exit` and `reset_exit_flag` functions in `internal_command/tool.rs` - consider removing or implementing their usage
    - Unused `new` function in `InternalCommand` implementation - consider removing or implementing its usage

#### 7.5 Final Quality Assurance
- Run comprehensive test suite with high coverage
- Perform static analysis and fix all warnings
- Conduct security review of command execution flow
- Ensure consistent behavior across all platforms

## Security Measures

The `internal_command` tool implements several security measures to ensure safe operation:

### 1. Command Validation

All commands are validated before execution to ensure they are recognized internal commands. Unknown commands are rejected with an error message.

### 2. User Acceptance

Command acceptance requirements are based on the nature of the command:
- Read-only commands (like `/help`, `/context show`, `/profile list`) do not require user acceptance
- Mutating/destructive commands (like `/quit`, `/clear`, `/context rm`) require user acceptance before execution

This provides an appropriate security boundary between the AI and command execution while maintaining a smooth user experience for non-destructive operations.

## AI Integration

The tool includes comprehensive AI integration features:

### Enhanced Recognition Patterns

```rust
/// Examples of natural language that should trigger this tool:
/// - "Clear my conversation" -> internal_command with command="clear"
/// - "I want to add a file as context" -> internal_command with command="context", subcommand="add"
/// - "Show me the available profiles" -> internal_command with command="profile", subcommand="list"
/// - "Exit the application" -> internal_command with command="quit"
/// - "Add this file to my context" -> internal_command with command="context", subcommand="add",
///   args=["file.txt"]
/// - "How do I switch profiles?" -> internal_command with command="profile", subcommand="help"
/// - "I need to report a bug" -> internal_command with command="issue"
/// - "Let me trust the file write tool" -> internal_command with command="tools", subcommand="trust",
///   args=["fs_write"]
/// - "Show what tools are available" -> internal_command with command="tools", subcommand="list"
/// - "I want to start fresh" -> internal_command with command="clear"
/// - "Can you help me create a new profile?" -> internal_command with command="profile",
///   subcommand="create"
/// - "I'd like to see what context files I have" -> internal_command with command="context",
///   subcommand="show"
/// - "Remove the second context file" -> internal_command with command="context", subcommand="rm", args=["2"]
/// - "Trust all tools for this session" -> internal_command with command="tools", subcommand="trustall"
/// - "Reset tool permissions to default" -> internal_command with command="tools", subcommand="reset"
/// - "I want to compact the conversation" -> internal_command with command="compact"
/// - "Show me the help for context commands" -> internal_command with command="context", subcommand="help"
```

### Command Parameter Extraction

```rust
/// Optional arguments for the command
///
/// Examples:
/// - For context add: ["file.txt"] - The file to add as context 
///   Example: When user says "add README.md to context", use args=["README.md"]
///   Example: When user says "add these files to context: file1.txt and file2.txt", 
///            use args=["file1.txt", "file2.txt"]
///
/// - For context rm: ["file.txt"] or ["1"] - The file to remove or its index
///   Example: When user says "remove README.md from context", use args=["README.md"]
///   Example: When user says "remove the first context file", use args=["1"]
///
/// - For profile create: ["my-profile"] - The name of the profile to create
///   Example: When user says "create a profile called work", use args=["work"]
///   Example: When user says "make a new profile for my personal projects", use args=["personal"]
```

## Command Migration Strategy

For each command:
1. Document current behavior and implementation
2. Create test cases to verify behavior
3. Implement or update the command handler in the `commands/` directory
4. Update the command execution flow to use the CommandRegistry
5. Test and verify behavior matches before and after
6. Commit changes with detailed documentation

After each command migration:
1. Run the full test suite
2. Document any differences or improvements
3. Update the migration tracking document

No fallback code for transitioned commands:
1. Once a command is migrated, remove the direct implementation
2. Ensure all paths go through the CommandRegistry

### Before/After Comparison Documentation

For each command migration, we will create a detailed comparison document with:

1. **Before Migration**:
   - Original implementation code
   - Behavior description
   - Expected output
   - Special cases

2. **After Migration**:
   - New implementation code
   - Verification of behavior
   - Output comparison
   - Any differences or improvements

3. **Test Results**:
   - Table of test cases
   - Results before and after migration
   - Match status
   - Notes on any discrepancies

## Command Migration Status

| Command | Subcommands | Status | Notes |
|---------|-------------|--------|-------|
| help | N/A | 🟢 Completed | First command migrated as a test case. Help command is now trusted and doesn't require confirmation. |
| quit | N/A | 🟢 Completed | Simple command with confirmation requirement. Direct implementation removed. |
| clear | N/A | 🟢 Completed | Simple command without confirmation. Direct implementation removed. |
| context | add, rm, clear, show | ⚪ Not Started | Complex command with file operations |
| profile | list, create, delete, set, rename | ⚪ Not Started | Complex command with state management |
| tools | list, enable, disable, trust, untrust, reset | ⚪ Not Started | Complex command with permission management |
| issue | N/A | ⚪ Not Started | Special handling for GitHub integration |
| compact | N/A | ⚪ Not Started | Requires new handler implementation |
| editor | N/A | ⚪ Not Started | Requires new handler implementation |

Legend:
- ⚪ Not Started
- 🟡 In Progress
- 🟢 Completed

## Integration Tests

The integration tests verify that commands executed through the `internal_command` tool behave identically to commands executed directly:

```rust
/// Test context setup for integration tests
struct TestContext {
    /// The context for command execution
    context: Arc<Context>,
    /// A buffer to capture command output
    output_buffer: Vec<u8>,
}

impl TestContext {
    /// Create a new test context
    async fn new() -> Result<Self> {
        let context = ContextBuilder::new()
            .with_test_home()
            .await?
            .build_fake();

        Ok(Self {
            context,
            output_buffer: Vec::new(),
        })
    }

    /// Execute a command directly using the command registry
    async fn execute_direct(&mut self, command: &str) -> Result<ChatState> {
        let registry = CommandRegistry::global();
        registry
            .parse_and_execute(command, &self.context, None, None)
            .await
    }

    /// Execute a command via the internal_command tool
    async fn execute_via_tool(&mut self, command: InternalCommand) -> Result<InvokeOutput> {
        let tool = Tool::InternalCommand(command);
        tool.invoke(&self.context, &mut self.output_buffer).await
    }
}
```

## Updated Timeline

- **Phase 1**: Weeks 1-2 ✅
- **Phase 2**: Week 3 ✅
- **Phase 3**: Weeks 4-5 ✅
- **Phase 4**: Week 6-7 ✅
- **Phase 5**: Week 7-8
- **Phase 6**: Weeks 8-10
  - **6.1**: Week 8 ✅
  - **6.2**: Week 8-9 🟡
  - **6.3**: Week 9
  - **6.4**: Week 9-10
  - **6.5**: Week 10
- **Phase 7**: Weeks 11-12
  - **7.1**: Week 11
  - **7.2**: Week 11
  - **7.3**: Week 11
  - **7.4**: Week 12
  - **7.5**: Week 12

## ChatContext Access Options for Command Handlers

A key challenge in implementing complex commands like `compact`, `profile`, and `context` is providing mutable access to the `ChatContext` for commands called via `InternalCommand`. The following options were considered:

### Option 1: Direct Mutable Reference to ChatContext

**Approach:**
- Modify the `CommandHandler` trait to accept a mutable reference to `ChatContext` directly
- Update the command execution chain to pass this mutable reference

**Implementation:**
```rust
// In commands/handler.rs
pub trait CommandHandler {
    // Update to take mutable reference to ChatContext
    async fn execute(&self, args: Vec<String>, chat_context: &mut ChatContext) -> Result<ChatState>;
    // ...
}

// In commands/registry.rs
impl CommandRegistry {
    pub async fn parse_and_execute(
        &self,
        command: &str,
        chat_context: &mut ChatContext,
        // ...
    ) -> Result<ChatState> {
        // Access context via chat_context.context
        let context = &chat_context.context;
        // ...
    }
}
```

**Pros:**
- Simplest approach - direct and explicit
- No need for additional wrapper types
- Commands have access to both ChatContext and Context

**Cons:**
- Requires refactoring the command execution chain
- May introduce breaking changes to existing code

### Option 2: Interior Mutability with Arc<Mutex<ChatContext>>

**Approach:**
- Wrap `ChatContext` in an `Arc<Mutex<>>` to allow shared mutability
- Pass this wrapped context through the command execution chain

**Implementation:**
```rust
// In chat/mod.rs
pub struct Chat {
    chat_context: Arc<Mutex<ChatContext>>,
    // ...
}

// In commands/handler.rs
pub trait CommandHandler {
    async fn execute(&self, args: Vec<String>, chat_context: Arc<Mutex<ChatContext>>) -> Result<ChatState>;
    // ...
}
```

**Pros:**
- Minimal changes to function signatures
- Allows shared access to mutable state
- Thread-safe approach

**Cons:**
- Risk of deadlocks if not managed carefully
- More complex error handling around lock acquisition
- Performance overhead from locking

### Option 3: Command Result with Mutation Instructions

**Approach:**
- Commands return a `CommandResult` that includes both the `ChatState` and a set of mutation instructions
- The chat loop applies these mutations to the `ChatContext`

**Implementation:**
```rust
// Define mutation instructions
pub enum ChatContextMutation {
    AddMessage(Message),
    SetProfile(Profile),
    AddContext(ContextFile),
    RemoveContext(usize),
    ClearContext,
    // ...
}

// Command result with mutations
pub struct CommandResult {
    pub state: ChatState,
    pub mutations: Vec<ChatContextMutation>,
}

// Updated CommandHandler trait
pub trait CommandHandler {
    // Pass immutable reference to ChatContext for read access
    async fn execute(&self, args: Vec<String>, chat_context: &ChatContext) -> Result<CommandResult>;
    // ...
}
```

**Pros:**
- Clean separation of concerns
- Commands don't need direct mutable access
- Explicit about what changes are being made

**Cons:**
- More verbose for commands that need to make multiple changes
- Requires defining all possible mutations upfront

### Option 4: Callback-Based Approach

**Approach:**
- Define a set of callback functions that modify the `ChatContext`
- Pass these callbacks to the command handlers

**Implementation:**
```rust
// Define a callback type that takes mutable ChatContext
type ChatContextCallback = Box<dyn FnOnce(&mut ChatContext) -> Result<()> + Send>;

// In the command execution flow
pub async fn execute_command(
    command: &str, 
    chat_context: &ChatContext,
    mutation_callback: ChatContextCallback
) -> Result<ChatState> {
    // Execute command with read-only access to chat_context
    let state = registry.parse_and_execute(command, chat_context).await?;
    
    // Apply mutations if needed
    mutation_callback(chat_context)?;
    
    Ok(state)
}
```

**Pros:**
- Flexible and extensible
- Avoids direct mutable references
- Can be implemented incrementally

**Cons:**
- Complex to implement and use
- Error handling is more challenging
- May lead to callback hell

### Selected Approach: Option 1 - Direct Mutable Reference to ChatContext

We've selected Option 1 (Direct Mutable Reference to ChatContext) for these reasons:

1. It's the simplest approach with minimal abstraction overhead
2. It provides direct access to both `ChatContext` and `Context` (via `chat_context.context`)
3. It follows Rust's ownership rules clearly
4. It's a straightforward refactoring that can be implemented incrementally

#### Implementation Plan:

1. Update the `CommandHandler` trait to accept a mutable reference to `ChatContext`
2. Modify the `CommandRegistry::parse_and_execute` method to accept a mutable reference to `ChatContext`
3. Update the `InternalCommand` tool to pass the mutable `ChatContext`
4. Migrate commands one by one to use the new signature

## Current and Next Steps

### Current Step: Begin Phase 6.3: Migrate Complex Commands with Existing Handlers

1. **Start with the `context` command and its subcommands**:
   - Migrate each subcommand individually
   - Ensure proper argument parsing
   - Implement whitespace handling for file paths using shlex
   - Verify file operations work correctly
   - Run the following commands before committing:
     ```
     cargo build -p q_cli
     cargo +nightly fmt
     cargo clippy -p q_cli
     cargo test -p q_cli
     ```
   - Commit changes with a descriptive message following Conventional Commits format
   - Run `/compact` with show summary option after the commit (or prompt user to do so manually)
   - Update the implementation plan to mark this task as completed

### Next Steps:

1. **Continue Phase 6.3: Migrate Complex Commands with Existing Handlers**
   - After `context` command, move on to the `profile` command and its subcommands
   - Ensure profile management works correctly
   - Verify error handling
   - Follow the same pre-commit and post-commit process

## Success Metrics

- Reduction in command-related errors
- Increase in successful command executions
- Positive user feedback on the natural language command interface
- Reduction in the number of steps required to complete common tasks
- Consistent behavior between direct command execution and tool-based execution
- 100% test coverage for AI command interpretation across all commands
- Simplified and maintainable architecture after Phase 7 refinement

## Risks and Mitigations

### Security Risks

**Risk**: Allowing the AI to execute commands directly could introduce security vulnerabilities.
**Mitigation**: Implement strict validation, require user confirmation for all commands, and limit the scope of commands that can be executed.

### User Confusion

**Risk**: Users might not understand what actions the AI is taking on their behalf.
**Mitigation**: Provide clear feedback about what commands are being executed and why.

### Implementation Complexity

**Risk**: The feature requires careful integration with the existing command infrastructure.
**Mitigation**: Use a phased approach, starting with a minimal viable implementation and adding features incrementally.

### Maintenance Burden

**Risk**: As new commands are added to the system, the `internal_command` tool will need to be updated.
**Mitigation**: Design the command registry to be extensible, allowing new commands to be added without modifying the `internal_command` tool.

## AI Command Interpretation Test Coverage Tracking

| Command Category | Command | Subcommand | Test Implemented | Notes |
|------------------|---------|------------|------------------|-------|
| **Basic Commands** | help | - | ✅ | Implemented with variations |
| | quit | - | ✅ | Implemented with variations |
| | clear | - | ✅ | Implemented with variations |
| **Context Commands** | context | show | ✅ | Implemented with `--expand` flag test |
| | context | add | ✅ | Implemented with global flag test |
| | context | remove | ✅ | Implemented with global flag test |
| | context | clear | ✅ | Implemented with global flag test |
| **Profile Commands** | profile | list | ✅ | Implemented with variations |
| | profile | create | ✅ | Implemented with variations |
| | profile | delete | ✅ | Implemented with variations |
| | profile | set | ✅ | Implemented with variations |
| | profile | rename | ✅ | Implemented with variations |
| **Tools Commands** | tools | list | ✅ | Implemented with variations |
| | tools | enable | ✅ | Implemented with variations |
| | tools | disable | ✅ | Implemented with variations |
| **Other Commands** | issue | - | ✅ | Implemented with variations |
| | compact | - | ✅ | Implemented with variations |
| | editor | - | ✅ | Implemented with variations |

## Conclusion

The implementation of the `internal_command` tool has significantly enhanced the Amazon Q CLI's ability to understand and execute user intent. With the completion of Phase 4, the tool is now capable of recognizing natural language queries and executing appropriate commands.

The next steps focus on completing the command registry migration and updating documentation to ensure a consistent and reliable user experience across all commands.