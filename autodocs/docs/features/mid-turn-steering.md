---
doc_meta:
  title: Mid-Turn Steering
  description: Send messages to the agent while it's working to redirect or guide its approach
  category: feature
  keywords: [steering, redirect, interrupt, queue, mid-turn, guidance, follow-up, queuing, steer mode, queue mode]
  related: [session-management, tangent-mode, key-bindings-settings]
  validated: 2026-06-15
  commit: d6d422fde
  status: validated
  testable_headless: false
---

# Mid-Turn Steering

Send messages to the agent while it's working to redirect or guide its approach without cancelling the current task.

## Overview

Mid-turn steering lets you type a follow-up message while the agent is actively processing. Instead of waiting for the agent to finish or cancelling and starting over, you can send guidance that the agent incorporates into its work.

Kiro offers two follow-up modes that control when your message reaches the agent:

| Mode | Behavior | Best For |
|------|----------|----------|
| **Steering** (default) | Message injected mid-turn at next tool boundary | Real-time course correction |
| **Queuing** | Message buffered locally, sent after turn ends | Batching multiple interrupts |

Press `Ctrl+S` to toggle between modes at any time.

## How It Works

### Steering Mode

1. **Type while busy**: When the agent is processing, type your message and press Enter
2. **Message sent to backend**: Your message is queued on the backend
3. **Injection point**: At the next tool boundary, your message is injected into the conversation
4. **Agent adjusts**: The agent sees your guidance wrapped in a `[LIVE STEERING]` block and adjusts its approach
5. **Acknowledgment**: The agent includes a `[STEERING steer-xxx: ...]` note explaining how it handled your input

### Queuing Mode

1. **Type while busy**: When the agent is processing, type your message and press Enter
2. **Message buffered locally**: Your message is added to a local queue (visible in activity tray)
3. **Turn completes**: The agent finishes its current turn normally
4. **Queue drains**: The first queued message is sent as a new prompt
5. **Repeat**: Process continues until queue is empty

### Injection Points (Steering Mode)

Your steering message is injected at:
- **Tool boundaries**: After a tool completes but before the next tool starts
- **Turn end**: If the agent finishes its turn before reaching a tool boundary

Multiple messages sent in steering mode are concatenated and delivered together.

## Usage

### Sending a Follow-Up Message

Type and press Enter while the agent is working:

```
> Refactor the auth module

[Agent starts working, reading files...]

> Actually, focus on the login function first

[In steering mode: message queued for mid-turn injection]
[In queuing mode: message added to local buffer]
```

### Toggling Modes

Press `Ctrl+S` to switch between steering and queuing modes:

```
[Steering mode active - messages inject mid-turn]

Ctrl+S

Switched to Queue mode

[Queuing mode active - messages buffer until turn ends]
```

When you toggle modes, any pending messages transfer to the new mode:
- **Steering → Queuing**: Backend queue content moves to local buffer
- **Queuing → Steering**: Local buffer content is sent to backend queue

### Viewing Queued Messages

The activity tray shows your pending messages:
- Press `Ctrl+X` to expand the activity tray
- The tab shows "Steer" (steering mode) or "Queue (N)" (queuing mode with N items)
- Press `Delete` or `Backspace` to clear the message/selected item

### Cancel + Redirect

If you press `Esc` while a steering message is queued:
1. The current operation is cancelled
2. Your queued message is automatically sent as a new prompt
3. This implements "stop that and do this instead" behavior

## Examples

### Example 1: Steering Mode - Redirect Approach

```
> Add error handling to the API endpoints

[Agent starts adding try/catch blocks...]

> Use a centralized error handler middleware instead

[Message injected at next tool boundary]
[Agent adjusts to implement middleware pattern]
```

### Example 2: Steering Mode - Add Clarification

```
> Update the database schema

[Agent starts modifying schema...]

> Make sure to add indexes for the email column

[Agent includes index in the migration]
```

### Example 3: Queuing Mode - Batch interrupts

```
[Ctrl+S to switch to queuing mode]

> Implement the search feature

[Agent working...]

> Use fuzzy matching
> Also add pagination

[Queue shows: 2 items]
[Agent finishes turn]
[First queued message sent: "Use fuzzy matching"]
[Agent processes, finishes]
[Second queued message sent: "Also add pagination"]
```

### Example 4: Cancel and Redirect

```
> Write unit tests for the user service

[Agent starts writing tests...]

> [types] Actually, let's do integration tests
> [presses Esc]

Redirecting to: Actually, let's do integration tests

[New turn starts with integration test focus]
```

## Configuration

### Default Follow-Up Mode

Set the startup default via `/settings` → `terminal` → `interrupt behaviour`:

```
/settings

> terminal
> interrupt behaviour
> steer ●     Inject interrupts mid-turn at tool boundaries
> queue       Buffer interrupts and send after turn ends
```

Or via CLI:
```bash
kiro-cli settings set chat.defaultInterruptBehavior steer
kiro-cli settings set chat.defaultInterruptBehavior queue
```

### Toggle Keybinding

The default toggle keybinding is `Ctrl+S`. Customize it via:

```bash
kiro-cli settings set chat.keybindings.toggleInterruptBehavior ctrl+s
```

View current binding in `/settings` → "keybindings".

## Activity Tray Integration

The activity tray displays follow-up state based on the active mode:

### Steering Mode

| State | Display |
|-------|---------|
| Message queued | Shows "Steer" tab with message text |
| Message injected | Queue clears, user bubble appears in chat |
| Message cleared | Tab shows no content |

### Queuing Mode

| State | Display |
|-------|---------|
| Messages queued | Shows "Queue (N)" tab with message list |
| Turn ends | First message sent, counter decrements |
| Queue empty | Tab shows no content |

Keyboard shortcuts in expanded tray:
- `Tab` - Switch between Tasks and Queue tabs
- `Delete`/`Backspace` - Clear message (steering) or remove selected item (queuing)
- `↑`/`↓` - Navigate queue items (queuing mode only)

## Limitations

- **Steering mode**: Only one message slot (multiple messages concatenate with `\n\n`)
- **Queuing mode**: Messages sent sequentially, not batched
- **No cross-mode editing**: In steering mode, you can only clear the pending message
- **Timing dependent**: If the agent finishes before your message is queued, it becomes a normal prompt

## Technical Details

### Message Format

Steering messages are wrapped in a structured format when injected:

```
[LIVE STEERING - New message from user]

The user sent a new message while you are working. As the currently active agent,
adjust your approach if necessary based on this guidance.

<user_message id="steer-abc123">
Your message here
</user_message>

IMPORTANT: After completing your work, include a brief note about how you handled
this steering message. Use this exact format:

[STEERING steer-abc123: <describe what you did or why it wasn't applicable>]
```

### Events

The steering system emits these events:
- `SteeringQueued` - Message added to backend queue
- `SteeringConsumed` - Message injected into conversation
- `SteeringCleared` - Queue cleared without injection

### Mode Toggle Behavior

When toggling from steering to queuing:
1. Backend queue content is fetched
2. Content is split on `\n\n` to restore individual messages
3. Messages are added to local queue
4. Backend queue is cleared

When toggling from queuing to steering:
1. Local queue is concatenated with `\n\n` separators
2. Combined content is sent to backend queue
3. Local queue is cleared

## Troubleshooting

### Message Not Delivered

**Symptom**: Typed a message but agent didn't acknowledge it  
**Cause**: Agent finished before injection point  
**Solution**: Message was sent as a normal follow-up prompt instead

### Queue Shows Empty But Message Was Typed

**Symptom**: Activity tray shows no queued message  
**Cause**: Message was already injected or cleared  
**Solution**: Check chat history for your message as a user bubble

### Agent Ignored Steering

**Symptom**: Agent acknowledged steering but didn't change approach  
**Cause**: Steering arrived too late or wasn't applicable  
**Solution**: Cancel and send as a new prompt for stronger redirection

### Cancel Didn't Redirect

**Symptom**: Pressed Esc but queued message wasn't sent  
**Cause**: No message was queued at cancel time  
**Solution**: Queue a message first, then cancel

### Mode Toggle Failed

**Symptom**: "Could not switch — steer failed" error  
**Cause**: Backend rejected the steer request  
**Solution**: Remain in current mode, messages are preserved. Try again or clear queue.

### Ctrl+S Not Working

**Symptom**: Toggle keybinding doesn't respond  
**Cause**: Keybinding conflict or custom binding  
**Solution**: Check `/settings` → "keybindings" for current binding

## Related

- [Session Management](session-management.md) - Session lifecycle and state
- [Tangent Mode](tangent-mode.md) - Alternative for side explorations
- [Key Bindings Settings](../settings/key-bindings-settings.md) - Customize toggle keybinding
