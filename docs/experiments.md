# Experimental Features

Amazon Q CLI includes experimental features that can be toggled on/off using the `/experiment` command. These features are in active development and may change or be removed at any time.

## Available Experiments

### Knowledge
**Command:** `/knowledge`  
**Description:** Enables persistent context storage and retrieval across chat sessions

**Features:**
- Store and search through files, directories, and text content
- Semantic search capabilities for better context retrieval  
- Persistent knowledge base across chat sessions
- Add/remove/search knowledge contexts

**Usage:**
```
/knowledge add <path>        # Add files or directories to knowledge base
/knowledge show             # Display knowledge base contents
/knowledge remove <path>    # Remove knowledge base entry by path
/knowledge update <path>    # Update a file or directory in knowledge base
/knowledge clear            # Remove all knowledge base entries
/knowledge status           # Show background operation status
/knowledge cancel           # Cancel background operation
```

### Thinking
**Description:** Enables complex reasoning with step-by-step thought processes

**Features:**
- Shows AI reasoning process for complex problems
- Helps understand how conclusions are reached
- Useful for debugging and learning
- Transparent decision-making process

**When enabled:** The AI will show its thinking process when working through complex problems or multi-step reasoning.

### Tangent Mode
**Command:** `/tangent`  
**Description:** Enables conversation checkpointing for exploring tangential topics

**Features:**
- Create conversation checkpoints to explore side topics
- Return to the main conversation thread at any time
- Preserve conversation context while branching off
- Keyboard shortcut support (default: Ctrl+T)

**Usage:**
```
/tangent                    # Toggle tangent mode on/off
```

**Settings:**
- `chat.enableTangentMode` - Enable/disable tangent mode feature (boolean)
- `chat.tangentModeKey` - Keyboard shortcut key (single character, default: 't')
- `introspect.tangentMode` - Auto-enter tangent mode for introspect questions (boolean)

**When enabled:** Use `/tangent` or the keyboard shortcut to create a checkpoint and explore tangential topics. Use the same command to return to your main conversation.

### TODO Lists
**Tool name**: `todo_list`
**Command:** `/todos`  
**Description:** Enables Q to create and modify TODO lists using the `todo_list` tool and the user to view and manage existing TODO lists using `/todos`.

**Features:**
- Q will automatically make TODO lists when appropriate or when asked
- View, manage, and delete TODOs using `/todos`
- Resume existing TODO lists stored in `.amazonq/cli-todo-lists`

**Usage:**
```
/todos clear-finished       # Delete completed TODOs in your working directory
/todos resume               # Select and resume an existing TODO list
/todos view                 # Select and view and existing TODO list
/todos delete               # Select and delete an existing TODO list
```

**Settings:**
- `chat.enableTodoList` - Enable/disable TODO list functionality (boolean)

### Delegate

**Command:** `/delegate`

**Keyboard Shortcut:** `Ctrl+D` (customizable via `q settings chat.delegateModeKey x`)

**Description:** Launch and manage asynchronous task processes. Enables running Q chat sessions with specific agents in parallel to the main conversation.

**Usage:**
- `/delegate launch "Fix the bug in main.rs"` - Launch task with default agent
- `/delegate launch --agent coding "Fix the bug in main.rs"` - Launch with specific agent (shows approval dialog)
- `/delegate status` - Show summary of all tasks
- `/delegate status abc12345` - Show status of specific task
- `/delegate read abc12345` - Read output from completed task (triggers LLM analysis)
- `/delegate delete abc12345` - Delete task and its files
- `/delegate list` - List all tasks with timestamps and brief info

**Agent Approval Flow:**
When using `--agent`, you'll see an approval dialog:
```
Agent: coding
Description: Coding assistant for software development
Task: Fix the bug in main.rs
Tools: fs_read, fs_write, execute_bash

⚠️  This task will run with trust-all permissions and can execute commands or consume system/cloud resources. Continue? [y/N]:
```

**When enabled:** Use `/delegate` commands or `Ctrl+D` to spawn independent Q processes that work on tasks while you continue your main conversation. Tasks with agents require explicit approval and show agent details. Tasks without agents run with a warning about trust-all permissions. Once delegated, tasks work independently and you can check progress, read results, or delete them as needed.

## Managing Experiments

Use the `/experiment` command to toggle experimental features:

```
/experiment
```

This will show an interactive menu where you can:
- See current status of each experiment (ON/OFF)
- Toggle experiments by selecting them
- View descriptions of what each experiment does

## Important Notes

⚠️ **Experimental features may be changed or removed at any time**  
⚠️ **Experience might not be perfect**  
⚠️ **Use at your own discretion in production workflows**

These features are provided to gather feedback and test new capabilities. Please report any issues or feedback through the `/issue` command.

## Fuzzy Search Support

All experimental commands are available in the fuzzy search (Ctrl+S):
- `/experiment` - Manage experimental features
- `/knowledge` - Knowledge base commands (when enabled)
- `/todos` - User-controlled TODO list commands (when enabled)

## Settings Integration

Experiments are stored as settings and persist across sessions:
- `EnabledKnowledge` - Knowledge experiment state
- `EnabledThinking` - Thinking experiment state
- `EnabledTodoList` - TODO list experiment state

You can also manage these through the settings system if needed.
