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

## Settings Integration

Experiments are stored as settings and persist across sessions:
- `EnabledKnowledge` - Knowledge experiment state
- `EnabledThinking` - Thinking experiment state

You can also manage these through the settings system if needed.
