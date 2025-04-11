# Amazon Q CLI skim Integration Plan

This document outlines the plan for integrating skim with Amazon Q CLI to provide a keyboard shortcut (Ctrl+K) for command selection and parameter input.

## Overview

The goal is to implement a Ctrl+K keybinding that opens a skim interface showing all available commands. When a command requiring parameters is selected (like `/context add`), another skim window will open for parameter selection (e.g., file selection).

## Implementation Plan

### 1. Enable Custom Bindings Feature

First, ensure rustyline is included with the "custom-bindings" feature in Cargo.toml:

```toml
[dependencies]
rustyline = { version = "15.0.0", features = ["custom-bindings"] }
skim = "0.10.4"
```

### 2. Create a Custom Event Handler

We'll create a custom event handler that:
1. Launches skim with our command list
2. Processes the selection
3. Inserts the selected command into the input line

### 3. Modify InputSource to Use Custom Bindings

Update the `InputSource` class in `./crates/q_cli/src/cli/chat/input_source.rs` to use custom bindings:

```rust
use rustyline::{Event, EventHandler, KeyEvent};
use rustyline::config::Configurer;
use rustyline::event::{Cmd, ConditionalEventHandler, EventContext};

// Custom event handler for skim command selection
struct SkimCommandSelector;

impl ConditionalEventHandler for SkimCommandSelector {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext<'_>,
    ) -> Option<Cmd> {
        // Launch skim command selector
        match skim_integration::select_command() {
            Ok(Some(command)) => {
                // Return a command to replace the current line with the selected command
                Some(Cmd::Replace(
                    Movement::WholeBuffer,
                    Some(command)
                ))
            },
            _ => {
                // If cancelled or error, do nothing
                Some(Cmd::Noop)
            }
        }
    }
}

impl InputSource {
    pub fn new() -> Result<Self> {
        let mut editor = rl()?;
        
        // Add custom keybinding for Ctrl+K to launch skim command selector
        editor.bind_sequence(
            KeyEvent::ctrl('k'),
            EventHandler::Conditional(Box::new(SkimCommandSelector))
        );
        
        Ok(Self(inner::Inner::Readline(editor)))
    }
}
```

### 4. Create skim Integration Module

Implement the skim integration module with functions to:
1. Collect available commands and their descriptions
2. Launch skim with the command list
3. Handle command selection and parameter input

```rust
// In skim_integration.rs
pub fn select_command() -> Result<Option<String>> {
    let commands = get_available_commands();
    let formatted_commands = format_commands_for_skim(&commands);
    
    match launch_skim_selector(&formatted_commands, "Select command: ", false)? {
        Some(selections) if !selections.is_empty() => {
            let selected_command = &selections[0];
            
            // Check if the command needs parameters
            if selected_command.starts_with("/context add") {
                // For context add, we need to select files
                match select_files_with_skim()? {
                    Some(files) if !files.is_empty() => {
                        // Construct the full command with selected files
                        let mut cmd = selected_command.clone();
                        for file in files {
                            cmd.push_str(&format!(" {}", file));
                        }
                        Ok(Some(cmd))
                    },
                    _ => Ok(Some(selected_command.clone())), // User cancelled file selection
                }
            } else {
                // Command doesn't need additional parameters
                Ok(Some(selected_command.clone()))
            }
        },
        _ => Ok(None), // User cancelled command selection
    }
}
```

### 5. Command Collection Strategy

We extract all available commands and their descriptions from the codebase:

1. Parse the `Command` enum in `./crates/q_cli/src/cli/chat/command.rs`
2. Extract subcommands from `ProfileSubcommand`, `ContextSubcommand`, and `ToolsSubcommand`
3. Include help text and usage information for each command

```rust
pub fn get_available_commands() -> Vec<CommandInfo> {
    vec![
        CommandInfo { command: "/help".to_string(), description: "Show the help dialogue".to_string() },
        CommandInfo { command: "/clear".to_string(), description: "Clear the conversation history".to_string() },
        // ... other commands
    ]
}
```

### 6. Terminal State Management

Proper handling of terminal state when switching between the main application and skim:

1. Use skim's native Rust API to handle terminal state
2. Leverage skim's built-in terminal handling capabilities
3. Handle window resizing and other terminal events

### 7. Error Handling

Robust error handling for cases where:

1. Selected command or parameters are invalid
2. skim encounters an error
3. File operations fail

## Implementation Status

✅ **Completed**: The implementation has been successfully completed using skim instead of fzf. The key advantages of this approach are:

1. **Native Rust Implementation**: skim is written in Rust, making it a more natural fit for the Amazon Q CLI
2. **No External Dependencies**: No need to check if fzf is installed on the system
3. **Better Integration**: Direct API calls instead of spawning external processes
4. **Similar User Experience**: Maintained the same functionality and UI experience
5. **Improved Performance**: Native implementation provides better performance

The implementation successfully passes cargo check and build, and the functionality works the same as originally planned but with skim instead of fzf.
