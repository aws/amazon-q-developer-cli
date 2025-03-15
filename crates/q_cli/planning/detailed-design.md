# Context Management Feature - Detailed Design Document

## Overview

This document provides a comprehensive design for implementing the context management feature in the Amazon Q Developer CLI. This feature allows users to maintain "sticky" context by specifying files that should always be included in the chat context, organized through profiles.

## Table of Contents

1. [Feature Requirements](#feature-requirements)
2. [Architecture](#architecture)
3. [Data Structures](#data-structures)
4. [File Storage](#file-storage)
5. [Command Interface](#command-interface)
6. [Implementation Plan](#implementation-plan)
7. [Testing Strategy](#testing-strategy)
8. [Future Enhancements](#future-enhancements)

## Feature Requirements

The context management feature must:

1. Allow users to add files/folders to the context globally or for specific profiles
2. Support glob patterns for file selection
3. Maintain separate configurations for global context and profile-specific context
4. Persist configurations between sessions
5. Provide a command interface for managing context files and profiles
6. Automatically include context files in every chat message
7. Indicate the active profile in the command prompt
8. Support switching between profiles
9. Allow creating and deleting profiles
10. Support specifying a profile at startup via CLI flag

## Architecture

The context management feature will be implemented as a new module within the existing Amazon Q Developer CLI codebase. The main components are:

1. **Context Manager**: Core component responsible for managing context files and profiles
2. **Configuration Storage**: JSON files for storing global and profile-specific configurations
3. **Command Interface**: Extensions to the existing slash command system
4. **Conversation Integration**: Updates to include context files in chat messages

### Component Diagram

```
┌─────────────────────┐      ┌─────────────────────┐
│  Command Interface  │◄────►│   Context Manager   │
└─────────────────────┘      └──────────┬──────────┘
                                       │
                                       ▼
┌─────────────────────┐      ┌─────────────────────┐
│    Conversation     │◄────►│   Configuration    │
│     Integration     │      │      Storage       │
└─────────────────────┘      └─────────────────────┘
```

## Data Structures

### Context Configuration

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextConfig {
    pub paths: Vec<String>,
}
```

### Context Manager

```rust
#[derive(Debug, Clone)]
pub struct ContextManager {
    config_dir: PathBuf,
    profiles_dir: PathBuf,
    global_config: ContextConfig,
    current_profile: String,
    profile_config: ContextConfig,
}
```

### Command Enum Extension

```rust
pub enum Command {
    // Existing variants...
    Context { subcommand: ContextSubcommand },
}

pub enum ContextSubcommand {
    Show,
    Add { global: bool, paths: Vec<String> },
    Remove { global: bool, paths: Vec<String> },
    Profile { delete: Option<String>, create: Option<String> },
    Switch { name: String, create: bool },
    Clear { global: bool },
}
```

## File Storage

### Directory Structure

The context management feature will use the following directory structure for storing configurations:

```
~/.aws/amazonq/context/
├── global.json
└── profiles/
    ├── default.json
    ├── profile1.json
    └── profile2.json
```

### File Format

The configuration files will use a simple JSON format:

**Global Configuration (global.json)**:
```json
{
  "paths": [
    "~/.aws/amazonq/rules/**/*.md",
    "AmazonQ.md"
  ]
}
```

**Profile Configuration (profiles/my-profile.json)**:
```json
{
  "paths": [
    "/path/to/project/docs/**/*.md",
    "/path/to/specific/file.md"
  ]
}
```

## Command Interface

The context management feature will extend the existing slash command system with a new `/context` command and several subcommands.

### Command Syntax

```
/context show                                  # Display current context configuration
/context add [--global] <path> [<path> ...]    # Add file(s) to context
/context rm [--global] <path> [<path> ...]     # Remove file(s) from context
/context profile                               # List available profiles
/context profile --create <name>               # Create a new profile
/context profile --delete <name>               # Delete a profile
/context switch <name> [--create]              # Switch to a different profile
/context clear [--global]                      # Clear all files from context
```

### Command Parsing

The command parsing will be implemented by extending the existing `Command::parse` method in `src/cli/chat/command.rs`:

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
                        // Parse add command with paths and --global flag
                        let mut global = false;
                        let mut paths = Vec::new();
                        
                        for part in &parts[2..] {
                            if *part == "--global" {
                                global = true;
                            } else {
                                paths.push(part.to_string());
                            }
                        }
                        
                        if paths.is_empty() {
                            return Err("No paths specified for /context add".to_string());
                        }
                        
                        Self::Context { subcommand: ContextSubcommand::Add { global, paths } }
                    },
                    // Other subcommands...
                }
            },
            // Rest of the commands...
        });
    }
    
    // Rest of the parsing logic...
}
```

### Command Completion

The command completion will be implemented by extending the `COMMANDS` array in `src/cli/chat/prompt.rs`:

```rust
const COMMANDS: &[&str] = &[
    "/clear", 
    "/help", 
    "/acceptall", 
    "/quit",
    "/context",
    "/context show",
    "/context add",
    "/context rm",
    "/context profile",
    "/context switch",
    "/context clear"
];
```

### Help Text

The help text will be updated to include information about the context management feature:

```rust
const HELP_TEXT: &str = color_print::cstr! {"
// Existing help text...
<em>/context</em>      <black!>Manage context files for the chat session</black!>
  <em>show</em>        <black!>Display current context configuration</black!>
  <em>add</em>         <black!>Add file(s) to context [--global]</black!>
  <em>rm</em>          <black!>Remove file(s) from context [--global]</black!>
  <em>profile</em>     <black!>List, create [--create], or delete [--delete] context profiles</black!>
  <em>switch</em>      <black!>Switch to a different context profile [--create]</black!>
  <em>clear</em>       <black!>Clear all files from current context [--global]</black!>
"};
```

## Implementation Plan

The implementation will be divided into several phases to ensure a structured and testable approach:

### Phase 1: Core Context Manager

1. Create a new module `src/cli/chat/context.rs` with the `ContextConfig` and `ContextManager` structs
2. Implement basic file operations for reading and writing configuration files
3. Implement methods for managing paths in global and profile-specific contexts
4. Implement methods for switching between profiles
5. Add unit tests for the core functionality

```rust
impl ContextManager {
    // Initialize the context manager
    pub fn new() -> Result<Self> {
        let config_dir = dirs::home_dir()
            .ok_or_else(|| eyre!("Could not determine home directory"))?
            .join(".aws")
            .join("amazonq")
            .join("context");
        
        let profiles_dir = config_dir.join("profiles");
        
        // Create directories if they don't exist
        fs::create_dir_all(&profiles_dir)?;
        
        // Load global configuration
        let global_config = Self::load_global_config(&config_dir)?;
        
        // Load default profile
        let current_profile = "default".to_string();
        let profile_config = Self::load_profile_config(&profiles_dir, &current_profile)?;
        
        Ok(Self {
            config_dir,
            profiles_dir,
            global_config,
            current_profile,
            profile_config,
        })
    }
    
    // Load global configuration
    fn load_global_config(config_dir: &Path) -> Result<ContextConfig> {
        let global_path = config_dir.join("global.json");
        
        if global_path.exists() {
            let file = File::open(&global_path)?;
            let config: ContextConfig = serde_json::from_reader(file)?;
            Ok(config)
        } else {
            // Default global configuration
            Ok(ContextConfig {
                paths: vec![
                    "~/.aws/amazonq/rules/**/*.md".to_string(),
                    "AmazonQ.md".to_string(),
                ],
            })
        }
    }
    
    // Additional methods...
}
```

### Phase 2: Command Interface

1. Update the `Command` enum in `src/cli/chat/command.rs` to include the `Context` variant
2. Implement parsing logic for the `/context` command and its subcommands
3. Update the command completion in `src/cli/chat/prompt.rs`
4. Update the help text in `src/cli/chat/mod.rs`
5. Add unit tests for command parsing

### Phase 3: Conversation Integration

1. Update the `ConversationState` struct to include a `ContextManager`
2. Modify the `append_new_user_message` method to include context files in the chat message
3. Update the command prompt to indicate the active profile
4. Add unit tests for the conversation integration

### Phase 4: CLI Flag

1. Update the CLI entry point to accept the `--profile` flag
2. Implement logic for initializing the `ContextManager` with the specified profile
3. Add error handling for non-existent profiles
4. Add unit tests for the CLI flag

## Testing Strategy

The testing strategy will include:

1. **Unit Tests**: Test individual components in isolation
   - Test `ContextManager` methods for managing paths and profiles
   - Test command parsing for the `/context` command
   - Test conversation integration for including context files

2. **Integration Tests**: Test the complete feature
   - Test the end-to-end flow of adding context files and seeing them included in chat messages
   - Test switching between profiles and verifying the correct context files are used
   - Test the CLI flag for specifying a profile at startup

3. **Manual Testing**: Verify the user experience
   - Test the command interface for usability
   - Test error messages for clarity
   - Test performance with large context files

## Future Enhancements

Several enhancements could be considered for future iterations:

1. **Context Size Indicator**: Show the percentage of total context that's taken up by the files
2. **Context Ordering**: Allow users to change the order of context files
3. **Context Tagging**: Allow users to tag context files for better organization
4. **Context Sharing**: Allow users to export and import context profiles
5. **Context Validation**: Validate context files before adding them to the context
6. **Context Auto-Detection**: Automatically detect relevant context files based on the current directory
7. **Context Visualization**: Provide a visual representation of the context hierarchy

## Appendix: Context Manager API

This section provides a detailed API reference for the `ContextManager` class:

```rust
impl ContextManager {
    // Initialize the context manager
    pub fn new() -> Result<Self>;
    
    // Load global configuration
    fn load_global_config(config_dir: &Path) -> Result<ContextConfig>;
    
    // Load profile configuration
    fn load_profile_config(profiles_dir: &Path, profile: &str) -> Result<ContextConfig>;
    
    // Switch to a different profile
    pub fn switch_profile(&mut self, profile: &str, create: bool) -> Result<()>;
    
    // Get all context files (global + profile-specific)
    pub fn get_context_files(&self) -> Result<Vec<(String, String)>>;
    
    // Process a path (handling globs and file types)
    fn process_path(&self, path: &str, cwd: &Path, context_files: &mut Vec<(String, String)>) -> Result<()>;
    
    // Add a file to the context collection
    fn add_file_to_context(&self, path: &Path, context_files: &mut Vec<(String, String)>) -> Result<()>;
    
    // Add paths to context (global or profile)
    pub fn add_paths(&mut self, paths: Vec<String>, global: bool) -> Result<()>;
    
    // Remove paths from context (global or profile)
    pub fn remove_paths(&mut self, paths: Vec<String>, global: bool) -> Result<()>;
    
    // Clear all paths (global or profile)
    pub fn clear(&mut self, global: bool) -> Result<()>;
    
    // List all available profiles
    pub fn list_profiles(&self) -> Result<Vec<String>>;
    
    // Create a new profile
    pub fn create_profile(&self, name: &str) -> Result<()>;
    
    // Delete a profile
    pub fn delete_profile(&self, name: &str) -> Result<()>;
    
    // Save configurations to disk
    fn save_config(&self, global: bool) -> Result<()>;
}
```

## Appendix: Command Execution Flow

This section provides a detailed flow for executing each `/context` subcommand:

### Show Command

```rust
match Command::parse(&input) {
    Ok(Command::Context { subcommand: ContextSubcommand::Show }) => {
        if let Some(context_manager) = &conversation_state.context_manager {
            println!("current profile: {}", context_manager.current_profile);
            println!();
            
            println!("global:");
            if context_manager.global_config.paths.is_empty() {
                println!("    <none>");
            } else {
                for path in &context_manager.global_config.paths {
                    println!("    {}", path);
                }
            }
            
            println!();
            println!("profile:");
            if context_manager.profile_config.paths.is_empty() {
                println!("    <none>");
            } else {
                for path in &context_manager.profile_config.paths {
                    println!("    {}", path);
                }
            }
        } else {
            println!("Context manager not initialized");
        }
    },
    // Other commands...
}
```

### Add Command

```rust
match Command::parse(&input) {
    Ok(Command::Context { subcommand: ContextSubcommand::Add { global, paths } }) => {
        if let Some(context_manager) = &mut conversation_state.context_manager {
            match context_manager.add_paths(paths, global) {
                Ok(_) => {
                    let target = if global { "global" } else { &context_manager.current_profile };
                    println!("Added paths to {} context", target);
                },
                Err(e) => {
                    eprintln!("Error adding paths: {}", e);
                }
            }
        } else {
            println!("Context manager not initialized");
        }
    },
    // Other commands...
}
```

### Remove Command

```rust
match Command::parse(&input) {
    Ok(Command::Context { subcommand: ContextSubcommand::Remove { global, paths } }) => {
        if let Some(context_manager) = &mut conversation_state.context_manager {
            match context_manager.remove_paths(paths, global) {
                Ok(_) => {
                    let target = if global { "global" } else { &context_manager.current_profile };
                    println!("Removed paths from {} context", target);
                },
                Err(e) => {
                    eprintln!("Error removing paths: {}", e);
                }
            }
        } else {
            println!("Context manager not initialized");
        }
    },
    // Other commands...
}
```

### Profile Command

```rust
match Command::parse(&input) {
    Ok(Command::Context { subcommand: ContextSubcommand::Profile { delete, create } }) => {
        if let Some(context_manager) = &mut conversation_state.context_manager {
            if let Some(name) = delete {
                match context_manager.delete_profile(&name) {
                    Ok(_) => println!("Deleted profile: {}", name),
                    Err(e) => eprintln!("Error deleting profile: {}", e),
                }
            } else if let Some(name) = create {
                match context_manager.create_profile(&name) {
                    Ok(_) => println!("Created profile: {}", name),
                    Err(e) => eprintln!("Error creating profile: {}", e),
                }
            } else {
                match context_manager.list_profiles() {
                    Ok(profiles) => {
                        for profile in profiles {
                            if profile == context_manager.current_profile {
                                println!("* {}", profile);
                            } else {
                                println!("  {}", profile);
                            }
                        }
                    },
                    Err(e) => eprintln!("Error listing profiles: {}", e),
                }
            }
        } else {
            println!("Context manager not initialized");
        }
    },
    // Other commands...
}
```

### Switch Command

```rust
match Command::parse(&input) {
    Ok(Command::Context { subcommand: ContextSubcommand::Switch { name, create } }) => {
        if let Some(context_manager) = &mut conversation_state.context_manager {
            match context_manager.switch_profile(&name, create) {
                Ok(_) => {
                    if create {
                        println!("Created and switched to profile: {}", name);
                    } else {
                        println!("Switched to profile: {}", name);
                    }
                },
                Err(e) => eprintln!("Error switching profile: {}", e),
            }
        } else {
            println!("Context manager not initialized");
        }
    },
    // Other commands...
}
```

### Clear Command

```rust
match Command::parse(&input) {
    Ok(Command::Context { subcommand: ContextSubcommand::Clear { global } }) => {
        if let Some(context_manager) = &mut conversation_state.context_manager {
            match context_manager.clear(global) {
                Ok(_) => {
                    let target = if global { "global" } else { &context_manager.current_profile };
                    println!("Cleared {} context", target);
                },
                Err(e) => eprintln!("Error clearing context: {}", e),
            }
        } else {
            println!("Context manager not initialized");
        }
    },
    // Other commands...
}
```
