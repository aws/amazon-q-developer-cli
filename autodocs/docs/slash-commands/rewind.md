---
doc_meta:
  validated: 2026-05-13
  commit: 32e2af204
  status: validated
  testable_headless: false
  category: slash_command
  title: /rewind
  description: Fork conversation at an earlier turn into a new session
  keywords: [rewind, fork, branch, undo, history, turn, session]
  related: [chat-load, checkpoint, session-management]
---

# /rewind

Fork conversation at an earlier turn into a new session.

## Overview

The `/rewind` command lets you go back to an earlier point in your conversation and continue from there in a new session. The original session remains untouched—rewind creates a fork, not an undo. This is useful when you want to explore a different approach without losing your current work.

## Usage

### Interactive Mode (Recommended)

```
/rewind
```

Opens a picker showing all your prompts in the current session (newest first). Select one to fork from that point.

### Direct Mode

```
/rewind <turn_index>
```

Fork directly to a specific turn by its log index (shown in the picker).

## How It Works

1. **Pick a turn**: Select which prompt you want to rewind to
2. **Fork created**: A new session is created containing all history up to and including that turn's response
3. **Auto-switch**: The TUI automatically loads the new forked session
4. **Original preserved**: Your original session stays exactly as it was

The forked session includes:
- All prompts and responses up to the selected turn
- Session state (model, agent, permissions)
- Context files that were active

The forked session does NOT include:
- Prompts and responses after the selected turn
- Any compaction that occurred after the selected turn

## Examples

### Example 1: Interactive Rewind

```
/rewind
```

**Output** (picker):
```
┌─ Rewind to turn ─────────────────────────────────────────┐
│ [12%] Fix the bug in auth.ts                             │
│ [ 8%] Add error handling to the API                      │
│ [ 5%] Create the user service                            │
│ [ 2%] Set up the project structure                       │
└──────────────────────────────────────────────────────────┘
```

Select a turn to see a preview of the assistant's response, then press Enter to fork.

### Example 2: Direct Rewind

```
/rewind 0
```

Forks to the very first prompt in the session.

**Output**:
```
Rewound to earlier turn (new session abc123-def456)
```

### Example 3: Exploring Alternative Approaches

You asked the assistant to implement a feature using approach A, but now want to try approach B:

1. Run `/rewind`
2. Select the turn where you originally asked for the feature
3. In the new forked session, ask for approach B instead
4. Compare results between the two sessions

## Picker Details

The picker shows:
- **Label**: First line of your prompt (truncated to 80 chars)
- **Group**: Context usage percentage at that turn (e.g., "12%") or token count (e.g., "1.2k")
- **Preview**: First few lines of the assistant's response (shown on hover/selection)

Turns are listed newest-first so recent prompts appear at the top.

## Troubleshooting

### Issue: "Session has no turns to rewind to"

**Symptom**: Error when running `/rewind`  
**Cause**: Current session has no prompts yet  
**Solution**: Send at least one prompt before rewinding

### Issue: "Turn index X is out of range"

**Symptom**: Error when using direct mode  
**Cause**: The specified index doesn't exist  
**Solution**: Use `/rewind` without arguments to see valid turns

### Issue: "Entry at index X is not a user prompt"

**Symptom**: Error when using direct mode  
**Cause**: The index points to a non-prompt log entry  
**Solution**: Use the picker to select valid prompt turns

### Issue: Forked session missing context

**Symptom**: Assistant doesn't remember earlier context  
**Cause**: Context was added after the rewind point  
**Solution**: Re-add context files in the forked session

## Related Features

- [Session Management](../features/session-management.md) - Managing multiple sessions
- [/chat load](chat-load.md) - Load a saved session
- [/checkpoint](checkpoint.md) - Workspace file checkpoints (different from conversation rewind)

## Limitations

- Cannot rewind to a point before the session started (use `/chat load` for that)
- Forked session gets a new session ID (not a branch of the original)
- Original session cannot be modified after forking
- Rewinding past a compaction restores pre-compaction history in the fork

## Technical Details

**Session Creation**: Forked sessions have `parent_session_id` set to the original session and `session_created_reason` set to `Rewind`.

**Log Entries**: All log entries from index 0 through the end of the selected turn are copied verbatim to the new session.

**State Handling**: Session state (model, agent, permissions) is cloned. The `conversation_id` is updated to the new session ID. Turn metadata is filtered to only include entries for copied prompts.

**Compaction**: If the original session was compacted, rewinding to a turn before the compaction restores the full pre-compaction history because the `Compaction` log entry is not copied to the fork.
