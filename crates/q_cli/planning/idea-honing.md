---
### INSTRUCTIONS: DO NOT MODIFY OR DELETE ###
Read rough-idea.md. Ask me one question at a time so we can develop a thorough, step-by-step spec for this idea. Each question should build on my previous answers, and our end goal is to have a detailed specification I can hand off to a developer. Let's do this iteratively and dig into every relevant detail. Remember, only one question at a time and don't repeat questions I've already answered. Update idea-honing.md as we go to track our progress.
### END INSTRUCTIONS ###
---

# Context Management Feature Specification Development

## Q&A Summary

### Q1: What specific problems are you trying to solve with the context management feature that aren't addressed by the current implementation of Amazon Q Developer CLI?

**A1:** Today, Q CLI chat maintains a context window, but older chat messages can age off the context window, leading to Q "forgetting" important things like the current task or goal you're trying to drive toward. This feature addresses this by allowing you to specify files that will always be passed as context on every chat message.

### Q2: How should the context management feature handle file size limits and prioritization when multiple context files are added?

**A2:** The file content should be added to the context in order. Global context file content should be added first, followed by alias-specific context file content.

### Q3: How should the system handle cases where the total size of all context files exceeds the maximum context window size?

**A3:** Eventually, the show command should include the percentage of total context that's taken up by the files, but this feature can be deferred for now since it's not clear how much context is actually available.

### Q4: How should the context management feature handle file paths and glob patterns?

**A4:** It should expand glob patterns and include the content of all returned files in the context. If a specified file does not exist, it should be ignored.

### Q5: How should the context configurations (global and alias-specific) be persisted between sessions? And where should this configuration be stored?

**A5:** The configurations should be stored in files in ~/.aws/amazonq/context/

### Q6: What format should be used for storing the context configurations, and how should the directory structure be organized?

**A6:** JSON-based configuration with:
- Global context stored in `~/.aws/amazonq/context/global.json`
- Each alias stored in a separate file like `~/.aws/amazonq/context/aliases/[alias-name].json`
- This format is simple to parse and modify programmatically while remaining human-readable and editable

### Q7: How should the /context command handle subcommands and arguments?

**A7:** Git-style subcommands with the following structure:
```
/context show
/context add [--global] <file_path> [<file_path> ...]
/context rm [--global] <file_path> [<file_path> ...]
/context alias
/context switch <alias_name> [--create]
```

### Q8: How should the system handle the default context when a user first uses the feature?

**A8:** It should default to a global context that includes `~/.aws/amazonq/rules/**/*.md` and `AmazonQ.md`. This can be hardcoded into the logic and used if no global.json file is found. When the user modifies global context for the first time, then write the global.json file.

### Q9: How should the system track which alias is currently active, and what should happen when a user starts a new chat session?

**A9:** At startup, always use the default alias unless the --alias flag is passed to the q chat command with the alias to use instead.

### Q10: How should the system handle errors such as invalid file paths, permission issues, or other potential problems when adding files to context or switching aliases?

**A10:** The system should provide clear error messages for common issues:
- For invalid file paths: "File not found: [path]"
- For permission issues: "Permission denied: [path]"
- For non-existent aliases: "Alias not found: [alias-name]"
- For other errors: Descriptive messages that explain what went wrong and how to fix it

### Q11: How should the system handle the actual integration of context files into the chat conversation? Should it be transparent to the user or should there be some indication that context files are being used?

**A11:** The system should indicate the active context by modifying the prompt with a colored prefix showing the current alias. The CLI already uses the `crossterm` crate for terminal styling, so we'll use that to implement a colored prompt like `[context:alias-name] >`. This provides a clear visual indicator of which context alias is active without being intrusive to the chat experience. Alias names should only be allowed to contain alphanumeric characters, hyphens, and underscores.

### Q12: How should the context management feature handle updates to context files during an active chat session? Should changes to files be automatically reflected, or should there be a command to refresh the context?

**A12:** Changes to files should be automatically reflected in the context. When sending a new message in the chat, the system should read the current content of all context files, ensuring that any changes made to those files since the last message are included in the new context. This provides a seamless experience where users can edit context files and have those changes immediately available in the conversation without requiring manual refresh commands.

### Q13: How should the context management feature handle the formatting and presentation of context files to the AI? Should there be any special formatting or metadata included to help the AI understand the context better?

**A13:** The system should use a clear section boundary format to distinguish context files from chat history:

```
--- CONTEXT FILES BEGIN ---
[filename.md]
<file content>
--- CONTEXT FILES END ---

--- CHAT HISTORY BEGIN ---
<chat messages>
```

This format provides clear identification of context files with their filenames and establishes a definitive boundary between context and chat history, helping the AI understand the different sections of input it's receiving.

### Q14: How should the system handle the `/context` command implementation within the existing slash command framework? What changes would be needed to integrate this new command?

**A14:** The implementation should follow these steps to integrate with the existing slash command framework:

1. **Extend the Command Enum**:
   Add a new variant to the `Command` enum in `src/cli/chat/command.rs`:

   ```rust
   pub enum Command {
       Ask { prompt: String },
       Execute { command: String },
       Clear,
       Help,
       AcceptAll,
       Quit,
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

2. **Update the Command Parser**:
   Modify the `parse` method to handle the `/context` command and its subcommands.

3. **Update Command Completion**:
   Add the `/context` command and its subcommands to the `COMMANDS` array in `src/cli/chat/prompt.rs`.

4. **Implement Command Execution**:
   Add a case for the `Context` command in the main chat loop in `src/cli/chat/mod.rs`.

5. **Update Help Text**:
   Add the `/context` command to the `HELP_TEXT` constant.

6. **Create Context Management Module**:
   Create a new module `src/cli/chat/context.rs` to handle the context management functionality.

7. **Integrate with Conversation State**:
   Modify the `ConversationState` class to include context files in the chat messages sent to the API.

### Q15: How should the system handle the CLI flag for specifying an alias at startup? What would be the syntax and behavior of this flag?

**A15:** The system should implement a `--alias` flag that takes the alias name as a required argument:

```
q chat --alias my-team-service
```

If the specified alias doesn't exist, the system should display an error message and exit:
```
Error: Alias 'my-team-service' not found. Use '/context alias --create my-team-service' to create it.
```

However, specifying `--alias default` should never produce an error, even though it's functionally equivalent to not using the flag at all, since "default" is the standard alias used when no alias is specified.

### Q16: How should the system handle the JSON schema for the configuration files? What fields should be included in the global and alias-specific configuration files?

**A16:** The system should use a simple path list schema for both global and alias-specific configuration files:

**Global configuration (global.json)**:
```json
{
  "paths": [
    "~/.aws/amazonq/rules/**/*.md",
    "AmazonQ.md"
  ]
}
```

**Alias configuration (aliases/my-alias.json)**:
```json
{
  "paths": [
    "/path/to/project/docs/**/*.md",
    "/path/to/specific/file.md"
  ]
}
```

This simple schema focuses on the essential information (the paths to include in context) without unnecessary metadata, making it easy to read, write, and maintain.

### Q17: How should the system handle the output of the `/context show` command? What information should be displayed and in what format?

**A17:** Based on the examples in planning/idea.md, the `/context show` command should display:

1. The current active alias name
2. The global context paths (indented)
3. The alias-specific context paths (indented)

Example output format:
```
current alias: my-team-service

global:
    ~/.aws/amazonq/rules/**/*.md
    AmazonQ.md
    /path/to/MyTeamQLib/**/*.md

alias:
    /path/to/MyTeamServiceDocs/AmazonQ.md
    /path/to/MyTeamServiceDocs/tasks/P129406383/**/*
```

If there are no paths in either the global or alias-specific context, it should display `<none>` under the respective section. The output should be formatted with clear section headers and indentation to make it easy to read and distinguish between global and alias-specific context paths.

### Q18: How should the system handle the `/context alias` command? What information should be displayed and in what format?

**A18:** The `/context alias` command should list all available aliases (including the default alias) in a format similar to `git branch`, with the current alias highlighted or marked with an asterisk:

```
  default
* my-team-service
  project-x
  golang-dev
```

In this example:
- Each alias is listed on a separate line
- The current active alias (my-team-service) is marked with an asterisk and could be highlighted using the `crossterm` crate's styling capabilities
- The list is alphabetically sorted, with "default" always appearing first
- The output is simple and clean, making it easy to scan the available aliases

### Q19: What should happen when a user tries to add a file path that already exists in the context? Should it be ignored, replaced, or should the user be notified?

**A19:** When a user tries to add a file path that already exists in the context, the system should return an error message to notify the user of the duplicate:

```
Error: Path '/path/to/file.md' already exists in the context.
```

This approach:
1. Prevents unintentional duplicates that could waste context space
2. Provides clear feedback to the user about what happened
3. Allows users to make informed decisions about their context configuration
4. Maintains the integrity of the context configuration by ensuring each path appears only once

### Q20: How should the system handle relative file paths in the context configuration? Should they be stored as relative paths or converted to absolute paths?

**A20:** The system should store relative file paths as they are provided, relative to the directory in which `q chat` was called. When reading the context files:

1. Relative paths (those not starting with `/` or `~`) should be resolved relative to the current working directory at the time of reading the files
2. Absolute paths (starting with `/`) and home-directory paths (starting with `~`) should be used as-is

This approach:
1. Maintains the portability of context configurations across different environments
2. Allows users to create context configurations that work with project-relative paths
3. Ensures that context files can be found correctly regardless of where the `q chat` command is executed from
4. Preserves the user's intent when specifying paths in different formats

### Q21: How should the system handle the case where a user wants to completely reset their context configuration? Should there be a specific command for this purpose?

**A21:** The system should provide a `/context clear` command to allow users to completely reset their context configuration. This command should:

1. Take an optional `--global` flag to clear the global context
2. If the `--global` flag is not provided, only clear the context for the current alias
3. Provide confirmation of the action with a message like:
   - "Cleared context for alias 'my-team-service'"
   - "Cleared global context"

This approach:
1. Gives users a simple way to start fresh with their context configuration
2. Provides flexibility to clear either global context or alias-specific context
3. Maintains the separation between global and alias-specific contexts
4. Follows the pattern of other commands with the `--global` flag

### Q22: How should the system handle the deletion of aliases? Should there be a specific command for this purpose?

**A22:** The system should extend the `/context alias` command to include a `--delete` or `-d` flag for deleting aliases, similar to the `git branch` command:

```
/context alias --delete my-team-service
```

This command should:
1. Delete the specified alias configuration file
2. Prevent deletion of the "default" alias
3. Prevent deletion of the currently active alias (user must switch to another alias first)
4. Provide confirmation of the deletion with a message like: "Deleted alias 'my-team-service'"
5. Return an error if the specified alias doesn't exist

This approach:
1. Follows the Git-style command pattern that users may already be familiar with
2. Provides a clear and explicit way to remove aliases that are no longer needed
3. Includes safeguards to prevent accidental deletion of important aliases
4. Maintains consistency with the rest of the command structure

### Q23: How should the system handle the creation of new aliases? Should it be part of the switch command or a separate operation?

**A23:** The system should support creating aliases in two ways:

1. Using the `/context alias` command with a `--create` or `-c` flag:
   ```
   /context alias --create my-team-service
   Created alias: my-team-service
   ```

2. Using the `/context switch` command with a `--create` or `-c` flag (for convenience):
   ```
   /context switch --create my-team-service
   Created and switched to alias: my-team-service
   ```

This dual approach:
1. Makes the `/context alias` command the central place for all alias management operations (listing, creating, deleting)
2. Provides a convenient shortcut for creating and switching to an alias in one command
3. Follows a more intuitive command structure where related operations are grouped together
4. Provides flexibility for different user workflows

If a user tries to switch to an alias that doesn't exist without using the `--create` flag, the system should return an error message:

```
Error: Alias 'my-team-service' not found. Use '/context alias --create my-team-service' to create it or '/context switch --create my-team-service' to create and switch to it.
```

### Q24: How should the system handle the help text for the `/context` command and its subcommands? What information should be included to help users understand how to use the feature?

**A24:** The system should add information about the context management feature to the existing `/help` command output. The help text should include:

```
<em>/context</em>      <black!>Manage context files for the chat session</black!>
  <em>show</em>        <black!>Display current context configuration</black!>
  <em>add</em>         <black!>Add file(s) to context [--global]</black!>
  <em>rm</em>          <black!>Remove file(s) from context [--global]</black!>
  <em>alias</em>       <black!>List, create [--create], or delete [--delete] context aliases</black!>
  <em>switch</em>      <black!>Switch to a different context alias [--create]</black!>
  <em>clear</em>       <black!>Clear all files from current context [--global]</black!>
```

This help text:
1. Provides a concise overview of the `/context` command and its subcommands
2. Includes all available options and flags
3. Uses consistent formatting with the existing help text
4. Is organized in a logical way that makes it easy to understand the available commands
5. Integrates seamlessly with the existing `/help` command
