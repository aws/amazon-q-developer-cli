# Profile Command

## Overview
The profile command allows users to manage different profiles for organizing context files and settings in Amazon Q.

## Command Details
- **Name**: `profile`
- **Description**: Manage profiles
- **Usage**: `/profile [subcommand]`
- **Requires Confirmation**: Only for delete operations

## Subcommands

### List
- **Usage**: `/profile list`
- **Description**: Lists all available profiles
- **Example**:
  ```
  /profile list
  ```

### Create
- **Usage**: `/profile create <profile_name>`
- **Description**: Creates a new profile with the specified name
- **Example**:
  ```
  /profile create work
  ```

### Delete
- **Usage**: `/profile delete <profile_name>`
- **Description**: Deletes the specified profile
- **Requires Confirmation**: Yes
- **Example**:
  ```
  /profile delete test
  ```

### Set
- **Usage**: `/profile set <profile_name>`
- **Description**: Switches to the specified profile
- **Example**:
  ```
  /profile set personal
  ```

### Rename
- **Usage**: `/profile rename <old_profile_name> <new_profile_name>`
- **Description**: Renames an existing profile
- **Example**:
  ```
  /profile rename work job
  ```

### Help
- **Usage**: `/profile help`
- **Description**: Shows help information for the profile command
- **Example**:
  ```
  /profile help
  ```

## Functionality
Profiles allow you to organize and manage different sets of context files for different projects or tasks. Each profile maintains its own set of context files, allowing you to switch between different contexts easily.

The "global" profile contains context files that are available in all profiles, while the "default" profile is used when no profile is specified.

## Example Usage
```
/profile list
```

Output:
```
Available profiles:
* default
  work
  personal
```

## Related Commands
- `/context`: Manage context files within the current profile
- `/context add --global`: Add context files to the global profile

## Use Cases
- Creating separate profiles for different projects
- Switching between work and personal contexts
- Organizing context files for different clients or tasks
- Managing different sets of context hooks

## Notes
- Profile settings are preserved between chat sessions
- The global profile's context files are available in all profiles
- Deleting a profile removes all associated context files and settings
- You cannot delete the default profile
