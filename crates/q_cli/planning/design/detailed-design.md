# Context Management Feature - Detailed Design Document

## Overview

This document provides a comprehensive design for implementing the context management feature in the Amazon Q Developer CLI. This feature allows users to maintain "sticky" context by specifying files that should always be included in the chat context, organized through profiles.

## Table of Contents

1. [Feature Requirements](#feature-requirements)
2. [Architecture](#architecture)
3. [Data Structures](#data-structures)
4. [File Storage](#file-storage)
5. [Command Interface](#command-interface)
6. [Implementation Details](#implementation-details)
7. [Context File Inclusion](#context-file-inclusion)
8. [Error Handling](#error-handling)
9. [Testing Strategy](#testing-strategy)
10. [Future Enhancements](#future-enhancements)

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
11. Support adding non-existent files with a force flag
12. Support renaming profiles

## Architecture

The context management feature is implemented as a new module within the existing Amazon Q Developer CLI codebase. The main components are:

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

The `ContextConfig` struct stores a list of file paths or glob patterns:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextConfig {
    pub paths: Vec<String>,
}
```

### Context Manager

The `ContextManager` struct manages the context configurations:

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

The command system is extended with a new `Context` variant and `ContextSubcommand` enum:

```rust
pub enum Command {
    // Existing commands...
    Context { subcommand: ContextSubcommand },
}

pub enum ContextSubcommand {
    Show { expand: bool },
    Add { global: bool, force: bool, paths: Vec<String> },
    Remove { global: bool, paths: Vec<String> },
    Profile { 
        delete: Option<String>, 
        create: Option<String>,
        rename: Option<(String, String)>, // (old_name, new_name)
    },
    Switch { name: String, create: bool },
    Clear { global: bool },
}
```

## File Storage

### Directory Structure

The context management feature uses the following directory structure for storing configurations:

```
~/.aws/amazonq/context/
├── global.json
└── profiles/
    ├── default.json
    ├── profile1.json
    └── profile2.json
```

### File Format

The configuration files use a simple JSON format:

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

The context management feature extends the existing slash command system with a new `/context` command and several subcommands.

### Command Syntax

```
/context show [--expand]                       # Display current context configuration
/context add [--global] [--force] <path> [<path> ...]    # Add file(s) to context
/context rm [--global] <path> [<path> ...]     # Remove file(s) from context
/context profile                               # List available profiles
/context profile --create <name>               # Create a new profile
/context profile --delete <name>               # Delete a profile
/context profile --rename <old_name> <new_name> # Rename a profile
/context switch <name> [--create]              # Switch to a different profile
/context clear [--global]                      # Clear all files from context
```

## Implementation Details

### Files to Modify

1. `src/cli/chat/mod.rs`
   - Add context management command handling
   - Update help text
   - Update chat initialization to support profiles

2. `src/cli/chat/command.rs`
   - Add `Context` variant to `Command` enum
   - Add `ContextSubcommand` enum
   - Update command parsing logic

3. `src/cli/chat/conversation_state.rs`
   - Add `context_manager` field to `ConversationState`
   - Update message creation to include context files

4. `src/cli/chat/prompt.rs`
   - Update command completion
   - Add profile indicator to prompt

5. `src/cli/mod.rs`
   - Add `--profile` flag to CLI options

### Files to Add

1. `src/cli/chat/context.rs`
   - Implement `ContextConfig` struct
   - Implement `ContextManager` struct
   - Implement file and profile management functions

### Key Components

1. **Context Manager**
   - Manages global and profile-specific context configurations
   - Handles file operations and path resolution
   - Provides profile management functions

2. **Command Parser**
   - Parses `/context` commands and their arguments
   - Validates command syntax and parameters

3. **Conversation Integration**
   - Includes context files in chat messages using the format described in the [Context File Inclusion](#context-file-inclusion) section
   - Prepends the formatted context to the user's message before sending to the LLM

4. **Command Prompt**
   - Displays active profile in the prompt
   - Uses color to distinguish profile indicator

5. **CLI Integration**
   - Supports `--profile` flag for specifying profile at startup
   - Validates profile existence before starting chat

## Context File Inclusion

When a user sends a message, the context management feature automatically includes the content of all configured context files at the beginning of the message. This is done in a structured format that allows the LLM to understand the context while keeping it separate from the user's actual message.

### Format Structure

Context files are included in the following format:

```
--- CONTEXT FILES BEGIN ---
[/path/to/file1.md]
Content of file1...

[/path/to/file2.md]
Content of file2...
--- CONTEXT FILES END ---

<user's actual message>
```

This structure:
1. Clearly marks the beginning and end of the context section
2. Includes the full path of each file for reference
3. Preserves the original content of each file
4. Separates the context from the user's message with a blank line

### Implementation

The context file inclusion is implemented in the `append_new_user_message` method of the `ConversationState` struct:

1. When a user sends a message, the method retrieves all context files using `context_manager.get_context_files()`
2. It formats the files with the header, file paths, content, and footer
3. It prepends this formatted context to the user's message
4. The combined message is then sent to the LLM

### Size Considerations

Since LLMs have token limits, the context management feature needs to be mindful of the total size of included files:

1. Very large files may need to be truncated
2. Users should be warned if their context is approaching token limits
3. Future enhancements may include more sophisticated handling of large contexts

## Error Handling

### Command Errors

1. **Missing Subcommand**
   - Error: "Missing subcommand for /context. Try /help for available commands."
   - Occurs when user types `/context` without a subcommand

2. **Unknown Subcommand**
   - Error: "Unknown context subcommand: {subcommand}"
   - Occurs when user provides an invalid subcommand

3. **Invalid Command Options**
   - Error: "Unknown option for /context {subcommand}: {option}"
   - Occurs when user provides an invalid option for a subcommand

### Path Management Errors

1. **No Paths Specified**
   - Error: "No paths specified for /context add"
   - Occurs when user tries to add paths without specifying any

2. **Invalid Path**
   - Error: "Invalid path '{path}': {reason}. Use --force to add anyway."
   - Occurs when a specified path doesn't exist or is invalid

3. **Duplicate Path**
   - Error: "Path '{path}' already exists in the context"
   - Occurs when trying to add a path that's already in the context

4. **Path Not Found**
   - Error: "None of the specified paths were found in the context"
   - Occurs when trying to remove paths that don't exist in the context

5. **Glob Pattern No Matches**
   - Error: "No files found matching glob pattern '{pattern}'"
   - Occurs when a glob pattern doesn't match any files

### Profile Management Errors

1. **Profile Already Exists**
   - Error: "Profile '{name}' already exists"
   - Occurs when trying to create a profile that already exists

2. **Profile Not Found**
   - Error: "Profile '{name}' does not exist. Use --create to create it"
   - Occurs when trying to switch to a non-existent profile

3. **Cannot Delete Default Profile**
   - Error: "Cannot delete the default profile"
   - Occurs when trying to delete the default profile

4. **Cannot Delete Active Profile**
   - Error: "Cannot delete the active profile. Switch to another profile first"
   - Occurs when trying to delete the currently active profile

5. **Invalid Profile Name**
   - Error: "Profile name must start with an alphanumeric character and can only contain alphanumeric characters, hyphens, and underscores"
   - Occurs when creating a profile with an invalid name

6. **Cannot Rename Default Profile**
   - Error: "Cannot rename the default profile"
   - Occurs when trying to rename the default profile

7. **Cannot Rename to Default**
   - Error: "Cannot rename to 'default' as it's a reserved profile name"
   - Occurs when trying to rename a profile to 'default'

8. **Multiple Profile Operations**
   - Error: "Only one of --delete, --create, or --rename can be specified"
   - Occurs when specifying multiple profile operations in one command

### CLI Flag Errors

1. **Profile Does Not Exist**
   - Error: "Profile '{name}' does not exist. Available profiles: {profiles}"
   - Occurs when starting the CLI with a non-existent profile

## Testing Strategy

The testing strategy includes:

1. **Unit Tests**: Test individual components in isolation
   - Test `ContextManager` methods for managing paths and profiles
   - Test command parsing for the `/context` command
   - Test conversation integration for including context files

2. **Integration Tests**: Test the complete feature
   - Test the end-to-end flow of adding context files and seeing them included in chat messages
   - Test switching between profiles and verifying the correct context files are used
   - Test the CLI flag for specifying a profile at startup
   - Test error handling for various edge cases

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
8. **Context Caching**: Cache frequently accessed files to improve performance
9. **Context Filtering**: Filter context files based on content or metadata
10. **Context Compression**: Compress large context files to save space
