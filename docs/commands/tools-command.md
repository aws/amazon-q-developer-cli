# Tools Command

## Overview
The tools command allows users to view and manage tool permissions in Amazon Q, controlling which tools require confirmation before use.

## Command Details
- **Name**: `tools`
- **Description**: View and manage tools and permissions
- **Usage**: `/tools [subcommand]`
- **Requires Confirmation**: Only for trustall operations

## Subcommands

### List (Default)
- **Usage**: `/tools` or `/tools list`
- **Description**: Lists all available tools and their trust status
- **Example**:
  ```
  /tools list
  ```

### Trust
- **Usage**: `/tools trust <tool_name> [tool_name2...]`
- **Description**: Trusts specific tools so they don't require confirmation for each use
- **Example**:
  ```
  /tools trust fs_write execute_bash
  ```

### Untrust
- **Usage**: `/tools untrust <tool_name> [tool_name2...]`
- **Description**: Reverts tools to require confirmation for each use
- **Example**:
  ```
  /tools untrust execute_bash
  ```

### Trustall
- **Usage**: `/tools trustall`
- **Description**: Trusts all tools for the session
- **Requires Confirmation**: Yes
- **Example**:
  ```
  /tools trustall
  ```

### Reset
- **Usage**: `/tools reset`
- **Description**: Resets all tool permissions to default settings
- **Example**:
  ```
  /tools reset
  ```

### Reset Single
- **Usage**: `/tools reset <tool_name>`
- **Description**: Resets a specific tool's permissions to default
- **Example**:
  ```
  /tools reset fs_write
  ```

### Help
- **Usage**: `/tools help`
- **Description**: Shows help information for the tools command
- **Example**:
  ```
  /tools help
  ```

## Functionality
Tools allow Amazon Q to perform actions on your system, such as executing commands or modifying files. By default, you will be prompted for permission before any tool is used. The tools command lets you manage which tools require confirmation and which are trusted for the duration of your session.

## Example Usage
```
/tools list
```

Output:
```
Available tools:
✓ fs_read (trusted)
✓ fs_write (trusted)
! execute_bash (requires confirmation)
! use_aws (requires confirmation)
```

## Related Commands
- `/acceptall`: Deprecated command, use `/tools trustall` instead

## Use Cases
- Viewing which tools are available and their trust status
- Trusting specific tools for repetitive operations
- Requiring confirmation for potentially destructive tools
- Resetting tool permissions after changing them

## Notes
- Tool permissions are only valid for the current session
- Trusted tools will not require confirmation each time they're used
- The trustall command requires confirmation as a safety measure
- You can trust or untrust multiple tools in a single command
