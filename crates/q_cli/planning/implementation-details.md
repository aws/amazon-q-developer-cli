# Context Management Feature Implementation Details

## Overview

This document outlines the technical implementation details for the context management feature in the Amazon Q Developer CLI. The feature allows users to maintain "sticky" context by specifying files that should always be included in the chat context, organized through aliases.

## Implementation Components

### 1. Context Management Module

Create a new module `src/cli/chat/context.rs` to handle all context management functionality:

```rust
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use eyre::{Result, eyre};
use glob::glob;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextConfig {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ContextManager {
    config_dir: PathBuf,
    aliases_dir: PathBuf,
    global_config: ContextConfig,
    current_alias: String,
    alias_config: ContextConfig,
}
```

### 2. Configuration Storage

The configurations will be stored in JSON files:

- Global context: `~/.aws/amazonq/context/global.json`
- Alias-specific context: `~/.aws/amazonq/context/aliases/[alias-name].json`

Example global.json:
```json
{
  "paths": [
    "~/.aws/amazonq/rules/**/*.md",
    "AmazonQ.md"
  ]
}
```

### 3. Context Manager Methods

Key methods for the `ContextManager`:

```rust
impl ContextManager {
    // Initialize the context manager
    pub fn new() -> Result<Self> { /* ... */ }
    
    // Load global configuration
    pub fn load_global_config(config_dir: &Path) -> Result<ContextConfig> { /* ... */ }
    
    // Switch to a different alias
    pub fn switch_alias(&mut self, alias: &str, create: bool) -> Result<()> { /* ... */ }
    
    // Get all context files (global + alias-specific)
    pub fn get_context_files(&self) -> Result<Vec<(String, String)>> { /* ... */ }
    
    // Process a path (handling globs and file types)
    fn process_path(&self, path: &str, cwd: &Path, context_files: &mut Vec<(String, String)>) -> Result<()> { /* ... */ }
    
    // Add a file to the context collection
    fn add_file_to_context(&self, path: &Path, context_files: &mut Vec<(String, String)>) -> Result<()> { /* ... */ }
    
    // Add paths to context (global or alias)
    pub fn add_paths(&mut self, paths: Vec<String>, global: bool) -> Result<()> { /* ... */ }
    
    // Remove paths from context (global or alias)
    pub fn remove_paths(&mut self, paths: Vec<String>, global: bool) -> Result<()> { /* ... */ }
    
    // Clear all paths (global or alias)
    pub fn clear(&mut self, global: bool) -> Result<()> { /* ... */ }
    
    // List all available aliases
    pub fn list_aliases(&self) -> Result<Vec<String>> { /* ... */ }
    
    // Create a new alias
    pub fn create_alias(&self, name: &str) -> Result<()> { /* ... */ }
    
    // Delete an alias
    pub fn delete_alias(&self, name: &str) -> Result<()> { /* ... */ }
    
    // Save configurations to disk
    fn save_config(&self, global: bool) -> Result<()> { /* ... */ }
}
```

### 4. ConversationState Integration

Modify the `ConversationState` struct to include a `ContextManager`:

```rust
#[derive(Debug, Clone)]
pub struct ConversationState {
    // Existing fields...
    context_manager: Option<ContextManager>,
}
```

Update the constructor to initialize the `ContextManager`:

```rust
pub fn new(tool_config: HashMap<String, ToolSpec>, alias: Option<String>) -> Self {
    // Existing code...
    
    // Initialize context manager
    let context_manager = match ContextManager::new() {
        Ok(mut manager) => {
            if let Some(alias_name) = alias {
                if let Err(e) = manager.switch_alias(&alias_name, false) {
                    error!("Failed to switch to alias {}: {}", alias_name, e);
                }
            }
            Some(manager)
        },
        Err(e) => {
            error!("Failed to initialize context manager: {}", e);
            None
        }
    };
    
    Self {
        // Existing fields...
        context_manager,
    }
}
```

### 5. Context File Processing

Update the `append_new_user_message` method to include context files:

```rust
pub fn append_new_user_message(&mut self, input: String) {
    // Existing code...

    // Get context files if context manager is available
    let context_files = if let Some(context_manager) = &self.context_manager {
        match context_manager.get_context_files() {
            Ok(files) => {
                if !files.is_empty() {
                    debug!("Adding {} context files to message", files.len());
                    Some(files)
                } else {
                    None
                }
            },
            Err(e) => {
                error!("Failed to get context files: {}", e);
                None
            }
        }
    } else {
        None
    };

    let msg = UserInputMessage {
        content: input,
        user_input_message_context: Some(UserInputMessageContext {
            // Existing fields...
            context_files, // Add context files to the message
            ..Default::default()
        }),
        user_intent: None,
    };
    self.next_message = Some(msg);
}
```

### 6. API Model Update

Update the `UserInputMessageContext` struct in the API client:

```rust
pub struct UserInputMessageContext {
    // Existing fields...
    pub context_files: Option<Vec<(String, String)>>, // (filename, content)
}
```

### 7. Command Implementation

Add a new variant to the `Command` enum:

```rust
pub enum Command {
    // Existing variants...
    Context { subcommand: ContextSubcommand },
}

pub enum ContextSubcommand {
    Show,
    Add { global: bool, paths: Vec<String> },
    Remove { global: bool, paths: Vec<String> },
    Alias { delete: Option<String>, create: Option<String> },
    Switch { name: String, create: bool },
    Clear { global: bool },
}
```

### 8. Command Parsing

Update the `parse` method to handle the `/context` command:

```rust
pub fn parse(input: &str) -> Result<Self, String> {
    // Existing code...
    
    if let Some(command) = input.strip_prefix("/") {
        let parts: Vec<&str> = command.split_whitespace().collect();
        
        return Ok(match parts[0].to_lowercase().as_str() {
            // Existing commands...
            "context" => {
                if parts.len() < 2 {
                    return Err("Missing subcommand for /context".to_string());
                }
                
                match parts[1].to_lowercase().as_str() {
                    "show" => Self::Context { subcommand: ContextSubcommand::Show },
                    "add" => {
                        // Parse add command
                    },
                    "rm" => {
                        // Parse rm command
                    },
                    "alias" => {
                        // Parse alias command
                    },
                    "switch" => {
                        // Parse switch command
                    },
                    "clear" => {
                        // Parse clear command
                    },
                    _ => return Err(format!("Unknown context subcommand: {}", parts[1])),
                }
            },
            // Rest of the commands...
        });
    }
    
    // Rest of the parsing logic...
}
```

### 9. Command Execution

Update the main chat loop to handle the `Context` command:

```rust
match Command::parse(&input) {
    // Existing commands...
    Ok(Command::Context { subcommand }) => {
        match subcommand {
            ContextSubcommand::Show => {
                // Display current context configuration
            },
            ContextSubcommand::Add { global, paths } => {
                // Add paths to context
            },
            ContextSubcommand::Remove { global, paths } => {
                // Remove paths from context
            },
            ContextSubcommand::Alias { delete, create } => {
                // Handle alias operations
            },
            ContextSubcommand::Switch { name, create } => {
                // Switch to a different alias
            },
            ContextSubcommand::Clear { global } => {
                // Clear context
            },
        }
    },
    // Rest of the match statement...
}
```

### 10. CLI Flag for Alias

Update the CLI entry point to accept the `--alias` flag:

```rust
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    // Existing fields...
    
    /// Specify a context alias to use for the chat session
    #[arg(long)]
    alias: Option<String>,
}
```

### 11. Help Text Update

Add the `/context` command to the help text:

```rust
const HELP_TEXT: &str = color_print::cstr! {"
// Existing help text...
<em>/context</em>      <black!>Manage context files for the chat session</black!>
  <em>show</em>        <black!>Display current context configuration</black!>
  <em>add</em>         <black!>Add file(s) to context [--global]</black!>
  <em>rm</em>          <black!>Remove file(s) from context [--global]</black!>
  <em>alias</em>       <black!>List, create [--create], or delete [--delete] context aliases</black!>
  <em>switch</em>      <black!>Switch to a different context alias [--create]</black!>
  <em>clear</em>       <black!>Clear all files from current context [--global]</black!>
"};
```

### 12. Context File Formatting

Format context files with clear section boundaries:

```
--- CONTEXT FILES BEGIN ---
[filename.md]
<file content>
--- CONTEXT FILES END ---

--- CHAT HISTORY BEGIN ---
<chat messages>
```

### 13. Command Prompt Styling

Update the prompt to indicate the current context alias:

```rust
fn get_prompt(context_manager: &Option<ContextManager>) -> String {
    if let Some(manager) = context_manager {
        format!("[context:{}] > ", manager.current_alias)
    } else {
        "> ".to_string()
    }
}
```

## Processing Order

Context files will be processed in the following order:

1. Global context files first, in the order they appear in the configuration
2. Alias-specific context files next, in the order they appear in the configuration
3. For glob patterns, files will be processed in the order they're returned by the glob expansion

This ensures that alias-specific context can override or supplement global context as needed.

## Error Handling

The implementation will provide clear error messages for common issues:

- For invalid file paths: "File not found: [path]"
- For permission issues: "Permission denied: [path]"
- For non-existent aliases: "Alias not found: [alias-name]"
- For duplicate paths: "Path already exists in context: [path]"

## Dependencies

Additional dependencies required:

- `glob` crate for handling glob patterns
- `dirs` crate for handling home directory expansion
- `serde` and `serde_json` for JSON configuration handling
