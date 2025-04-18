# Internal Command Next State Enhancement

## Overview

We've enhanced the `internal_command` tool to always return a valid `ChatState` via the `next_state` field in `InvokeOutput`. This ensures that commands executed through the tool properly control the chat flow after execution.

## Rationale

Commands executed through the `internal_command` tool often output information directly to the end-user terminal. In most cases, the LLM doesn't need to take any additional action immediately after command execution. By defaulting to returning a `PromptUser` state, we ensure that:

1. The chat flow continues smoothly after command execution
2. The user sees the command output directly in their terminal
3. The LLM doesn't attempt to interpret or repeat the command output
4. Commands can still override this behavior when needed (e.g., the `Exit` command)

## Implementation Details

1. We're using the existing `next_state` field in `InvokeOutput` to pass through the `ChatState` returned by command execution
2. All command execution paths now return a valid `ChatState`, defaulting to `PromptUser` when no specific state is provided
3. This ensures consistent behavior whether commands are executed directly or through the `internal_command` tool
4. Removed the redundant `SHOULD_EXIT` flag and related functions, as the `Exit` state is now properly passed through the `next_state` field

## Code Changes

1. Updated the `invoke` method in `internal_command/tool.rs` to:
   - Pass through any `ChatState` returned by command execution via the `next_state` field
   - Default to returning a `PromptUser` state for error cases and commands that don't specify a state
2. Removed the `SHOULD_EXIT` static flag and related functions (`should_exit()` and `reset_exit_flag()`)
3. Simplified the `Exit` state handling to rely solely on the `next_state` field

## Testing

This change has been tested with various commands to ensure:
1. Commands that return specific states (like `Exit`) properly control the chat flow
2. Commands that don't specify a state default to `PromptUser`
3. Error cases properly return to the prompt

## Future Considerations

As we continue migrating commands to the new registry system, we should ensure that all command handlers return appropriate `ChatState` values based on their behavior and intended user experience.