# Experimental Features

KIRO CLI includes experimental features that can be toggled on/off using the `/experiment` command. These features are in active development and may change or be removed at any time.

## Available Experiments

### Checkpointing
**Description:** Enables session-scoped checkpoints for tracking file changes using Git CLI commands

**Features:**
- Snapshots file changes into a shadow bare git repo
- List, expand, diff, and restore to any checkpoint
- Conversation history unwinds when restoring checkpoints
- Auto-enables in git repositories (ephemeral, cleaned on session end)
- Manual initialization available for non-git directories

**Usage:**
```
/checkpoint init                    # Manually enable checkpoints (if not in git repo)
/checkpoint list [--limit N]       # Show turn-level checkpoints with file stats
/checkpoint expand <tag>            # Show tool-level checkpoints under a turn
/checkpoint diff <tag1> [tag2|HEAD] # Compare checkpoints or with current state
/checkpoint restore [<tag>] [--hard] # Restore to checkpoint (interactive picker if no tag)
/checkpoint clean                   # Delete session shadow repo
```

**Restore Options:**
- Default: Revert tracked changes & deletions; keep files created after checkpoint
- `--hard`: Make workspace exactly match checkpoint; deletes tracked files created after it

**Example:**
```
/checkpoint list
[0] 2025-09-18 14:00:00 - Initial checkpoint
[1] 2025-09-18 14:05:31 - add two_sum.py (+1 file)
[2] 2025-09-18 14:07:10 - add tests (modified 1)

/checkpoint expand 2
[2] 2025-09-18 14:07:10 - add tests
 └─ [2.1] fs_write: Add minimal test cases to two_sum.py (modified 1)
```

### Context Usage Percentage
**Description:** Shows context window usage as a percentage in the chat prompt

**Features:**
- Displays percentage of context window used in prompt (e.g., "[rust-agent] 6% >")
- Color-coded indicators:
  - Green: <50% usage
  - Yellow: 50-89% usage  
  - Red: 90-100% usage
- Helps monitor context window consumption
- Disabled by default

**When enabled:** The chat prompt will show your current context usage percentage with color coding to help you understand how much of the available context window is being used.

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
- Resume existing TODO lists stored in `.kiro/cli-todo-lists`

**Usage:**
```
/todos clear-finished       # Delete completed TODOs in your working directory
/todos resume               # Select and resume an existing TODO list
/todos view                 # Select and view and existing TODO list
/todos delete               # Select and delete an existing TODO list
```

**Settings:**
- `chat.enableTodoList` - Enable/disable TODO list functionality (boolean)

### Use Subagent
**Tool name**: `use_subagent`
**Description:** Enables delegating complex tasks to specialized subagents that run in parallel with isolated context

**Features:**
- Spawn up to 4 subagents simultaneously for parallel task execution
- Each subagent operates with its own isolated context to prevent main conversation bloat
- Real-time visual indicator showing status of all running subagents
- Support for different agent configurations per subagent
- Interactive controls for monitoring and managing subagents
- Automatic execution summary with tool usage and duration metrics

**How it works:**
When enabled, the main agent can delegate tasks to subagents using the `use_subagent` tool. Each subagent:
1. Receives a specific query/task and optional context
2. Runs independently with its own agent configuration
3. Uses the `summary` tool to report findings back to the main agent
4. Operates in isolation to keep the main conversation focused

**Visual Indicator:**
The subagent indicator displays:
- Real-time status for each subagent (starting up, thinking, calling tools, summarizing)
- Animated spinner showing active subagents
- Current activity and progress messages
- Tool approval requests (press 'y' to approve, 'n' to deny)
- MCP server loading status and OAuth requirements
- Execution summary upon completion (tool uses, duration)

**Controls:**
```
j/↓         Navigate down through subagents
k/↑         Navigate up through subagents
y           Approve tool use for selected subagent
n           Deny tool use for selected subagent
Enter       Copy OAuth URL to clipboard (when applicable)
Ctrl+C      Interrupt all subagents
Esc         Deselect current subagent
```

**Use cases:**
- Breaking down complex multi-step tasks into parallel subtasks
- Preventing context window bloat in long conversations
- Running independent research or analysis tasks simultaneously
- Delegating specialized tasks to different agent configurations
- Maintaining focus in the main conversation while handling auxiliary tasks

**Example workflow:**
```
User: "Research the top 3 JavaScript frameworks and compare their performance"

Main agent uses subagent tool to spawn 3 subagents:
- Subagent 1: Research React performance metrics
- Subagent 2: Research Vue.js performance metrics
- Subagent 3: Research Angular performance metrics

Each subagent:
- Conducts independent research
- Gathers relevant data
- Calls summary tool with findings

Main agent receives all summaries and synthesizes comparison
```

**Settings:**
- `EnabledUseSubagent` - Enable/disable subagent functionality (boolean)

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
- `EnabledCheckpointing` - Checkpointing experiment state
- `EnabledContextUsagePercentage` - Context usage percentage experiment state
- `EnabledKnowledge` - Knowledge experiment state
- `EnabledThinking` - Thinking experiment state
- `EnabledTodoList` - TODO list experiment state

You can also manage these through the settings system if needed.
