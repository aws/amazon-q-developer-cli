# Custom Commands

The custom commands feature allows you to create and execute reusable command templates that can be shared across your team (project commands) or used personally across all projects (user commands). Commands provide a simple way to standardize common workflows and prompts.

> Note: This is a beta feature that must be enabled before use.

## Getting Started

### Enable the Custom Commands Feature

The custom commands feature is experimental and disabled by default. Enable it with:

```bash
q settings chat.enableCommands true
```

## Basic Usage

Once enabled, you can create and execute custom commands:

```bash
/commands add code-review
/project:code-review
/project:clippy
```

## Command Types

### Project Commands

Commands stored in your repository and shared with your team.

- **Location**: `.amazonq/commands/`
- **Prefix**: `/project:`
- **Scope**: Repository-specific, version controlled
- **Use Case**: Team workflows, project-specific standards

Example:
```bash
/commands add optimize
/project:optimize
```

### User Commands

Commands available across all your projects.

- **Location**: `~/.amazonq/commands/`
- **Prefix**: `/user:`
- **Scope**: Personal, global across all projects
- **Use Case**: Personal workflows, cross-project utilities

Example:
```bash
/user:security-review
/user:frontend:component
```

## Commands

### `/commands add <name>`

Create a new project command with the specified name. This will:

1. Create the `.amazonq/commands/` directory if it doesn't exist
2. Generate a markdown template file named `<name>.md`
3. Open your default editor to customize the command
4. Cache the command for immediate use

```bash
/commands add git-helper
/commands add code-review
/commands add test-runner
```

**Command Name Rules:**
- Cannot be empty
- Cannot contain spaces or path separators (`/`, `\`)
- Cannot use reserved names: `help`, `add`, `remove`, `show`, `list`
- Use hyphens or underscores for multi-word names

### `/commands show [--scope project|global]`

Display all available commands with detailed information including creation dates, file paths, and content previews.

```bash
/commands show                    # Show all commands
/commands show --scope project    # Show only project commands
/commands show --scope global     # Show only user/global commands
```

The output includes:
- Command name and scope (project/global)
- File path and last modified date
- Content preview (first few lines)
- Usage examples with proper syntax

### `/commands remove <name> [--scope project|global]`

Remove commands from your command library. Includes safety confirmation prompts.

```bash
/commands remove git-helper                # Remove from project scope
/commands remove code-review --scope global # Remove from user scope
```

**Safety Features:**
- Confirmation prompt before deletion
- Clear indication of which file will be deleted
- Scope specification to avoid accidental deletions

### `/commands update <name> [--scope project|global]`

Edit existing commands using your default editor. The command will be reloaded automatically after editing.

```bash
/commands update git-helper                # Update project command
/commands update code-review --scope global # Update user command
```

**Features:**
- Opens existing command in your preferred editor
- Automatic cache refresh after editing
- Scope resolution (project commands take precedence)

### `/commands clear [--scope project|global]`

Remove multiple commands at once with batch processing capabilities.

```bash
/commands clear                    # Clear all commands (with confirmation)
/commands clear --scope project    # Clear only project commands
/commands clear --scope global     # Clear only user commands
```

**Safety Features:**
- Interactive confirmation with detailed impact summary
- Batch processing with progress indication
- Scope-specific clearing to prevent accidental data loss

## Command Execution

### Basic Execution

Execute commands using their respective prefixes:

```bash
/project:<name>              # Execute project command
/user:<name>                 # Execute user/global command
```

**Execution Flow:**
1. Command is located in the appropriate scope (project takes precedence)
2. Command content is loaded and processed
3. Final content is sent to Amazon Q for processing

### Namespaced Commands

Organize commands in subdirectories for better organization and team collaboration:

**Structure**:
```
.amazonq/commands/
├── frontend/
│   ├── component.md
│   ├── styling.md
│   └── testing.md
├── backend/
│   ├── api.md
│   ├── database.md
│   └── security.md
└── devops/
    ├── deployment.md
    └── monitoring.md
```

**Execution Examples**:
```bash
# Frontend commands
/project:frontend:component
/project:frontend:styling
/project:frontend:testing

# Backend commands  
/project:backend:api
/project:backend:database
/project:backend:security

# DevOps commands
/project:devops:deployment
/project:devops:monitoring
```

**User Command Namespacing**:
```bash
# Personal workflow commands
/user:review:security
/user:review:performance
/user:tools:format
/user:tools:lint
```

**Benefits of Namespacing**:
- **Organization**: Group related commands logically
- **Team Collaboration**: Clear ownership and responsibility
- **Discoverability**: Easier to find relevant commands
- **Scalability**: Manage large command libraries effectively

## Command Template Structure

When you create a new command, a structured template is generated:

```markdown
# Command Name

Brief description of what this command does.

## Instructions

Provide detailed instructions for Amazon Q:

1. Step 1: What to analyze first
2. Step 2: What to look for
3. Step 3: How to format the response

## Context

Any additional context or requirements for this command.

## Examples

Provide examples of how this command should be used or what output is expected.
```

## Execution Patterns

### Command Resolution Order

When executing commands, the system follows this resolution order:

1. **Project Commands**: Commands in `.amazonq/commands/` (current repository)
2. **User Commands**: Commands in `~/.amazonq/commands/` (global user commands)

**Example**:
```bash
# If both project and user commands exist with same name:
/project:<name>     # Explicitly uses project version
/user:<name>        # Explicitly uses user version
```

### Common Execution Patterns

**Code Review Workflow**:
```bash
/project:review
/project:security-scan
/project:performance-check
```

**Testing and Quality Assurance**:
```bash
/project:test
/project:lint
/project:format-check
```

**Documentation and Analysis**:
```bash
/project:document
/project:analyze
/project:explain
```

**Development Workflows**:
```bash
/user:setup:environment
/user:debug:performance
/user:refactor:patterns
```

### Error Handling

**Command Not Found**:
```bash
/project:nonexistent
# Output: ❌ Command 'nonexistent' not found in project scope.
#         Use '/commands add nonexistent' to create it.
```

**Security Violations**:
```bash
# If command contains dangerous patterns
# Output: 🔒 Security violation: Command contains potentially dangerous pattern: rm -rf
```

**Feature Disabled**:
```bash
# If custom commands are disabled
# Output: ❌ Commands tool is disabled. Enable it with: q settings chat.enableCommands true
```

### Execution Indicators

When executing commands, you'll see clear indicators:

```bash
/project:code-review
# Output: 🚀 Executing command: project:code-review
#         [Command content is processed and sent to Amazon Q]
```

## Advanced Features

### Security Validation

Commands are automatically validated for potentially dangerous patterns:

**Blocked Patterns:**
- `rm -rf` (destructive file operations)
- `sudo` (privilege escalation)
- `chmod 777` or `chmod +x` (permission changes)
- `../../../` (path traversal)
- `curl -s`, `wget` (network access)
- `nc` (netcat)

If a command contains these patterns, execution will be blocked with a security error.

> **Note**: The custom commands feature uses the built-in `commands` tool. For more information about tool permissions and configuration, see the [Built-in Tools documentation](./built-in-tools.md#commands-tool).

### Editor Integration

Commands automatically open in your preferred editor:

1. **Environment Variables**: Uses `$EDITOR` or `$VISUAL`
2. **Platform Defaults**:
   - **macOS**: `open -t` (opens in default text editor)
   - **Linux**: `nano`
   - **Windows**: `notepad`

### Caching System

Commands are cached in memory for improved performance:
- Commands are loaded on first use
- Cache persists during the chat session
- File changes require restarting the session to take effect

## Best Practices

### Command Organization

- **Use descriptive names**: `code-review` instead of `review`
- **Group related commands**: Use subdirectories for namespacing
- **Keep commands focused**: One specific task per command
- **Document thoroughly**: Include clear instructions and examples

### Effective Command Design

- **Be specific**: Provide detailed, actionable instructions
- **Include context**: Reference relevant project information
- **Structure output**: Specify desired response format
- **Use clear language**: Write instructions that are easy to understand

### Team Collaboration

- **Version control**: Commit `.amazonq/commands/` to your repository
- **Document commands**: Maintain a team README for command usage
- **Standardize naming**: Establish team conventions for command names
- **Review regularly**: Update commands as workflows evolve

## Example Commands

### Code Review Command

```markdown
# Code Review

Perform a comprehensive code review focusing on:

1. **Code Quality**: Check for code smells and anti-patterns
2. **Security**: Look for vulnerabilities and hardcoded secrets  
3. **Performance**: Identify bottlenecks and optimization opportunities
4. **Testing**: Evaluate test coverage and quality

Provide specific recommendations with file paths and line numbers.
```

### Clippy Runner Command

```markdown
# Clippy Runner

Run the complete clippy validation pipeline:

1. `cargo clippy --locked --workspace --color always -- -D warnings`
2. `cargo clippy --locked -p chat_cli --color always -- -D warnings`
3. `cargo test --locked -p chat_cli`
4. `cargo +nightly fmt --check -- --color always`

Fix any errors found and ensure all checks pass.
```

## Limitations

### File System Constraints

- Commands must be valid markdown files with `.md` extension
- File names determine command names (without extension)
- Directory structure affects namespacing

### Security Restrictions

- Certain command patterns are blocked for security
- No execution of arbitrary system commands
- Commands are processed as text prompts only

### Performance Considerations

- Command cache is session-specific
- File changes require session restart to take effect
- Large command files may impact performance

### Feature Limitations

- Command cache is session-specific (file changes require session restart)
- No command versioning or history tracking
- No automatic command synchronization across team members

## Troubleshooting

### Commands Not Found

If your command isn't recognized:

1. **Check file location**: Ensure the `.md` file is in the correct directory
2. **Verify naming**: Command name should match filename (without `.md`)
3. **Restart session**: File changes require restarting the chat session
4. **Check permissions**: Ensure files are readable

### Feature Disabled Error

If you see "Commands tool is disabled":

1. **Enable feature**: Run `q settings chat.enableCommands true`
2. **Restart session**: Settings changes may require restarting
3. **Verify setting**: Check that the setting was applied correctly

### Editor Issues

If the editor doesn't open when creating commands:

1. **Set environment variable**: Export `EDITOR` or `VISUAL`
2. **Check editor path**: Ensure the editor is in your PATH
3. **Try different editor**: Test with a simple editor like `nano`
4. **Platform-specific**: On macOS, ensure text editor associations are set

### Security Validation Errors

If commands are blocked by security validation:

1. **Review content**: Check for blocked patterns listed above
2. **Modify approach**: Use alternative phrasing or methods
3. **Contact support**: If legitimate use case is blocked

## Troubleshooting

### Commands Not Found

If your command isn't recognized:

1. **Check file location**: Ensure the `.md` file is in the correct directory
   - Project commands: `.amazonq/commands/`
   - User commands: `~/.amazonq/commands/`
2. **Verify naming**: Command name should match filename (without `.md`)
3. **Restart session**: File changes require restarting the chat session
4. **Check permissions**: Ensure files are readable

### Feature Disabled Error

If you see "Commands tool is disabled":

1. **Enable feature**: Run `q settings chat.enableCommands true`
2. **Restart session**: Settings changes may require restarting
3. **Verify setting**: Check that the setting was applied correctly

### Editor Issues

If the editor doesn't open when creating commands:

1. **Set environment variable**: Export `EDITOR` or `VISUAL`
2. **Check editor path**: Ensure the editor is in your PATH
3. **Try different editor**: Test with a simple editor like `nano`
4. **Platform-specific**: On macOS, ensure text editor associations are set

### Security Validation Errors

If commands are blocked by security validation:

1. **Review content**: Check for blocked patterns listed above
2. **Modify approach**: Use alternative phrasing or methods
3. **Contact support**: If legitimate use case is blocked

### Command Execution Issues

If commands don't execute as expected:

1. **Check syntax**: Ensure proper `/project:` or `/user:` prefix
2. **Verify scope**: Make sure command exists in the specified scope
3. **Check cache**: Restart session if recent file changes aren't reflected
4. **Review content**: Ensure command file contains valid markdown

### Performance Issues

If commands are slow to load or execute:

1. **File size**: Large command files may impact performance
2. **Cache warming**: First execution may be slower due to caching
3. **Directory structure**: Deeply nested namespaces may affect performance

## Quick Reference

### Management Commands
```bash
/commands add <name>                    # Create new command
/commands show [--scope project|global] # List commands
/commands remove <name> [--scope]       # Delete command
/commands update <name> [--scope]       # Edit command
/commands clear [--scope]               # Delete multiple commands
```

### Execution Commands
```bash
/project:<name>                         # Execute project command
/user:<name>                            # Execute user command
/user:<namespace>:<name>                # Execute namespaced user command
```

### Command Features
- **Namespacing**: Organize commands in subdirectories
- **Scoping**: Project commands override user commands
- **Security**: Automatic validation of dangerous patterns

### File Locations
- **Project Commands**: `.amazonq/commands/` (version controlled)
- **User Commands**: `~/.amazonq/commands/` (personal, global)
