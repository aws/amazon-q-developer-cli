# Task Management Tools

Built-in tools for tracking multi-step work within an ACP session. The agent uses these to break complex requests
into subtasks with dependency management and progress tracking.

## Tools

| Tool | Purpose |
|------|---------|
| `task_create` | Create a new task with subject, description, and optional metadata |
| `task_update` | Update status, details, owner, dependencies, or metadata |
| `task_get` | Retrieve full details of a task by ID |
| `task_list` | List all active tasks with summary info |

## Storage

Tasks are stored as individual JSON files co-located with ACP session data:

```
~/.kiro/sessions/cli/{session_id}/tasks/{id}.json
```

This ensures tasks naturally follow the session lifecycle — when a session is resumed via `LoadSessionRequest`,
the `TaskStore` scans existing files to recover the ID counter.

### Task File Format

```json
{
  "id": "1",
  "subject": "Fix auth bug",
  "description": "Fix the login flow authentication issue",
  "status": "pending",
  "owner": "subagent-1",
  "depends_on": [],
  "metadata": { "priority": "high" }
}
```

### ID Allocation

The next task ID is tracked in memory via `AtomicU64`. On session resume, `TaskStore::new()` scans the tasks
directory for the highest existing file name (e.g., `5.json` → next ID is 6). No separate counter file on disk.

## Task Lifecycle

```
 ┌─────────┐      ┌─────────────┐      ┌───────────┐
 │ pending  │ ───► │ in_progress │ ───► │ completed │
 └─────────┘      └─────────────┘      └───────────┘
      │                   │                    │
      │                   │                    │ all tasks completed
      └───────┬───────────┘                    │ + new task_create
              ▼                                ▼
    (explicit delete)                  (auto cleanup)
```

### Deletion

1. **Explicit**: `task_update` with `status: "deleted"` removes the JSON file from disk immediately.
2. **Automatic cleanup**: When `task_create` is called and every existing task is `completed`, all task files are
   deleted before the new task is created. If any task is still `pending` or `in_progress`, nothing is cleaned up.

Completed tasks remain visible in context injection until the agent starts a new batch of work.

### Dependencies

Dependencies are declared via `add_depends_on` in `task_update`:

- `task_update` with `add_depends_on: ["1"]` on task #2 means #2 cannot start until #1 completes
- `task_list` filters `depends_on` at read-time: completed/deleted dependencies are excluded
- Setting a task to `in_progress` with unresolved dependencies succeeds but returns a `warning` field — this is
  advisory, the update still goes through

## Context Injection

When tasks exist, the agent injects a formatted task summary into every conversation request as a user+assistant
message pair at the front of the history:

```
--- CONTEXT ENTRY BEGIN ---
Active Task List (1/3 completed):
✓ #1: Fix auth bug
■ #2: Write tests
□ #3: Deploy service — depends on #2
--- CONTEXT ENTRY END ---
```

Icons: `✓` completed, `■` in_progress, `□` pending.

## Changes from V1 Todo List

V2 task tools replace the V1 `todo_list` tool with a redesigned system.

### Why Redesign

The V1 `todo_list` was a single tool with 6 subcommands (`create`, `complete`, `add`, `remove`, `load`, `lookup`)
operating on a flat checklist. It worked but had limitations:

1. **No dependencies** — No way to express "task B can't start until task A finishes."
2. **Index-based addressing** — Tasks identified by array index (`completed_indices: [0, 2]`), which shifted when
   tasks were added or removed mid-plan. V2 uses stable numeric IDs that never change.
3. **No status workflow** — Binary (done/not done). No `in_progress` state for tracking active work.
4. **No ownership** — No way to assign tasks to subagents in multi-agent scenarios.
5. **Monolithic tool** — One tool with a `command` discriminator made the schema complex and the LLM more likely
   to produce malformed calls.

### Subcommand Mapping

| V1 subcommand | What it does | V2 equivalent |
|---|---|---|
| `create` | Create a new list with batch of tasks + description | `task_create` (called once per task) |
| `complete` | Mark tasks done by array index, attach context + modified files | `task_update` with `status: "completed"` |
| `add` | Insert new tasks at specific array indices | `task_create` (just create more tasks) |
| `remove` | Remove tasks by array index | `task_update` with `status: "deleted"` |
| `load` | Switch to a different todo list by ID | Dropped — one task set per session |
| `lookup` | List all existing todo list IDs | Dropped — same reason |

Notable simplifications:
- V1's `create` made a batch of tasks via `tasks: Vec<TaskInput>`. V2 creates one at a time — simpler schema.
- V1's `complete` had `context_update` and `modified_files` tracking fields. V2 dropped these — the conversation
  history already captures that.
- V1's `add` needed `insert_indices` to specify array position. V2 doesn't need this since tasks aren't ordered.
- V1's `load`/`lookup` managed multiple named lists. V2 has one flat set per session, cleaned up automatically.

### What Stayed the Same

- **Context injection** — Both inject task state into every conversation request. This ensures the LLM always has
  fresh task state.
- **Auto-allowed** — Both require no user approval (`permissions.rs` returns `Allow` unconditionally).
- **Behavioral guidance in descriptions** — Both embed "when to use / when not to use" policy in tool descriptions.

### Token Cost

Tool specs (descriptions + schemas) are sent with every LLM request:

| | Chars | ~Tokens |
|---|---|---|
| V1 `todo_list` (1 tool) | 3,723 | ~1,200 |
| V2 task tools (4 tools) | 7,912 | ~2,150 |
| Delta | +4,189 | ~+950 |

The increase comes from richer behavioral guidance in descriptions (467 → 5,833 chars). The schemas actually
shrank (3,256 → 2,079 chars) by splitting one complex schema with 6 subcommands into 4 focused schemas.

Context injection adds ~8 tokens per task per request (same in both versions).

## Future Improvements

- **Tool group abstraction**: Expose `"task"` as a single user-facing tool name in agent configs, docs, and TUI.
  Users should not need to know about `task_create`, `task_update`, etc. — those are LLM implementation details.
  `allowed_tools: ["task"]` should expand to all 4 tools internally. The TUI should render all task tool calls
  under a unified "Task" label.
