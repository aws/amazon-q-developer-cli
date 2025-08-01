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

## Command Execution

### Basic Execution

Execute commands using their respective prefixes:

```bash
/project:command-name
/user:command-name
```

### Arguments Support

Commands can accept arguments using the `$ARGUMENTS` placeholder:

**Command Definition** (in `optimize.md`):
```markdown
# Performance Optimization

Analyze the following code for performance issues: $ARGUMENTS

Please focus on:
1. Algorithm efficiency
2. Memory usage
3. Database query optimization
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

Organize commands in subdirectories for better organization:

**Structure**:
```
.amazonq/commands/
├── frontend/
│   ├── component.md
│   └── styling.md
└── backend/
    ├── api.md
    └── database.md
```

**Usage**:
```bash
/project:frontend:component
/project:backend:api
```

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

- No command deletion through CLI (manual file deletion required)
- No command listing or search functionality
- No command versioning or history

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
