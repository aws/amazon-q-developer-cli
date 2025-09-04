# TODO Management

The `/todos` command provides persistent TODO list management for Amazon Q CLI, allowing you to view, resume, and manage TODO lists created during chat sessions.

## Getting Started

TODO lists are automatically created when Q breaks down complex tasks. You can then manage these lists using the todos command:

`/todos view`
`/todos resume`

## Commands

#### `/todos view`

Display and select a TODO list to view its contents, showing task descriptions and completion status.

Interactive selection shows:
- ✓ Completed lists (green checkmark)
- ✗ In-progress lists with completion count (red X with progress)

#### `/todos resume`

Show an interactive menu of available TODO lists with their current progress status. Selecting a todo list will load the list back into your chat session, allowing Q to continue where it left off.

#### `/clear-finished`

Remove all completed TODO lists from storage. This helps clean up your workspace by removing lists where all tasks have been completed.

#### `/todos delete [--all]`

Delete specific TODO lists or all lists at once.

`q chat todos delete` # Interactive selection to delete one list
`q chat todos delete --all` # Delete all TODO lists

**Options:**
- `--all` - Delete all TODO lists without interactive selection

## Storage

TODO lists are stored locally in `.amazonq/cli-todo-lists/` directory within your current working directory. Each list is saved as a JSON file with:

- Unique timestamp-based ID
- Task descriptions and completion status  
- Context updates from completed tasks
- Modified file paths
- Overall list description

#### Interactive Selection

All commands use interactive selection allowing you to:
- Navigate with arrow keys
- Press Enter to select
- Press Esc to cancel

## Best Practices

#### Managing Lists

- Use `clear-finished` regularly to remove completed lists
- Resume lists to continue complex multi-step tasks
- View lists to check progress without resuming

#### Workflow Integration

- Let Q create TODO lists for complex tasks automatically
- Use `resume` to pick up where you left off in previous sessions
- Check `view` to see what tasks remain before resuming work

#### TODO List Storage

- Lists are stored in current working directory only
- No automatic cleanup of old lists
- No cross-directory list sharing

## Troubleshooting

#### No Lists Available

If commands show "No to-do lists available":

1. **Check directory**: Ensure you're in the directory where lists were created
2. **Verify storage**: Look for `.amazonq/cli-todo-lists/` directory
3. **Create lists**: Use chat sessions to create new TODO lists

#### Lists Not Loading

If lists exist but won't load:

1. **Check permissions**: Ensure read access to `.amazonq/cli-todo-lists/`
2. **Verify format**: Lists should be valid JSON files
3. **Check file integrity**: Corrupted files may prevent loading

## `todo_list` vs. `/todos`
The `todo_list` tool is specifically for the model to call. The model is allowed to create TODO lists, mark tasks as complete, add/remove
tasks, load TODO lists with a given ID (which are automatically provided when resuming TODO lists), and search for existing TODO lists.

The `/todos` command is for the user to manage existing TODO lists created by the model. The user can view, resume, and delete TODO lists
by using the appropriate subcommand and selecting the TODO list to perform the action on.

## Examples
#### Asking Q to make a TODO list:
```
> Make a todo list with 3 read-only tasks.

> I'll create a todo list with 3 read-only tasks for you.

🛠️  Using tool: todo_list (trusted)
 ⋮ 
 ● TODO:
[ ] Review project documentation
[ ] Check system status
[ ] Read latest updates
 ⋮ 
 ● Completed in 0.4s
```

#### Selecting a TODO list to view:
```
> /todos view

? Select a to-do list to view: ›
❯ ✗ Unfinished todo list (0/3)
  ✔ Completed todo list (3/3)
```

#### Resuming a TODO list (after selecting):
```
> /todos resume

⟳ Resuming: Read-only tasks for information gathering

🛠️  Using tool: todo_list (trusted)
 ⋮ 
 ● TODO:
[x] Review project documentation
[ ] Check system status
[ ] Read latest updates
 ⋮ 
 ● Completed in 0.1s
 ```




