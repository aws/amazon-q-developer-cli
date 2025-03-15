# Slash Command Implementation Guide

This document explains how slash commands are implemented in the Amazon Q Developer CLI chat interface and provides guidance on how to implement new slash commands.

## Overview

Slash commands in the Amazon Q chat interface provide special functionality that can be triggered by typing a command that starts with a forward slash (`/`). Examples of existing slash commands include `/help`, `/clear`, `/acceptall`, and `/quit`.

## Implementation Architecture

The slash command system is primarily implemented in the following files:

1. `src/cli/chat/command.rs` - Defines the `Command` enum and parsing logic
2. `src/cli/chat/mod.rs` - Contains the main chat loop that processes commands
3. `src/cli/chat/prompt.rs` - Implements command completion for slash commands

## Command Parsing

Slash commands are parsed in the `Command::parse` method in `command.rs`. Here's how it works:

```rust
pub fn parse(input: &str) -> Result<Self, String> {
    let input = input.trim();

    if let Some(command) = input.strip_prefix("/") {
        return Ok(match command.to_lowercase().as_str() {
            "clear" => Self::Clear,
            "help" => Self::Help,
            "acceptall" => Self::AcceptAll,
            "q" | "exit" | "quit" => Self::Quit,
            _ => return Err(format!("Unknown command: {}", input)),
        });
    }

    // Other command types (! prefix or regular prompts)
    // ...
}
```

The `Command` enum defines all possible command types:

```rust
pub enum Command {
    Ask { prompt: String },
    Execute { command: String },
    Clear,
    Help,
    AcceptAll,
    Quit,
}
```

## Command Execution

Commands are executed in the main chat loop in `src/cli/chat/mod.rs`. The relevant code looks something like this:

```rust
match Command::parse(&input) {
    Ok(Command::Ask { prompt }) => {
        // Handle regular chat prompt
    },
    Ok(Command::Clear) => {
        // Clear the terminal
        execute!(stdout(), terminal::Clear(terminal::ClearType::All))?;
        execute!(stdout(), cursor::MoveTo(0, 0))?;
        println!("{}", WELCOME_TEXT);
    },
    Ok(Command::Help) => {
        // Display help text
        println!("{}", HELP_TEXT);
    },
    Ok(Command::AcceptAll) => {
        // Toggle accept all mode
        accept_all = !accept_all;
        println!("Auto-accept is now {}", if accept_all { "ON" } else { "OFF" });
    },
    Ok(Command::Quit) => {
        // Exit the application
        break;
    },
    Ok(Command::Execute { command }) => {
        // Execute shell command
    },
    Err(err) => {
        // Handle parsing error
        eprintln!("{}", err);
    }
}
```

## Command Completion

Command completion is implemented in `src/cli/chat/prompt.rs`. The `ChatCompleter` struct provides completion for slash commands:

```rust
const COMMANDS: &[&str] = &["/clear", "/help"];

impl Completer for ChatCompleter {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Self::Candidate>), ReadlineError> {
        let (start, word) = extract_word(line, pos, None, |c| c.is_space());
        Ok((
            start,
            if word.starts_with('/') {
                COMMANDS
                    .iter()
                    .filter(|p| p.starts_with(word))
                    .map(|s| (*s).to_owned())
                    .collect()
            } else {
                Vec::new()
            },
        ))
    }
}
```

## Step-by-Step Guide to Implementing a New Slash Command

To implement a new slash command, follow these steps:

### 1. Update the Command Enum

Add a new variant to the `Command` enum in `src/cli/chat/command.rs`:

```rust
pub enum Command {
    Ask { prompt: String },
    Execute { command: String },
    Clear,
    Help,
    AcceptAll,
    Quit,
    NewCommand, // Add your new command here
}
```

### 2. Update the Command Parser

Add your new command to the parsing logic in `Command::parse`:

```rust
pub fn parse(input: &str) -> Result<Self, String> {
    let input = input.trim();

    if let Some(command) = input.strip_prefix("/") {
        return Ok(match command.to_lowercase().as_str() {
            "clear" => Self::Clear,
            "help" => Self::Help,
            "acceptall" => Self::AcceptAll,
            "q" | "exit" | "quit" => Self::Quit,
            "newcommand" => Self::NewCommand, // Add your new command here
            _ => return Err(format!("Unknown command: {}", input)),
        });
    }

    // Rest of the parsing logic...
}
```

### 3. Update Command Completion

Add your new command to the `COMMANDS` array in `src/cli/chat/prompt.rs`:

```rust
const COMMANDS: &[&str] = &["/clear", "/help", "/newcommand"]; // Add your new command here
```

### 4. Implement Command Execution

Add a case for your new command in the main chat loop in `src/cli/chat/mod.rs`:

```rust
match Command::parse(&input) {
    // Existing commands...
    Ok(Command::NewCommand) => {
        // Implement your command's functionality here
        println!("Executing new command!");
        // Your command's logic goes here
    },
    // Rest of the match statement...
}
```

### 5. Update Help Text

If your command should be included in the help text, update the `HELP_TEXT` constant in `src/cli/chat/mod.rs`:

```rust
const HELP_TEXT: &str = color_print::cstr! {"
<cyan!>Available Commands:</cyan!>
<em>/help</em>         <black!>Show this help dialogue</black!>
<em>/clear</em>        <black!>Clear the terminal</black!>
<em>/acceptall</em>    <black!>Toggle acceptance prompting for the session</black!>
<em>/quit</em>         <black!>Quit the application</black!>
<em>/newcommand</em>   <black!>Description of your new command</black!>
"};
```

## Example: Implementing a `/version` Command

Let's walk through implementing a `/version` command that displays the current version of the CLI:

### 1. Update the Command Enum

```rust
pub enum Command {
    Ask { prompt: String },
    Execute { command: String },
    Clear,
    Help,
    AcceptAll,
    Quit,
    Version, // Add Version command
}
```

### 2. Update the Command Parser

```rust
pub fn parse(input: &str) -> Result<Self, String> {
    let input = input.trim();

    if let Some(command) = input.strip_prefix("/") {
        return Ok(match command.to_lowercase().as_str() {
            "clear" => Self::Clear,
            "help" => Self::Help,
            "acceptall" => Self::AcceptAll,
            "q" | "exit" | "quit" => Self::Quit,
            "version" => Self::Version, // Add Version command
            _ => return Err(format!("Unknown command: {}", input)),
        });
    }

    // Rest of the parsing logic...
}
```

### 3. Update Command Completion

```rust
const COMMANDS: &[&str] = &["/clear", "/help", "/version"]; // Add version command
```

### 4. Implement Command Execution

```rust
match Command::parse(&input) {
    // Existing commands...
    Ok(Command::Version) => {
        // Get version from Cargo.toml or environment
        let version = env!("CARGO_PKG_VERSION");
        println!("Amazon Q Developer CLI version: {}", version);
    },
    // Rest of the match statement...
}
```

### 5. Update Help Text

```rust
const HELP_TEXT: &str = color_print::cstr! {"
<cyan!>Available Commands:</cyan!>
<em>/help</em>         <black!>Show this help dialogue</black!>
<em>/clear</em>        <black!>Clear the terminal</black!>
<em>/acceptall</em>    <black!>Toggle acceptance prompting for the session</black!>
<em>/quit</em>         <black!>Quit the application</black!>
<em>/version</em>      <black!>Display the current version of the CLI</black!>
"};
```

## Best Practices for Implementing Slash Commands

1. **Keep it Simple**: Slash commands should be simple and focused on a single task.
2. **Use Consistent Naming**: Follow the existing naming pattern (lowercase, no spaces).
3. **Provide Feedback**: Always provide clear feedback when a command is executed.
4. **Handle Errors Gracefully**: If your command can fail, handle errors appropriately.
5. **Update Documentation**: Make sure to update the help text and any other documentation.
6. **Add Tests**: Add tests for your new command to ensure it works as expected.

## Testing Slash Commands

You can test your slash command implementation by:

1. Adding unit tests for the command parsing logic
2. Adding integration tests that verify the command's functionality
3. Manually testing the command in the CLI

Here's an example of a unit test for the command parsing:

```rust
#[test]
fn test_parse_version_command() {
    let command = Command::parse("/version").unwrap();
    assert!(matches!(command, Command::Version));
}
```

## Conclusion

Implementing a new slash command in the Amazon Q Developer CLI is straightforward. By following the steps outlined in this guide, you can add new functionality to the chat interface that users can easily access with a simple slash command.
