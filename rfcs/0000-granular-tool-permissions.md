- Feature Name: granular_tool_permissions
- Start Date: 2025-03-23

# Summary

Replace the binary `/acceptall` toggle with a more nuanced permission system that allows users to set permissions on a per-tool basis, trust specific tools for the session duration, and still approve individual actions when desired. This provides users with finer control over which tools can execute without confirmation while maintaining security for more sensitive operations.

# Motivation

The current `/acceptall` command is an all-or-nothing approach that either requires confirmation for every tool execution or allows all tools to run without any confirmation. This presents several issues:

1. Users who want to streamline their workflow with frequently used safe tools (like `fs_read`) must also accept the risk of automatically approving potentially destructive operations (like `fs_write` or `execute_bash`).

2. The current approach doesn't allow for nuanced trust decisions based on the sensitivity of different tools.

3. Users are forced to choose between convenience (no prompts) and security (all prompts), with no middle ground.

By implementing granular tool permissions, we can provide users with a better balance between security and convenience, allowing them to make informed decisions about which tools they trust while maintaining safeguards for more sensitive operations.

# Guide-level explanation

## Basic Usage

Users can now control permissions for individual tools using the new `/tools` command family:

```
/tools                   Show current tool permission settings
/tools trust <tool>      Trust a specific tool for the session
/tools untrust <tool>    Revert a tool to per-request confirmation
/tools trustall          Trust all tools (equivalent to old /acceptall)
/tools reset             Reset all tools to default permission levels
```

### Viewing Current Permissions

Users can see which tools are trusted and which require confirmation:

```
> /tools
Current tool permissions:
- fs_read: Trusted
- fs_write: Per-request
- execute_bash: Per-request
- use_aws: Per-request
```

### Trusting Individual Tools

Users can trust specific tools they use frequently or consider safe:

```
> /tools trust fs_read
Tool 'fs_read' is now trusted for this session. Amazon Q can read files without confirmation.
```

### Interactive Trust Decision

When Amazon Q attempts to use a tool that requires confirmation, users now have three options:

```
Amazon Q: I'll check the contents of your file.
[Tool Request: fs_read (path=/Users/user/config.json)]
Allow this action? [y/n/t]: _
```

Where:
- `y` = Allow this specific request
- `n` = Deny this specific request
- `t` = Trust this tool for the session (no more prompts for this tool)

This allows users to make a trust decision at the moment it's relevant, without having to use a separate command.

### Backward Compatibility

The existing `/acceptall` command is maintained as an alias for `/tools trustall` to ensure backward compatibility, though it will display a gentle deprecation notice suggesting the new commands.

## Security Considerations

- Tool permissions are session-based only and reset when the application is restarted
- Clear visual indicators show when a trusted tool is being used
- Users can easily reset all permissions to default with `/tools reset`

# Reference-level explanation

## Data Structures

We'll add a new structure to track tool permissions:

```rust
struct ToolPermission {
    trusted: bool,
}

struct ToolPermissions {
    permissions: HashMap<String, ToolPermission>,
}

impl ToolPermissions {
    fn new() -> Self {
        Self {
            permissions: HashMap::new(),
        }
    }
    
    fn is_trusted(&self, tool_name: &str) -> bool {
        self.permissions
            .get(tool_name)
            .map(|perm| perm.trusted)
            .unwrap_or(false)
    }
    
    fn trust_tool(&mut self, tool_name: &str) {
        self.permissions.insert(
            tool_name.to_string(), 
            ToolPermission { trusted: true }
        );
    }
    
    fn untrust_tool(&mut self, tool_name: &str) {
        self.permissions.insert(
            tool_name.to_string(), 
            ToolPermission { trusted: false }
        );
    }
    
    fn reset(&mut self) {
        self.permissions.clear();
    }
    
    fn trust_all(&mut self, tool_names: &[&str]) {
        for tool_name in tool_names {
            self.trust_tool(tool_name);
        }
    }
}
```

## Integration Points

### ChatSession Update

The `ChatSession` struct will be updated to include the tool permissions:

```rust
pub struct ChatSession<W: Write> {
    // existing fields...
    tool_permissions: ToolPermissions,
    // ...
}
```

### Tool Execution Flow

The tool execution flow will be modified to check permissions before prompting:

```rust
fn execute_tool(&mut self, tool_name: &str, params: &Value) -> Result<Value> {
    if self.tool_permissions.is_trusted(tool_name) {
        // Execute without confirmation
        self.output_trusted_tool_execution(tool_name, params)?;
        self.execute_tool_internal(tool_name, params)
    } else {
        // Ask for confirmation
        match self.prompt_for_tool_execution(tool_name, params)? {
            ToolPromptResponse::Yes => {
                self.execute_tool_internal(tool_name, params)
            }
            ToolPromptResponse::No => {
                Err(Error::ToolExecutionDenied)
            }
            ToolPromptResponse::Trust => {
                self.tool_permissions.trust_tool(tool_name);
                self.output_tool_now_trusted(tool_name)?;
                self.execute_tool_internal(tool_name, params)
            }
        }
    }
}
```

### Command Handlers

New command handlers will be added for the `/tools` commands:

```rust
fn handle_tools_command(&mut self, args: &str) -> Result<()> {
    let args = args.trim();
    
    if args.is_empty() {
        self.show_tool_permissions()?;
        return Ok(());
    }
    
    let parts: Vec<&str> = args.split_whitespace().collect();
    match parts[0] {
        "trust" if parts.len() > 1 => {
            self.tool_permissions.trust_tool(parts[1]);
            self.output_tool_now_trusted(parts[1])?;
        }
        "untrust" if parts.len() > 1 => {
            self.tool_permissions.untrust_tool(parts[1]);
            self.output_tool_now_untrusted(parts[1])?;
        }
        "trustall" => {
            self.tool_permissions.trust_all(&AVAILABLE_TOOLS);
            self.output_all_tools_trusted()?;
        }
        "reset" => {
            self.tool_permissions.reset();
            self.output_permissions_reset()?;
        }
        _ => {
            self.output_tools_help()?;
        }
    }
    
    Ok(())
}
```

### UI Updates

The UI will be enhanced to provide clear feedback about tool permissions:

```rust
fn output_trusted_tool_execution(&mut self, tool_name: &str, params: &Value) -> Result<()> {
    execute!(
        self.output,
        style::SetForegroundColor(Color::Yellow),
        style::Print(format!("[Tool Request: {} ({}) - Trusted]\n", tool_name, params)),
        style::Print("Executing...\n"),
        style::SetForegroundColor(Color::Reset)
    )?;
    
    Ok(())
}

fn prompt_for_tool_execution(&mut self, tool_name: &str, params: &Value) -> Result<ToolPromptResponse> {
    execute!(
        self.output,
        style::SetForegroundColor(Color::Yellow),
        style::Print(format!("[Tool Request: {} ({})]\n", tool_name, params)),
        style::Print("Allow this action? [y/n/t]: "),
        style::SetForegroundColor(Color::Reset)
    )?;
    
    // Read user input and return appropriate response
    // ...
}
```

## Backward Compatibility

The existing `/acceptall` command will be maintained as an alias:

```rust
fn handle_command(&mut self, command: &str) -> Result<bool> {
    match command {
        "/acceptall" => {
            self.handle_tools_command("trustall")?;
            self.output_acceptall_deprecation_notice()?;
            Ok(true)
        }
        "/tools" | "/tools help" => {
            self.output_tools_help()?;
            Ok(true)
        }
        cmd if cmd.starts_with("/tools ") => {
            self.handle_tools_command(&cmd[7..])?;
            Ok(true)
        }
        // Other commands...
    }
}
```

# Drawbacks

1. **Increased Complexity**: Adding granular permissions increases the complexity of the codebase and the user interface.

2. **Learning Curve**: Users familiar with the simple `/acceptall` toggle will need to learn the new command structure.

3. **UI Clutter**: Additional prompts and options could make the interface more cluttered.

4. **Implementation Effort**: This feature requires significant changes to the tool execution flow and UI.

# Rationale and alternatives

## Why this design?

This design was chosen because it:

1. Provides a balance between security and convenience
2. Maintains backward compatibility with the existing `/acceptall` command
3. Introduces an intuitive interactive prompt that allows users to make trust decisions in context
4. Keeps permissions session-based, avoiding persistent security risks

## Alternatives considered

### 1. Tool Categories

Instead of per-tool permissions, we could group tools into categories like "safe" (e.g., `fs_read`), "modify" (e.g., `fs_write`), and "execute" (e.g., `execute_bash`). Users could then trust entire categories.

This was rejected because:
- It's less flexible than per-tool permissions
- Tool categorization is subjective and may not align with all users' needs
- It adds another conceptual layer for users to understand

### 2. Persistent Permissions

We could store tool permissions in user profiles, making them persistent across sessions.

This was rejected because:
- It introduces long-term security risks if users forget which tools they've trusted
- Session-based permissions provide a good balance between convenience and security
- It would require additional storage and management code

### 3. Do Nothing

We could keep the current `/acceptall` toggle without adding granular permissions.

Impact of not doing this:
- Users would continue to face the all-or-nothing choice between security and convenience
- We would miss an opportunity to improve user experience and security

# Unresolved questions

1. **Tool Discovery**: How will users discover which tools are available to trust? Should we provide a list of all available tools?

2. **Permission Grouping**: Should we provide a way to trust/untrust multiple tools at once based on common patterns?

3. **Visual Indicators**: What's the best way to visually indicate trusted vs. untrusted tools in the UI?

4. **Metrics**: Should we collect anonymous usage metrics to understand how users are using this feature?

# Future possibilities

1. **Blocked Tools**: Add a "blocked" permission level that prevents specific tools from being used at all.

2. **Permission Presets**: Allow users to save and load permission presets for different workflows.

3. **Parameter-Level Permissions**: Allow users to trust tools only for specific parameter patterns (e.g., trust `fs_read` only for certain directories).

4. **Time-Limited Trust**: Allow users to trust tools for a limited time or number of uses.

5. **Integration with Profiles**: Allow saving trusted tools as part of user profiles for persistent but explicit permissions.

6. **Tool Risk Levels**: Add visual indicators of the relative risk level of different tools to help users make informed decisions.
