# Migration Strategy: Internal Command Feature to New Chat Crate Structure

## Overview

This document outlines the strategy for migrating the internal command feature from the `feature/use_q_command` branch to a new branch based on the current main branch, where the chat module has been moved to its own crate.

## Background

- The `feature/use_q_command` branch implements the internal command tool and command registry infrastructure in the q_cli crate's chat module.
- In the main branch, commit `3c00e8ca` moved the chat module from `crates/q_cli/src/cli/chat/` to its own crate at `crates/q_chat/`.
- We need to migrate our changes to work with the new crate structure.

## Current Structure Analysis

### Main Branch Structure
- The chat module has been moved to its own crate at `crates/q_chat/`
- The q_cli crate now depends on q_chat (via workspace dependency)
- The chat functionality is now accessed through the q_chat crate

### Feature Branch Structure
- The chat module is still in the q_cli crate at `crates/q_cli/src/cli/chat/`
- The internal command tool and command registry are implemented in this module
- Significant changes have been made to the command execution flow

## Migration Strategy

We will create a new branch from the current main branch and port our changes to the new structure:

```bash
# Create new branch from main
git checkout origin/main
git checkout -b feature/internal_command
```

## Step-by-Step Migration Process

### 1. Set Up the New Branch

```bash
# Ensure we have the latest main branch
git fetch origin
git checkout origin/main
# Create new branch
git checkout -b feature/internal_command
```

### 2. Port Command Registry Infrastructure

1. Create the commands directory structure in q_chat:

```bash
mkdir -p crates/q_chat/src/commands/context
mkdir -p crates/q_chat/src/commands/profile
mkdir -p crates/q_chat/src/commands/tools
```

2. Port the command registry files:
   - `crates/q_cli/src/cli/chat/commands/mod.rs` → `crates/q_chat/src/commands/mod.rs`
   - `crates/q_cli/src/cli/chat/commands/handler.rs` → `crates/q_chat/src/commands/handler.rs`
   - `crates/q_cli/src/cli/chat/commands/registry.rs` → `crates/q_chat/src/commands/registry.rs`

3. Update imports in these files:
   - Replace `crate::cli::chat::*` with appropriate paths in the new structure
   - Update relative imports to match the new structure

### 3. Port Command Handlers

1. Port basic command handlers:
   - `crates/q_cli/src/cli/chat/commands/help.rs` → `crates/q_chat/src/commands/help.rs`
   - `crates/q_cli/src/cli/chat/commands/quit.rs` → `crates/q_chat/src/commands/quit.rs`
   - `crates/q_cli/src/cli/chat/commands/clear.rs` → `crates/q_chat/src/commands/clear.rs`

2. Port context command handlers:
   - `crates/q_cli/src/cli/chat/commands/context/mod.rs` → `crates/q_chat/src/commands/context/mod.rs`
   - `crates/q_cli/src/cli/chat/commands/context/add.rs` → `crates/q_chat/src/commands/context/add.rs`
   - `crates/q_cli/src/cli/chat/commands/context/remove.rs` → `crates/q_chat/src/commands/context/remove.rs`
   - `crates/q_cli/src/cli/chat/commands/context/clear.rs` → `crates/q_chat/src/commands/context/clear.rs`
   - `crates/q_cli/src/cli/chat/commands/context/show.rs` → `crates/q_chat/src/commands/context/show.rs`

3. Port profile command handlers:
   - `crates/q_cli/src/cli/chat/commands/profile/mod.rs` → `crates/q_chat/src/commands/profile/mod.rs`
   - `crates/q_cli/src/cli/chat/commands/profile/*.rs` → `crates/q_chat/src/commands/profile/*.rs`

4. Port tools command handlers:
   - `crates/q_cli/src/cli/chat/commands/tools/mod.rs` → `crates/q_chat/src/commands/tools/mod.rs`
   - `crates/q_cli/src/cli/chat/commands/tools/*.rs` → `crates/q_chat/src/commands/tools/*.rs`

5. Port other command handlers:
   - `crates/q_cli/src/cli/chat/commands/compact.rs` → `crates/q_chat/src/commands/compact.rs`
   - `crates/q_cli/src/cli/chat/commands/tools.rs` → `crates/q_chat/src/commands/tools.rs`
   - `crates/q_cli/src/cli/chat/commands/test_utils.rs` → `crates/q_chat/src/commands/test_utils.rs`

### 4. Port Internal Command Tool

1. Create the internal_command directory in q_chat:

```bash
mkdir -p crates/q_chat/src/tools/internal_command
```

2. Port the internal command tool files:
   - `crates/q_cli/src/cli/chat/tools/internal_command/mod.rs` → `crates/q_chat/src/tools/internal_command/mod.rs`
   - `crates/q_cli/src/cli/chat/tools/internal_command/tool.rs` → `crates/q_chat/src/tools/internal_command/tool.rs`
   - `crates/q_cli/src/cli/chat/tools/internal_command/schema.rs` → `crates/q_chat/src/tools/internal_command/schema.rs`
   - `crates/q_cli/src/cli/chat/tools/internal_command/permissions.rs` → `crates/q_chat/src/tools/internal_command/permissions.rs`
   - `crates/q_cli/src/cli/chat/tools/internal_command/test.rs` → `crates/q_chat/src/tools/internal_command/test.rs`

3. Update the tool registration in `crates/q_chat/src/tools/mod.rs` to include the internal command tool

### 5. Port Tests

1. Port the command execution tests:
   - `crates/q_cli/src/cli/chat/command_execution_tests.rs` → `crates/q_chat/src/command_execution_tests.rs`

2. Port the AI command interpretation tests:
   - `crates/q_cli/tests/ai_command_interpretation/` → `crates/q_chat/tests/ai_command_interpretation/`

3. Update test imports and dependencies

### 6. Update Integration with q_cli

1. Update the chat function in q_cli to use the new q_chat crate:
   - Modify `crates/q_cli/src/cli/mod.rs` to import and use the chat function from q_chat

2. Ensure proper dependencies are set in Cargo.toml files

### 7. Testing and Validation

After each component migration:

```bash
# Build and test the q_chat crate
cargo build -p q_chat
cargo test -p q_chat

# Build and test the q_cli crate
cargo build -p q_cli
cargo test -p q_cli

# Test the full application
cargo build
cargo test
```

## Import Update Guidelines

When updating imports, follow these patterns:

### In q_chat crate:

- Replace `crate::cli::chat::*` with `crate::*`
- Replace `super::*` with appropriate relative paths
- Use `crate::commands::*` for command-related imports
- Use `crate::tools::*` for tool-related imports

### In q_cli crate:

- Replace `crate::cli::chat::*` with `q_chat::*`
- Use `q_chat::commands::*` for command-related imports
- Use `q_chat::tools::*` for tool-related imports

## Commit Strategy

Commit changes incrementally after each component is successfully migrated:

```bash
# Example commit
git add .
git commit -m "feat(chat): Migrate command registry to q_chat crate"
```

Follow the Conventional Commits specification for all commits:
- Use appropriate types (feat, fix, refactor, etc.)
- Include scope (chat, internal_command, etc.)
- Add detailed descriptions
- Include "🤖 Assisted by [Amazon Q Developer](https://aws.amazon.com/q/developer)" footer

## Success Criteria

The migration is considered successful when:

1. All components from the `feature/use_q_command` branch are ported to the new structure
2. All tests pass
3. The application builds successfully
4. All functionality works as expected

🤖 Assisted by [Amazon Q Developer](https://aws.amazon.com/q/developer)
