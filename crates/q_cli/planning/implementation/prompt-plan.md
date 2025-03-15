# Context Management Feature Implementation Prompt Plan

This document outlines a series of prompts for implementing the context management feature in a test-driven manner. Each prompt builds on the previous ones, ensuring incremental progress and early testing.

## Prompt 1: Create the Core Data Structures

```
Create the core data structures for the context management feature. Implement the `ContextConfig` struct for storing paths and the `ContextManager` struct for managing context configurations. Include basic methods for initialization and configuration loading/saving.

Specifically:
1. Create a new file `src/cli/chat/context.rs` with the `ContextConfig` and `ContextManager` structs
2. Implement serialization/deserialization for `ContextConfig` using serde
3. Add methods to initialize the `ContextManager` with default paths
4. Implement methods to load and save configurations from/to JSON files
5. Write unit tests for these core functions

Focus on the data structures and file operations without implementing the command interface yet.
```

## Prompt 2: Implement Path Management Functions

```
Implement the path management functions for the `ContextManager`. These functions should handle adding, removing, and listing paths in both global and profile-specific contexts.

Specifically:
1. Implement `add_paths` method to add paths to global or profile context
2. Implement `remove_paths` method to remove paths from global or profile context
3. Implement `clear` method to clear all paths from global or profile context
4. Add validation to prevent duplicate paths
5. Write unit tests for these path management functions

Ensure proper error handling for common issues like invalid paths or duplicate entries.
```

## Prompt 3: Implement Profile Management Functions

```
Implement the profile management functions for the `ContextManager`. These functions should handle creating, deleting, switching, and listing profiles.

Specifically:
1. Implement `list_profiles` method to list all available profiles
2. Implement `create_profile` method to create a new profile
3. Implement `delete_profile` method to delete an existing profile
4. Implement `switch_profile` method to switch to a different profile
5. Add validation to prevent deleting the active profile or the default profile
6. Write unit tests for these profile management functions

Ensure proper error handling for common issues like non-existent profiles or permission issues.
```

## Prompt 4: Implement Context File Processing

```
Implement the context file processing functions for the `ContextManager`. These functions should handle reading file content and preparing it for inclusion in chat messages.

Specifically:
1. Implement `get_context_files` method to get all context files (global + profile-specific)
2. Implement `process_path` helper method to handle glob patterns and file types
3. Implement `add_file_to_context` helper method to read file content and add it to the context collection
4. Add support for relative paths, absolute paths, and home directory expansion
5. Write unit tests for these context file processing functions

Ensure proper error handling for file reading issues and implement size limits for context files if necessary.
```

## Prompt 5: Extend the Command Enum and Parser

```
Extend the Command enum and parser to support the new `/context` command and its subcommands.

Specifically:
1. Add a new `Context` variant to the `Command` enum in `src/cli/chat/command.rs`
2. Create a `ContextSubcommand` enum for the various subcommands (show, add, rm, profile, switch, clear)
3. Update the `Command::parse` method to handle the `/context` command and its subcommands
4. Add parameter parsing for flags like `--global` and `--create`
5. Write unit tests for the command parsing logic

Ensure the command parsing is consistent with the existing code style and error handling patterns.
```

## Prompt 6: Implement Command Completion

```
Implement command completion for the `/context` command and its subcommands.

Specifically:
1. Update the `COMMANDS` array in `src/cli/chat/prompt.rs` to include the `/context` command and its subcommands
2. Modify the `ChatCompleter` implementation if necessary to handle the new commands
3. Write unit tests for the command completion logic

Ensure the command completion is consistent with the existing code style and behavior.
```

## Prompt 7: Update the Help Text

```
Update the help text to include information about the context management feature.

Specifically:
1. Add the `/context` command and its subcommands to the `HELP_TEXT` constant in `src/cli/chat/mod.rs`
2. Format the help text consistently with the existing help text
3. Include clear descriptions of each subcommand and its options

Ensure the help text is clear, concise, and follows the existing formatting patterns.
```

## Prompt 8: Integrate with ConversationState

```
Integrate the context management feature with the `ConversationState` struct.

Specifically:
1. Add a `context_manager` field to the `ConversationState` struct in `src/cli/chat/conversation.rs`
2. Update the `ConversationState::new` method to initialize the `ContextManager`
3. Modify the `append_new_user_message` method to include context files in chat messages
4. Add support for switching profiles based on the CLI flag
5. Write unit tests for the integration

Ensure the integration is seamless and doesn't break existing functionality.
```

## Prompt 9: Implement Command Execution

```
Implement the execution of the `/context` command and its subcommands in the main chat loop.

Specifically:
1. Add a case for the `Context` command in the main chat loop in `src/cli/chat/mod.rs`
2. Implement handlers for each subcommand (show, add, rm, profile, switch, clear)
3. Add appropriate output formatting for command results
4. Ensure proper error handling and user feedback
5. Write integration tests for the command execution

Ensure the command execution is consistent with the existing code style and behavior.
```

## Prompt 10: Update the Command Prompt

```
Update the command prompt to indicate the active context profile.

Specifically:
1. Create a function to generate the prompt string based on the active profile
2. Modify the main chat loop to use this function for the prompt
3. Use terminal styling to make the profile indicator visually distinct
4. Write unit tests for the prompt generation

Ensure the prompt is clear, concise, and visually appealing.
```

## Prompt 11: Add CLI Flag for Profile

```
Add a CLI flag for specifying a profile at startup.

Specifically:
1. Update the CLI entry point to accept the `--profile` flag
2. Modify the `chat` function to use the specified profile
3. Add error handling for non-existent profiles
4. Write unit tests for the CLI flag handling

Ensure the CLI flag is consistent with the existing code style and behavior.
```

## Prompt 12: Integration Testing

```
Create integration tests for the context management feature.

Specifically:
1. Test the end-to-end flow of adding context files and seeing them included in chat messages
2. Test switching between profiles and verifying the correct context files are used
3. Test the CLI flag for specifying a profile at startup
4. Test error handling for various edge cases

Ensure the tests cover all major functionality and edge cases.
```

## Prompt 13: Documentation and Final Touches

```
Add documentation and final touches to the context management feature.

Specifically:
1. Add documentation comments to all public functions and types
2. Update the README.md file to include information about the context management feature
3. Add examples of common usage patterns
4. Perform a final code review to ensure consistency and quality
5. Address any remaining TODOs or edge cases

Ensure the documentation is clear, concise, and helpful for users and developers.
```

## Prompt 14: Refactoring and Optimization

```
Refactor and optimize the context management feature.

Specifically:
1. Look for opportunities to improve code organization and readability
2. Optimize file reading and processing for large context files
3. Consider adding caching for frequently accessed files
4. Improve error messages and user feedback
5. Address any performance issues identified during testing

Ensure the code is maintainable, efficient, and follows best practices.
```

## Prompt 15: Final Integration and Testing

```
Perform final integration and testing of the context management feature.

Specifically:
1. Ensure all components work together seamlessly
2. Verify that the feature doesn't break existing functionality
3. Test with various file types, sizes, and configurations
4. Verify that error handling works as expected in all scenarios
5. Ensure the user experience is smooth and intuitive

Address any issues found during final testing and prepare the feature for release.
```
