# Custom Commands

The custom commands feature allows you to create and execute reusable command templates that can be shared across your team (project commands) or used personally across all projects (user commands). Commands support dynamic arguments, file references, and provide a powerful way to standardize common workflows.

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
/project:optimize "database queries"
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
/user:frontend:component "Button"
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
/project:command-name              # Execute project command
/user:command-name                 # Execute user/global command
```

**Execution Flow:**
1. Command is located in the appropriate scope (project takes precedence)
2. Command content is loaded and processed
3. Arguments and file references are substituted
4. Final content is sent to Amazon Q for processing

### Arguments Support

Commands can accept arguments that are dynamically substituted at execution time using the `$ARGUMENTS` placeholder.

**Command Definition** (in `optimize.md`):
```markdown
# Performance Optimization

Analyze the following code for performance issues: $ARGUMENTS

Please focus on:
1. Algorithm efficiency
2. Memory usage
3. Database query optimization
```

**Execution Examples:**
```bash
/project:optimize "the user authentication module"
/project:optimize src/database.rs
/project:optimize "functions in utils.py that handle file I/O"
```

**Argument Processing:**
- Arguments are passed as a single string after the command name
- Use quotes for multi-word arguments: `/project:review "authentication logic"`
- Arguments can reference files, functions, or any descriptive text
- The `$ARGUMENTS` placeholder is replaced with the exact text provided

### Advanced Argument Usage

**Multiple Argument References:**
```markdown
# Code Review Template

Please review $ARGUMENTS for the following aspects:

1. **Security**: Look for vulnerabilities in $ARGUMENTS
2. **Performance**: Analyze efficiency of $ARGUMENTS
3. **Best Practices**: Check if $ARGUMENTS follows coding standards
```

**Conditional Arguments:**
```markdown
# Testing Command

Run comprehensive tests for: $ARGUMENTS

If no specific component is mentioned, run the full test suite.
```

**Usage:**
```bash
/project:test "authentication module"     # Tests specific module
/project:test                            # Runs full test suite
```
```

**Usage**:
```bash
/project:optimize "the user authentication module"
```

The `$ARGUMENTS` placeholder will be replaced with "the user authentication module".

### File References

Include file contents in commands using the `@filename` syntax:

**Command Definition**:
```markdown
# Code Review

Please review the implementation in @src/main.rs and compare it with @docs/architecture.md

Focus on adherence to the documented architecture.
```

**Execution**:
When executed, `@src/main.rs` and `@docs/architecture.md` will be replaced with the actual file contents.

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
/project:frontend:component "Button with loading state"
/project:frontend:styling "responsive navigation bar"
/project:frontend:testing "user authentication flow"

# Backend commands  
/project:backend:api "user registration endpoint"
/project:backend:database "optimize user queries"
/project:backend:security "JWT token validation"

# DevOps commands
/project:devops:deployment "staging environment setup"
/project:devops:monitoring "API response time alerts"
```

**User Command Namespacing**:
```bash
# Personal workflow commands
/user:review:security "authentication implementation"
/user:review:performance "database query optimization"
/user:tools:format "TypeScript configuration files"
/user:tools:lint "React component structure"
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
/project:deploy     # Explicitly uses project version
/user:deploy        # Explicitly uses user version
```

### Common Execution Patterns

**Code Review Workflow**:
```bash
/project:review "authentication module"
/project:review @src/auth.rs
/project:review "changes in PR #123"
```

**Testing and Quality Assurance**:
```bash
/project:test "user registration flow"
/project:lint "TypeScript configuration"
/project:security-scan "API endpoints"
```

**Documentation and Analysis**:
```bash
/project:document "database schema changes"
/project:analyze "performance bottlenecks in @src/database.rs"
/project:explain "OAuth implementation approach"
```

**Development Workflows**:
```bash
/user:setup:environment "React TypeScript project"
/user:debug:performance "slow API responses"
/user:refactor:patterns "extract common utilities"
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
/project:code-review "authentication logic"
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
- **Use placeholders**: Leverage `$ARGUMENTS` for flexibility
- **Include context**: Reference relevant files with `@filename`
- **Structure output**: Specify desired response format

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

Please analyze: $ARGUMENTS

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
- File references are read-only

### Performance Considerations

- Large files referenced with `@filename` may impact performance
- Command cache is session-specific
- File changes require session restart to take effect

### Feature Limitations

- Command cache is session-specific (file changes require session restart)
- No command versioning or history tracking
- File references are read-only (cannot modify files through commands)
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
3. **File references**: Use `@filename` instead of direct commands
4. **Contact support**: If legitimate use case is blocked

### File Reference Issues

If `@filename` references aren't working:

1. **Check file paths**: Ensure referenced files exist and are accessible
2. **Use relative paths**: Paths are relative to current working directory
3. **Verify permissions**: Ensure files are readable
4. **Test manually**: Try reading the file directly to confirm access

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
/project:<name> [arguments]             # Execute project command
/user:<name> [arguments]                # Execute user command
/user:<namespace>:<name> [arguments]    # Execute namespaced user command
```

### Command Features
- **Arguments**: Use `$ARGUMENTS` placeholder in command content
- **File References**: Use `@filename` to include file contents
- **Namespacing**: Organize commands in subdirectories
- **Scoping**: Project commands override user commands
- **Security**: Automatic validation of dangerous patterns

### File Locations
- **Project Commands**: `.amazonq/commands/` (version controlled)
- **User Commands**: `~/.amazonq/commands/` (personal, global)
