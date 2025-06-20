# Trusted Commands

The Trusted Commands feature allows you to define shell commands that can be executed by Amazon Q Developer CLI without requiring explicit confirmation each time.

## Overview

By default, Amazon Q Developer CLI requires your confirmation before executing potentially modifying commands (like `npm install`, `git commit`, etc.) to ensure safety. However, you may have certain commands that you frequently use and trust, and would prefer to execute without confirmation.

The Trusted Commands feature lets you define these commands in a configuration file, so Amazon Q can execute them directly when you ask.

## Configuration

### Configuration File Location

Create a JSON configuration file at:

```
~/.aws/amazonq/trusted_commands.json
```

### Configuration Format

The configuration file should have the following structure:

```json
{
  "trusted_commands": [
    {
      "type": "match",
      "command": "npm*",
      "description": "All npm commands"
    },
    {
      "type": "regex",
      "command": "^git (status|log|diff)",
      "description": "Git read-only commands"
    }
  ]
}
```

### Command Types

There are two ways to define trusted commands:

1. **Match Pattern** (`"type": "match"`): Uses a glob-style pattern with `*` as a wildcard
   - Example: `"npm*"` matches all commands that start with "npm"
   - Example: `"*install"` matches all commands that end with "install"
   - Example: `"git*commit*"` matches commands that contain "git", followed by anything, followed by "commit", followed by anything
   - Multiple wildcards are fully supported, allowing for complex pattern matching
   - **Important**: Unlike shell globs, spaces are treated as literal characters, so `"git * commit"` will only match if there are spaces around the wildcard

2. **Regex Pattern** (`"type": "regex"`): Uses a regular expression for more complex matching
   - Example: `"^git\\s+(status|log|diff)"` matches git status, git log, and git diff commands
   - Note: Regex patterns must be valid and properly escaped in JSON

### Description Field

The `"description"` field is optional but recommended for documentation purposes. It helps you remember why you added a particular command to your trusted list.

## Security Considerations

### Safety Measures

Even if a command matches your trusted patterns, Amazon Q will still require confirmation if the command contains potentially dangerous patterns such as:

- Redirections (`>`, `>>`)
- Command substitutions (`$(...)`, backticks)
- Command chaining (`&&`, `||`, `;`)
- Background execution (`&`)

This ensures that potentially destructive operations always require your explicit approval.

### File Permissions

For security reasons, you should ensure that your `trusted_commands.json` file has secure permissions:

- On Unix/Linux/macOS: Set permissions to 600 (`chmod 600 ~/.aws/amazonq/trusted_commands.json`)
- On Windows: Ensure the file is only accessible by your user account

If the file has insecure permissions, Amazon Q will still load it but will display a warning.

## Examples

### Basic Examples

```json
{
  "trusted_commands": [
    {
      "type": "match",
      "command": "npm*",
      "description": "All npm commands"
    },
    {
      "type": "match",
      "command": "git push*",
      "description": "Git push command"
    }
  ]
}
```

### Advanced Examples

```json
{
  "trusted_commands": [
    {
      "type": "match",
      "command": "npm *",
      "description": "All npm commands"
    },
    {
      "type": "regex",
      "command": "^git\\s+(status|log|diff|pull)",
      "description": "Common git commands"
    },
    {
      "type": "regex",
      "command": "^docker\\s+ps(\\s+(-a|--all))?$",
      "description": "Docker ps commands"
    }
  ]
}
```

## Implementation Details

### Pattern Evaluation
- **Match Patterns**: Evaluated using Rust's `glob` crate, which provides full glob pattern matching with support for multiple wildcards
- **Regex Patterns**: Compiled using Rust's `regex` crate when the configuration is loaded and evaluated against commands using the `is_match` method

### Caching
The trusted commands configuration is cached in memory for 5 minutes (300 seconds) to improve performance. This means that changes to the configuration file may take up to 5 minutes to take effect.

## Troubleshooting

If your trusted commands aren't working as expected:

1. **Check for syntax errors**: Ensure your JSON is valid and regex patterns are properly escaped
2. **Check file permissions**: Make sure the configuration file is readable by your user
3. **Check for dangerous patterns**: Commands with redirections, substitutions, etc. will always require confirmation
4. **Cache timing**: Changes to the configuration file may take up to 5 minutes to take effect due to caching

## Best Practices

1. **Start conservatively**: Begin with a small set of trusted commands and expand as needed
2. **Use specific patterns**: Prefer specific patterns over overly broad ones
3. **Avoid trusting destructive commands**: Be cautious about trusting commands that delete or modify important files
4. **Document your patterns**: Always include a description to remind yourself why you trusted a command
5. **Review periodically**: Regularly review your trusted commands list to ensure it still matches your needs
6. **Think twice before trusting all execute_bash commands**: Hitting `t` after an execute_bash commands proposition will let Amazon Q Developer CLI executes any commands with no confirmation.