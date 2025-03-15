# Context Management Feature Implementation Checklist

This document provides a comprehensive checklist for implementing the context management feature in the Amazon Q Developer CLI, based on the implementation plan outlined in the detailed design document.

## Phase 1: Core Context Manager

### Setup
- [ ] Create new module file `src/cli/chat/context.rs`
- [ ] Add necessary imports:
  - [ ] `std::collections::HashMap`
  - [ ] `std::fs::{self, File}`
  - [ ] `std::io::{Read, Write}`
  - [ ] `std::path::{Path, PathBuf}`
  - [ ] `eyre::{Result, eyre}`
  - [ ] `glob::glob`
  - [ ] `serde::{Deserialize, Serialize}`
  - [ ] `dirs` crate for home directory handling
- [ ] Add `glob` and `dirs` dependencies to Cargo.toml

### Data Structures
- [ ] Implement `ContextConfig` struct:
  - [ ] Add `paths: Vec<String>` field
  - [ ] Derive `Debug`, `Clone`, `Serialize`, `Deserialize`, `Default` traits
- [ ] Implement `ContextManager` struct:
  - [ ] Add `config_dir: PathBuf` field
  - [ ] Add `profiles_dir: PathBuf` field
  - [ ] Add `global_config: ContextConfig` field
  - [ ] Add `current_profile: String` field
  - [ ] Add `profile_config: ContextConfig` field
  - [ ] Derive `Debug` and `Clone` traits

### Core Methods
- [ ] Implement `ContextManager::new()` method:
  - [ ] Determine home directory and create config paths
  - [ ] Create necessary directories if they don't exist
  - [ ] Load global configuration
  - [ ] Load default profile configuration
  - [ ] Return initialized `ContextManager`
- [ ] Implement `ContextManager::load_global_config()` method:
  - [ ] Check if global config file exists
  - [ ] If exists, load and parse JSON
  - [ ] If not, return default configuration
- [ ] Implement `ContextManager::load_profile_config()` method:
  - [ ] Check if profile config file exists
  - [ ] If exists, load and parse JSON
  - [ ] If not, return empty configuration
- [ ] Implement `ContextManager::switch_profile()` method:
  - [ ] Check if profile exists
  - [ ] Create profile if requested and doesn't exist
  - [ ] Load profile configuration
  - [ ] Update current profile name
- [ ] Implement `ContextManager::get_context_files()` method:
  - [ ] Get current working directory
  - [ ] Process global paths
  - [ ] Process profile-specific paths
  - [ ] Return combined list of (filename, content) tuples
- [ ] Implement `ContextManager::process_path()` method:
  - [ ] Handle home directory expansion (~)
  - [ ] Handle relative paths
  - [ ] Handle glob patterns
  - [ ] Process each matching file
- [ ] Implement `ContextManager::add_file_to_context()` method:
  - [ ] Read file content
  - [ ] Add filename and content to context files list
- [ ] Implement `ContextManager::add_paths()` method:
  - [ ] Check for duplicate paths
  - [ ] Add paths to global or profile config
  - [ ] Save updated configuration
- [ ] Implement `ContextManager::remove_paths()` method:
  - [ ] Remove paths from global or profile config
  - [ ] Save updated configuration
- [ ] Implement `ContextManager::clear()` method:
  - [ ] Clear paths from global or profile config
  - [ ] Save updated configuration
- [ ] Implement `ContextManager::list_profiles()` method:
  - [ ] List all profile files in profiles directory
  - [ ] Extract profile names from filenames
  - [ ] Sort alphabetically with "default" first
- [ ] Implement `ContextManager::create_profile()` method:
  - [ ] Validate profile name (alphanumeric, hyphens, underscores)
  - [ ] Create empty profile configuration
  - [ ] Save to file
- [ ] Implement `ContextManager::delete_profile()` method:
  - [ ] Check if profile is "default" (prevent deletion)
  - [ ] Check if profile is current (prevent deletion)
  - [ ] Delete profile configuration file
- [ ] Implement `ContextManager::save_config()` method:
  - [ ] Serialize configuration to JSON
  - [ ] Write to appropriate file (global or profile)

### Unit Tests
- [ ] Write tests for `ContextManager::new()`
- [ ] Write tests for `ContextManager::load_global_config()`
- [ ] Write tests for `ContextManager::load_profile_config()`
- [ ] Write tests for `ContextManager::switch_profile()`
- [ ] Write tests for `ContextManager::get_context_files()`
- [ ] Write tests for `ContextManager::process_path()`
- [ ] Write tests for `ContextManager::add_file_to_context()`
- [ ] Write tests for `ContextManager::add_paths()`
- [ ] Write tests for `ContextManager::remove_paths()`
- [ ] Write tests for `ContextManager::clear()`
- [ ] Write tests for `ContextManager::list_profiles()`
- [ ] Write tests for `ContextManager::create_profile()`
- [ ] Write tests for `ContextManager::delete_profile()`
- [ ] Write tests for `ContextManager::save_config()`

## Phase 2: Command Interface

### Command Enum Extension
- [ ] Update `Command` enum in `src/cli/chat/command.rs`:
  - [ ] Add `Context { subcommand: ContextSubcommand }` variant
- [ ] Create `ContextSubcommand` enum:
  - [ ] Add `Show` variant
  - [ ] Add `Add { global: bool, paths: Vec<String> }` variant
  - [ ] Add `Remove { global: bool, paths: Vec<String> }` variant
  - [ ] Add `Profile { delete: Option<String>, create: Option<String> }` variant
  - [ ] Add `Switch { name: String, create: bool }` variant
  - [ ] Add `Clear { global: bool }` variant

### Command Parsing
- [ ] Update `Command::parse()` method:
  - [ ] Add case for "/context" prefix
  - [ ] Parse "show" subcommand
  - [ ] Parse "add" subcommand with paths and --global flag
  - [ ] Parse "rm" subcommand with paths and --global flag
  - [ ] Parse "profile" subcommand with --create and --delete flags
  - [ ] Parse "switch" subcommand with name and --create flag
  - [ ] Parse "clear" subcommand with --global flag
  - [ ] Add appropriate error handling for missing arguments

### Command Completion
- [ ] Update `COMMANDS` array in `src/cli/chat/prompt.rs`:
  - [ ] Add "/context" command
  - [ ] Add "/context show" command
  - [ ] Add "/context add" command
  - [ ] Add "/context rm" command
  - [ ] Add "/context profile" command
  - [ ] Add "/context switch" command
  - [ ] Add "/context clear" command

### Help Text
- [ ] Update `HELP_TEXT` constant in `src/cli/chat/mod.rs`:
  - [ ] Add "/context" command description
  - [ ] Add subcommand descriptions

### Unit Tests
- [ ] Write tests for parsing "/context show" command
- [ ] Write tests for parsing "/context add" command
- [ ] Write tests for parsing "/context rm" command
- [ ] Write tests for parsing "/context profile" command
- [ ] Write tests for parsing "/context switch" command
- [ ] Write tests for parsing "/context clear" command
- [ ] Write tests for command completion

## Phase 3: Conversation Integration

### ConversationState Update
- [ ] Update `ConversationState` struct:
  - [ ] Add `context_manager: Option<ContextManager>` field
- [ ] Update `ConversationState::new()` method:
  - [ ] Initialize `ContextManager`
  - [ ] Handle profile parameter
  - [ ] Handle initialization errors

### Message Context Integration
- [ ] Update `append_new_user_message()` method:
  - [ ] Get context files from `ContextManager`
  - [ ] Add context files to message context
  - [ ] Format context files with clear section boundaries

### Command Prompt Styling
- [ ] Update prompt generation:
  - [ ] Add profile indicator to prompt
  - [ ] Use color to highlight active profile

### Command Execution
- [ ] Implement execution for `Context` command in main chat loop:
  - [ ] Handle `Show` subcommand
  - [ ] Handle `Add` subcommand
  - [ ] Handle `Remove` subcommand
  - [ ] Handle `Profile` subcommand
  - [ ] Handle `Switch` subcommand
  - [ ] Handle `Clear` subcommand
  - [ ] Add appropriate error handling and user feedback

### Unit Tests
- [ ] Write tests for `ConversationState` with `ContextManager`
- [ ] Write tests for context files in messages
- [ ] Write tests for command execution

## Phase 4: CLI Flag

### CLI Arguments
- [ ] Update `Args` struct:
  - [ ] Add `profile: Option<String>` field with `--profile` flag
- [ ] Update CLI entry point:
  - [ ] Pass profile to `ConversationState::new()`
  - [ ] Add error handling for non-existent profiles

### Unit Tests
- [ ] Write tests for CLI flag handling

## Integration Testing

### End-to-End Tests
- [ ] Test adding context files and verifying they're included in messages
- [ ] Test switching between profiles
- [ ] Test CLI flag for specifying profile at startup
- [ ] Test error handling for various scenarios

### Manual Testing
- [ ] Test command interface usability
- [ ] Test error messages clarity
- [ ] Test performance with large context files
- [ ] Test with various file types and glob patterns
