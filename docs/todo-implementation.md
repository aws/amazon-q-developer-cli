# To-Do List Implementation Details

This document provides technical details about the to-do list functionality implementation in Amazon Q CLI.

## Architecture Overview

The to-do list functionality is implemented as a built-in tool (`todo_list`) that provides persistent task management across chat sessions. The implementation consists of several key components:

### Core Components

- **TodoList Tool** (`crates/chat-cli/src/cli/chat/tools/todo.rs`) - Main tool implementation
- **TodoSubcommand** (`crates/chat-cli/src/cli/chat/cli/todos.rs`) - Slash command interface
- **TodoListState** - Serializable state structure for persistence

### Tool Integration

The `todo_list` tool is integrated into the tool manager and appears in agent tool listings with "trusted" status by default. It requires no special configuration and is automatically available to all agents.

## Data Structures

### TodoListState

```rust
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct TodoListState {
    pub tasks: Vec<String>,
    pub completed: Vec<bool>,
    pub description: String,
    pub context: Vec<String>,
    pub modified_files: Vec<String>,
    pub id: String,
}
```

### TodoList Commands

The tool supports five main commands:

- `create` - Initialize new to-do list
- `complete` - Mark tasks as finished
- `load` - Load existing list by ID
- `add` - Insert new tasks
- `remove` - Delete tasks

## Storage Implementation

### File System Layout

```
.amazonq/cli-todo-lists/
├── 1692123456789.json
├── 1692123567890.json
└── ...
```

### ID Generation

To-do list IDs are generated using Unix timestamps in milliseconds:

```rust
pub fn generate_new_todo_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis();
    format!("{timestamp}")
}
```

### Automatic Directory Creation

The storage directory is automatically created during chat session initialization:

```rust
// In ChatSession::new()
let _ = TodoListState::init_dir(os).await;
```

## Slash Command Interface

### Command Structure

```
/todos <subcommand>
```

Available subcommands:
- `resume` - Interactive selection and resumption
- `view` - Non-destructive list viewing
- `delete [--all]` - Single or bulk deletion
- `clear-finished` - Remove completed lists only

### Interactive Selection

The interface uses `dialoguer::FuzzySelect` for user-friendly list selection:

```rust
fn fuzzy_select_todos(entries: &[TodoDisplayEntry], prompt_str: &str) -> Option<usize> {
    FuzzySelect::new()
        .with_prompt(prompt_str)
        .items(entries)
        .report(false)
        .interact_opt()
        .unwrap_or(None)
}
```

## Resume Functionality

### Automatic Resume Process

When a user selects `/todos resume`:

1. **List Discovery** - Scan `.amazonq/cli-todo-lists/` for incomplete lists
2. **Interactive Selection** - Present fuzzy-searchable interface
3. **Context Loading** - Load selected list state and context
4. **Tool Invocation** - Automatically call `todo_list load` command
5. **Session Continuation** - Q resumes work from previous context

### Resume Implementation

```rust
pub async fn resume_todo_request(&mut self, os: &mut Os, id: &str) -> Result<ChatState, ChatError> {
    let todo_list = TodoListState::load(os, id).await?;
    let contents = serde_json::to_string(&todo_list)?;
    
    let summary_content = format!(
        "[SYSTEM NOTE: This is an automated request, not from the user]\n
        Read the TODO list contents below and understand the task description, completed tasks, and provided context.\n 
        Call the `load` command of the todo_list tool with the given ID as an argument to display the TODO list to the user and officially resume execution of the TODO list tasks.\n
        You do not need to display the tasks to the user yourself. You can begin completing the tasks after calling the `load` command.\n
        TODO LIST CONTENTS: {}\n
        ID: {}\n",
        contents, id
    );
    
    // Create automated message and continue session
    // ...
}
```

## Conversation Integration

### Summary Enhancement

Conversation summaries automatically include active to-do list IDs:

```rust
// In conversation summary generation
"5) REQUIRED: the ID of the currently loaded todo list, if any\n\n\
FORMAT THE SUMMARY IN THIRD PERSON, NOT AS A DIRECT RESPONSE. Example format:\n\n\
## CONVERSATION SUMMARY\n\
* Topic 1: Key information\n\
* Topic 2: Key information\n\n\
## TOOLS EXECUTED\n\
* Tool X: Result Y\n\n\
## TODO ID\n\
* <id>\n\n\
Remember this is a DOCUMENT not a chat response."
```

### Tool Filtering

During resume operations, only the `todo_list` tool is made available to ensure focused execution:

```rust
let mut tools = self.conversation.tools.clone();
tools.retain(|k, v| match k {
    ToolOrigin::Native => {
        v.retain(|tool| match tool {
            api_client::model::Tool::ToolSpecification(tool_spec) => tool_spec.name == "todo_list",
        });
        true
    },
    ToolOrigin::McpServer(_) => false,
});
```

## Display and UI

### Visual Indicators

The implementation uses crossterm for styled terminal output:

- **Incomplete tasks**: `☐` (empty checkbox)
- **Completed tasks**: `■` (filled checkbox, green, italic)
- **Progress indicators**: `(completed/total)` format
- **Status symbols**: `✗` (red) for incomplete, `✓` (green) for complete

### Display Implementation

```rust
fn queue_next_without_newline(output: &mut impl Write, task: String, completed: bool) -> Result<()> {
    if completed {
        queue!(
            output,
            style::SetAttribute(style::Attribute::Italic),
            style::SetForegroundColor(style::Color::Green),
            style::Print(" ■ "),
            style::SetForegroundColor(style::Color::DarkGrey),
            style::Print(task),
            style::SetAttribute(style::Attribute::NoItalic),
        )?;
    } else {
        queue!(
            output,
            style::SetForegroundColor(style::Color::Reset),
            style::Print(format!(" ☐ {task}")),
        )?;
    }
    Ok(())
}
```

## Error Handling

### Validation

Each command includes comprehensive validation:

- **Index bounds checking** - Ensures task indices are valid
- **Empty content validation** - Prevents empty tasks or descriptions
- **Duplicate index detection** - Uses HashSet for uniqueness validation
- **File system error handling** - Graceful handling of I/O operations

### Error Recovery

The system provides graceful degradation:

- **Corrupted files** - Individual file errors don't prevent listing other to-do lists
- **Missing directories** - Automatic creation on first use
- **Permission issues** - Clear error messages for file system problems

## Performance Considerations

### File System Operations

- **Lazy loading** - To-do lists are loaded only when accessed
- **Batch operations** - Multiple file operations are grouped when possible
- **Error collection** - Failed operations are collected rather than failing fast

### Memory Management

- **Streaming directory reads** - Uses async iterators for large directories
- **Minimal state retention** - Only active to-do list state is kept in memory
- **JSON serialization** - Efficient serialization with serde

## Security Considerations

### File System Access

- **Scoped storage** - All files are contained within `.amazonq/cli-todo-lists/`
- **Path validation** - ID-to-path conversion prevents directory traversal
- **Automatic cleanup** - No sensitive data is stored in to-do lists

### Trust Model

- **Default trust** - The tool is trusted by default for user convenience
- **No network access** - All operations are local file system only
- **User control** - Users can delete or modify lists through slash commands

## Testing and Validation

### Unit Tests

The implementation includes comprehensive validation for:

- **Command parameter validation**
- **Index bounds checking**
- **File system error handling**
- **State serialization/deserialization**

### Integration Points

- **Tool manager integration** - Proper registration and invocation
- **Slash command routing** - Correct command parsing and execution
- **Session state management** - Proper integration with chat sessions
