- Feature Name: extra_granular_permissions
- Start Date: 2025-04-14

# Summary

[summary]: #summary

This RFC proposes extending the existing granular tool permissions system (proposed in https://github.com/aws/amazon-q-developer-cli/pull/921) to provide fine-grained control over tool execution. It introduces:

1. Path-based glob permissions for filesystem operations
2. Command matching for the `execute_bash` tool
3. Service/operation permissions for the `use_aws` tool
4. Interactive permission rule creation

Tool calls that don't match permission rules will require user confirmation.

# Motivation

[motivation]: #motivation

The current system forces users to either trust a tool completely or approve each use. This creates friction in common workflows where users want to:

- Trust filesystem operations only within specific project directories
- Trust certain shell commands but not others (e.g., allow `git status` but require confirmation for `git push`)
- Trust specific AWS operations (e.g., `aws s3 ls` but not other AWS commands)

User testing shows:
- "Prompt fatigue" from repeatedly approving the same operations
- Users trusting entire tools when they only need a subset of functionality
- Users wanting different permission levels for different projects

# Guide-level explanation

[guide-level-explanation]: #guide-level-explanation

## Core Permission Model

In addition to the current `/tools` command suite, the new permission model introduces three primary commands:

```
/tools allow <tool> [options]
/tools block <tool> [options]
/tools remove-rule <tool> [options]

options:
--path <path>                                # for path-based tools: allow or block a path
--command <command>                          # for command-based tools: allow or block any shell command
--service <service> --operation <operation>  # for use_aws: allow or block an aws service
```

These commands manage "rules" that control tool permissions:
- `allow` grants permission for specific paths, commands, or AWS operations
- `block` explicitly blocks specific paths, commands, or operations (overrides allow rules)
- `remove-rule` removes a rule from the allow and block lists

Aligned with existing functionality, tools can be completely trusted using `/tools trust`, which would be the equivalent of allowing all paths/commands. Tools can also be completely untrusted using `/tools untrust`, which would be the equivalent of blocking all paths/commands.

Note: Not all options apply to each tool. To make it clear to the user what sort of permissions they are granting, explicit and tool-specific options are necessary.

### Path-Based Permissions

Users can specify allowed and blocked paths for filesystem operations:

```
/tools allow fs_read --path /path/to/project /path/to/project2/**
Trusted 2 paths for 'fs_write'. I will **not** ask for confirmation before running this tool with these paths.

/tools allow fs_write --path /path/to/output/**
Trusted 1 path for 'fs_read'. I will **not** ask for confirmation before running this tool with that path.

/tools block fs_write --path /path/to/project/config/**
Blocked 1 path for 'fs_write'. I **will** ask for confirmation before running this tool with that path.
```

This allows Amazon Q to:
- Read from `/path/to/project` and `/path/to/project2` and all their subdirectories without confirmation
- Write to `/path/to/output` and all its subdirectories without confirmation
- Prompt to write to `/path/to/project/config` or its subdirectories (despite the broader allow rule existing)

Path patterns support standard glob syntax and are normalized to absolute paths when comparing. Direct paths to folders will include all subcontent (i.e. /my/folder == /my/folder/**)

### Command-Based Permissions

For the `execute_bash` tool, users can allow or block specific commands:

```
/tools allow execute_bash --command "git status" "rm"
Trusted 2 commands for 'execute_bash'. I will **not** ask for confirmation before running these commands.

/tools block execute_bash --command "rm -rf"
Blocked 1 command for 'execute_bash'. I **will** ask for confirmation before running this command.
```

This allows Amazon Q to:
- Run any `git status` or `rm` without confirmation
- Prompt to run `rm -rf` (even though we allowed `rm`)

Command patterns will match by prefix (i.e. `.startswith()`). For example `git` and `git status` will match `git status -s`.

Piped commands will be treated as separate commands. All commands sent to a single `execute_bash` call must pass the allow rules to avoid confirmation.


### AWS CLI Permissions

For the `use_aws` tool, users can allow or block specific service/operation combinations:

```
/tools allow use_aws --service s3 --operation "get*"
Trusted 1 command for 'use_aws'. I will **not** ask for confirmation before running this command.

/tools allow use_aws --service "*" --operation "describe*"
Trusted 1 command for 'use_aws'. I will **not** ask for confirmation before running this command.

/tools block use_aws --service iam --operation "*"
Blocked 1 command for 'use_aws'. I **will** ask for confirmation before running this command.
```

This allows Amazon Q to:
- Run any S3 `get` operations without confirmation
- Run any `describe` operations on any service without confirmation
- Prompt to run any IAM operations

This pattern match will be similar to glob style matching, allowing the use of "*" to as a wildcard for matching on services or operations. This is useful for AWS operations by enabling users to:
- Allow all read-only operations (`get*`, `describe*`, `list*`)
- Allow operations on specific services, or all services with "*"
- Block sensitive services or operations


## Other tools

### Built-In tools

Other tools that do not require further granularity such as `report_issue` continue to work with `/tools trust` and `/tools untrust`, and does not accept any other granular permission commands, e.g.
```
/tools allow report_issue --path /some/path/*

Error: 'report_issue' does not use path permissions. Use `/tools [trust/untrust] report_issue` to enabled/disable acceptance prompting.
```


### Custom tools from MCP

MCP tools are considered a black box and are given blanket trust/untrust permissions only. Like non-granular built-in tools, their permissions can only be controlled with the current implementation of `/tools trust` and `/tools untrust`. By default, these tools are marked as `Trusted`.


## Viewing Current Permissions

Users can see which tools are trusted (never prompt), untrusted (always prompt), or which tools have granular permission rules. This view displays the default permissions for tools (if they are not changed), which includes what commands and operations are considered "readonly" by `execute_bash` and `use_aws`.
Tools from MCP are also listed here, however they are marked as `Trusted`/`Untrusted` only.

```
> /tools

Current tools and permissions:
  fs_read:
    Trusted Paths
      ./*
      /users/me/documents/*

    Requires confirmation
      ./data/secrets.txt


  fs_write:
    Trusted Paths
      <none>

    Requires confirmation
      *


  execute_bash:
    Trusted Commands
      git status
      git push

    Requires confirmation
      git push -f


  use_aws:
    Trusted Services          Operations
      * (all)                  get*
      s3                       put*

    Requires confirmation
      iam                      * (all)

  report_issue: Trusted

  MCP Tools:
    - parse_markdown: Trusted
    - talk_to_other_ai: Per-request
    ...
```

To view permissions for a particular tool, the user can run:
```
/tools fs-write

Current permissions for `fs_write`:
  Trusted Paths
    <none>

  Requires confirmation
    *
```

Or to view permissions for tools from MCP, the user can run:
```
/tools --mcp

Current tools and permissions from MCP:
  - parse_markdown: Trusted
  - talk_to_other_ai: Per-request
```

### Default Permissions

Starting Q CLI with no permission adjustments will yield the following permissions for each built-in tool. They would display using the `/tools` command:
- fs_read
  - allow: *
  - block:
- fs_write
  - allow:
  - block: *
- execute_bash:
  - allow: ls, cat, echo... (all current readonly commands)
  - block:
- use_aws:
  - allow: (service=\*, operation="get\*,ls\*, ...") (all current readonly operations)
  - block:
- report_issue: Trusted


## Rule Removal

Users can run the existing `/tools reset <optional tool name>` command to reset tools to their default permission levels. Alternatively, the `remove-rule` command to remove to remove patterns from the rules. Removing a rule will remove it from both allowed and block lists. `/tools untrust` and `/tools trust` will also remove any permissions set such that everything is either allowed or blocked.

```
> /tools fs-write

Current permissions for `fs_write`:
  Trusted Paths
    /my/path/

  Requires confirmation
    /my/path/

> /tools remove-rule fs_write --path /my/path/

Rule removed.

> /tools fs-write

Current permissions for `fs_write`:
  Trusted Paths
    <none>

  Requires confirmation
    * (all)
```


## Interactive Rule Creation

When prompted for tool approval, users can create rules directly:

```
Amazon Q: I'll check the status of your git repository.
[Tool Request: execute_bash (command=git status -s)]
... tool details ...
Allow this action? Use 'c' to configure tool permission. [y/n/c]:

> c

Create rule for: execute_bash (command=git status -s)
Trusted commands do not ask for confirmation before running.

1. Trust this exact command only
2. Trust all 'git status' commands with any arguments
3. Trust all 'git' commands
4. Trust all requests from this tool 'execute_bash'
Or, 'y' to run without adding a rule:

> 2

Rule added: execute_bash --command="git status *"

Executing command...
```

For fs_write, this might look like:
```
Create rule for: fs_write (write=/users/me/my/project/test.txt)
Trusted paths do not ask for confirmation before writing.

1. Trust this exact path only
2. Trust the current directory (/users/me/)         # if the path in question is within the current directory, otherwise prompt for the parent of the target file
3. Trust all requests from this tool 'fs_write'
Or, 'y' to run without adding a rule:
```

For use_aws, this might look like:
```
Create rule for: use_aws (s3 list-buckets)
Trusted paths do not ask for confirmation before writing.

1. Trust this exact command only
2. Trust the all list* operations for s3
3. Trust the all list* operations for any service
4. Trust all requests from this tool 'use_aws'
Or, 'y' to run without adding a rule:
```

For MCP tools, the prompt would be what is currently available.
```
[Tool Request: parse_markdown]
... tool details ...
Allow this action? Use 't' to trust (always allow) this tool for the session. [y/n/t]:

> t

Executing command...
```

## Rule evaluation

1. If the tool is fully trusted, allow without prompting
2. Check for matching block patterns - if found, require confirmation
3. Check for matching allow patterns - if found, allow without prompting
4. If no patterns match, require confirmation


## Storage

Permission rules will be stored in memory for the current session only, consistent with the existing tool permissions system.


# Reference-level explanation

[reference-level-explanation]: #reference-level-explanation

<details>
<summary>Data Structure updates pseudo-code implementation</summary>

## Data Structures

We'll extend the existing `ToolPermission` structure to support pattern-based rules:

```rust
pub struct ToolPermission {
    trusted: bool,

    // New fields
    allowed_patterns: HashSet<String>,
    blocked_patterns: HashSet<String>,
}

pub struct ToolPermissions {
    permissions: HashMap<String, ToolPermission>,
}

pub struct AwsUse {
    service: String,
    operation: String,
}

pub enum ToolParams {
    Path(String),
    Command(String),
    AwsUse(AwsUse)
    Empty,
}

impl ToolPermissions {
    // ... existing implementation

    fn glob_match(&self, pattern: &str, value: &str) -> bool {
        // Implementation using the globset crate for efficient pattern matching
        let glob = globset::Glob::new(pattern)
            .map(|g| g.compile_matcher())
            .unwrap_or_else(|_| globset::GlobMatcher::new());
        
        glob.is_match(value)
    }

    fn aws_matches(pattern: &str, param: &AwsParam) {
        let args = pattern.split_whitespace();
        if args[0] != "*" && args[0] !== param.service{
            return false
        }
        if !self.glob_match(args[1], param.operation) {
            false
        }
        return true
    }

    fn requires_acceptance(&self, tool_name: &str, params: &ToolParams) -> bool {
        let permission = match self.permissions.get(tool_name) {
            Some(p) => p,
            None => return true, // No permission entry means we need acceptance
        };
        
        // If the entire tool is trusted, no acceptance needed
        if permission.trusted {
            return false;
        }

        match params {
            ToolParams::Path(path_param) => {
                // First check for explicit blocks
                for pattern in &permission.blocked_patterns {
                    if self.glob_match(pattern, path_param) {
                        // Match any blocked pattern to require acceptance
                        return true;
                    }
                }
                
                // Then check for allows
                for pattern in &permission.allowed_patterns {
                    if self.glob_match(pattern, path_param) {
                        // Must match any allow to skip acceptance
                        return false;
                    }
                }

                // Didn't match any allow patterns, need acceptance
                return true;
            },
            ToolParams::Command(command_param) => {
                // Gather piped commands
                let commands: Vec<String> = command_param
                    .split('|')
                    .map(|cmd| cmd.trim().to_string())
                    .collect();

                for cmd in &commands {
                    // First check for explicit blocks
                    for pattern in &permission.blocked_patterns {
                        if cmd.startswith(pattern) {
                            return true;
                        }
                    }
                    
                    // Then check for allows
                    let mut cmd_allowed = false;
                    for pattern in &permission.allowed_patterns {
                        if cmd.startswith(pattern) {
                            cmd_allowed = true;
                            break;
                        }
                    }
                    
                    // If any command in the pipe isn't allowed, require acceptance
                    if !cmd_allowed {
                        return true;
                    }
                }
                
                // All commands in the pipe are allowed
                return false;
            },
            ToolParams::AwsUse(aws_param) => {
                for pattern in &permission.blocked_patterns {
                    if self.aws_matches(pattern, aws_param) {
                        // Match any blocked pattern to require acceptance
                        return true;
                    }
                }
                
                for pattern in &permission.allowed_patterns {
                    if self.aws_matches(pattern, aws_param) {
                        // Must match any allow to skip acceptance
                        return false;
                    }
                }
                
                return true;
            },
            ToolParams::Empty => return true, // Nothing to check and the tool isn't trusted - require acceptance
        }
    }

    fn add_rule_pattern(&mut self, tool_name: &str, pattern: String, allow: bool) -> Result<()> {
        let permission = self.permissions.entry(tool_name.to_string())
            .or_insert_with(|| ToolPermission {
                trusted: false,
                allowed_patterns: HashSet::new(),
                blocked_patterns: HashSet::new(),
            });
            
        if allow {
            permission.allowed_patterns.insert(pattern);
        } else {
            permission.blocked_patterns.insert(pattern);
        }
        Ok(())
    }

    fn remove_rule_pattern(&mut self, tool_name: &str, pattern: &str) -> Result<()> {
        let permission = match self.permissions.get_mut(tool_name) {
            Some(p) => p,
            None => return Err(eyre::eyre!("Tool does not have permissions")),
        };
        
        // Remove from both lists to simplify UX
        let removed_from_allowed = permission.allowed_patterns.remove(pattern);
        let removed_from_blocked = permission.blocked_patterns.remove(pattern);
        
        if !removed_from_allowed && !removed_from_blocked {
            return Err(eyre::eyre!("Pattern not found in rules"));
        }
        
        Ok(())
    }
}
```

## Call For Tool Acceptance

```rust
async fn tool_use_execute(&mut self, mut tool_uses: Vec<QueuedTool>) -> Result<ChatState, ChatError> {
    // Verify tools have permissions.
    for (index, tool) in tool_uses.iter_mut().enumerate() {
        // Manually accepted by the user or otherwise verified already.
        if tool.accepted {
            continue;
        }

        // Prepare the permissions request from the tool
        let params = match tool {
            FsWrite(t), FsRead(t) => ToolParams::Path(t.path)
            ExecuteBash(t) => ToolParams::Command(t.command)
            UseAws(t) => ToolParams::AwsUse { service: t.service, operation: t.operation },
            _ => ToolParams::Empty
        };
        let requires_acceptance = if self.tool_permissions.requires_acceptance(&tool.name, params);

        if !requires_acceptance {
            tool.accepted = true;
            continue;
        }

        // ask for acceptance
    }

    // Execute tools
    // ...
}
```

## Command Parsing and Handling

The `/tools` command will be extended to support the new rule-based syntax:

```rust
pub enum ToolsSubcommand {
    /// ...existing commands
    
    // New commands
    Allow { 
        tool_name: String, 
        path: Option<String>,
        command: Option<String>,
        service: Option<String>,
        operation: Option<String>,
    },
    Deny { 
        tool_name: String, 
        path: Option<String>,
        command: Option<String>,
        service: Option<String>,
        operation: Option<String>,
    },
    RemoveRule { 
        tool_name: String, 
        path: Option<String>,
        command: Option<String>,
        service: Option<String>,
        operation: Option<String>,
    },
    Help,
}

fn handle_tools_command(&mut self, args: &str) -> Result<()> {
    let args = args.trim();
    
    if args.is_empty() {
        self.show_tool_permissions()?;
        return Ok(());
    }
    
    let parts: Vec<&str> = args.split_whitespace().collect();
    match parts[0] {
        // ... existing commands

        "allow" if parts.len() > 1 => {
            let tool_name = parts[1];
            let mut path = None;
            let mut command = None;
            let mut service = None;
            let mut operation = None;
            
            // Parse remaining arguments
            for i in 2..parts.len() {
                if parts[i] == "--path" && i + 1 < parts.len() {
                    path = Some(parts[i + 1]);
                } else if parts[i] == "--command" && i + 1 < parts.len() {
                    command = Some(parts[i + 1]);
                } else if parts[i] == "--service" && i + 1 < parts.len() {
                    service = Some(parts[i + 1]);
                } else if parts[i] == "--operation" && i + 1 < parts.len() {
                    operation = Some(parts[i + 1]);
                }
            }
            
            // Add appropriate rule based on tool and arguments
            if let Some(p) = path {
                self.tool_permissions.add_rule_pattern(tool_name, p.to_string(), true)?;
            } else if let Some(cmd) = command {
                self.tool_permissions.add_rule_pattern(tool_name, cmd.to_string(), true)?;
            } else if let (Some(svc), Some(op)) = (service, operation) {
                let pattern = format!("{} {}", svc, op);
                self.tool_permissions.add_rule_pattern(tool_name, pattern, true)?;
            } else {
                return Err(eyre::eyre!("Missing required arguments for allow command"));
            }
        },
        "block" if parts.len() > 1 => {
            // Similar implementation to "allow" but with is_allowed=false
            // ...
        },
        "remove-rule" if parts.len() > 1 => {
            // Similar implementation to parse arguments and call remove_rule_pattern
            // ...
        },
        // Other commands...
        _ => {
            self.output_tools_help()?;
        }
    }
    
    Ok(())
}
```

## Interactive Rule Creation

When a user chooses to create a rule during tool approval, we'll present appropriate options based on the tool type:

```rust
fn prompt_for_tool_execution(&mut self, tool_name: &str, params: &Value) -> Result<ToolPromptResponse> {
    execute!(
        self.output,
        style::SetForegroundColor(Color::Yellow),
        style::Print(format!("[Tool Request: {} ({})]\n", tool_name, params)),
        style::Print("Allow this action? Use 'c' to configure tool permission. [y/n/c]:"),
        style::SetForegroundColor(Color::Reset)
    )?;
    
    // Read user input and return appropriate response
    if input == "c" {
        prompt_for_rule_creation(tool_name, params);
    } else {
        // ...
    }
}

fn handle_command_rule_creation(&mut self, tool_name: &str, params: &Value) -> Result<ToolPromptResponse> {
    execute!(
        self.output,
        style::Print("Create rule for: execute_bash (command=git status)\n"),
        style::Print("Trusted commands do not ask for confirmation before running.\n\n"),
        style::Print("1. Trust this exact command only\n"),
        style::Print(format!("2. Trust all '{}' commands\n", sub_command(params.command))),
        style::Print(format!("3. Trust all '{}' commands\n", base_command(params.command))),
        style::Print(format!("4. Trust all requests from this tool '{}'\n", tool_name)),
        style::Print("Or, 'y' to run without adding a rule: "),
    )?;

    // Read user input and return appropriate response
    let choice = self.read_line()?;
    match choice.trim() {
        "1" => {
            self.tool_permissions.add_rule_pattern(tool_name, params.command.to_string(), true)?;
            Ok(ChatState::ToolExecute)
        },
        "2" => {
            let base_cmd = base_command(params.command);
            self.tool_permissions.add_rule_pattern(tool_name, sub_command, true)?;
            Ok(ChatState::ToolExecute)
        },
        "3" => {
            let cmd_name = base_command(params.command).split_whitespace().next().unwrap_or("");
            self.tool_permissions.add_rule_pattern(tool_name, base_cmd, true)?;
            Ok(ChatState::ToolExecute)
        },
        "4" => {
            self.tool_permissions.trust_tool(tool_name);
            Ok(ChatState::ToolExecute)
        },
        "y" => Ok(ChatState::ToolExecute),
        _ => Ok(ChatState::HandleInput)
    }
}

fn handle_filesystem_rule_creation(&mut self, tool_name: &str, params: &Value) -> Result<ToolPromptResponse> {
    execute!(
        self.output,
        style::Print("Create rule for: fs_write (path=/users/me/my/project/test.txt)\n"),
        style::Print("Trusted paths do not ask for confirmation before writing.\n\n"),
        style::Print("1. Trust this exact path only\n"),
        style::Print(format!("2. Trust the current directory ({})\n", pwd())),
        style::Print(format!("3. Trust all requests from this tool '{}'\n", tool_name)),
        style::Print("Or, 'y' to run without adding a rule: "),
    )?;

    // Read user input and return appropriate response
    let choice = self.read_line()?;
    match choice.trim() {
        "1" => {
            self.tool_permissions.add_rule_pattern(tool_name, params.path.to_string(), true)?;
            Ok(ChatState::ToolExecute)
        },
        "2" => {
            let current_dir = current_dir()?;
            self.tool_permissions.add_rule_pattern(tool_name, format!("{}/*", current_dir.display()), true)?;
            Ok(ChatState::ToolExecute)
        },
        "3" => {
            self.tool_permissions.trust_tool(tool_name);
            Ok(ToolPromptResponse::Yes)
        },
        "y" => Ok(ChatState::ToolExecute),
        _ => Ok(ChatState::HandleInput)
    }
}

fn handle_aws_rule_creation(&mut self, tool_name: &str, params: &Value) -> Result<ToolPromptResponse> {
    execute!(
        self.output,
        style::Print("Create rule for: use_aws (s3 list-buckets)\n"),
        style::Print("Trusted paths do not ask for confirmation before writing.\n\n"),
        style::Print("1. Trust this exact command only\n"),
        style::Print(format!("2. Trust all {} commands for {}\n", params.operation, params.service)),
        style::Print(format!("3. Trust all {} commands for any service\n", params.operation)),
        style::Print(format!("4. Trust all requests from this tool '{}'\n", tool_name)),
        style::Print("Or, 'y' to run without adding a rule: "),
    )?;

    // Read user input and return appropriate response
    let choice = self.read_line()?;
    match choice.trim() {
        "1" => {
            self.tool_permissions.add_rule_pattern(tool_name, params.path.to_string(), true)?;
            Ok(ChatState::ToolExecute)
        },
        "2" => {
            let current_dir = current_dir()?;
            self.tool_permissions.add_rule_pattern(tool_name, format!("{} {}", params.service, params.operation), true)?;
            Ok(ChatState::ToolExecute)
        },
        "3" => {
            self.tool_permissions.add_rule_pattern(tool_name, format!("* {}", params.operation), true)?;
            Ok(ToolPromptResponse::Yes)
        },
        "4" => {
            self.tool_permissions.trust_tool(tool_name);
            Ok(ToolPromptResponse::Yes)
        },
        "y" => Ok(ChatState::ToolExecute),
        _ => Ok(ChatState::HandleInput)
    }
}

fn prompt_for_rule_creation(&mut self, tool_name: &str, params: &Value) -> Result<()> {
    match tool_name {
        "fs_read" | "fs_write" => {
            self.handle_filesystem_rule_creation(tool_name, params)?;
        },
        "execute_bash" => {
            self.handle_command_rule_creation(tool_name, params)?;
        },
        "use_aws" => {
            self.handle_aws_rule_creation(tool_name, params)?;
        },
        _ => Err("Unsupported tool")
    }
    
    Ok(())
}
```

</details>


# Drawbacks

[drawbacks]: #drawbacks

1. **Increased Complexity**: Adding pattern-based permissions significantly increases the complexity of the permission system, both in terms of implementation and user understanding.

2. **UI Complexity**: We are adding more complicated configuration commands, which may make the tool less approachable for new users.

3. **Performance Impact**: Pattern matching, especially for complex patterns and large numbers of rules, could introduce performance overhead when checking permissions. Each tool use would require evaluating multiple patterns.

4. **Security Risks**: More granular permissions could lead to unintended security holes if users create overly broad patterns without fully understanding their implications.

5. **Maintenance Burden**: Additional built-in tools may have to implement custom permissions handling.

6. **Missing Further Granularity**: Users cannot easily specify specific arguments or filepaths.


# Rationale and alternatives

[rationale-and-alternatives]: #rationale-and-alternatives

## Why this design?

This design was chosen because it:

1. **Builds on existing foundations**: It extends the existing tool permissions system rather than replacing it, maintaining backward compatibility and leveraging users' existing knowledge.

2. **Addresses real user needs**: It solves common friction points where users want to trust specific operations but not entire tools.

3. **Uses familiar patterns**: The glob-style pattern matching is familiar to developers from file systems, shell commands, and gitignore files. This reduces the learning curve by building on existing knowledge. Non-glob style (prefix) matching for commands retains simplicity and covers most cases for users.

4. **Provides flexibility**: The pattern-based approach can be extended to new tools and use cases without changing the core permission model. This ensures the system can evolve as new tools are added. Command based matching can be extended to check for particular arguments or filespaths in any order.


## Alternatives considered

### 1. Permission Presets

We could define common permission presets (e.g., "development mode", "read-only mode") that users could switch between.

While simpler for users, this was rejected because it wouldn't provide the fine-grained control that users need for their specific workflows. It would be difficult to generalize useful presets.

### 2. Directory Allowlists

Instead of pattern matching, we could implement a simpler directory allowlist system where users specify directories that are trusted for specific operations.

While simpler to interact with, this was rejected because it lacks the flexibility needed to address the full range of use cases, particularly for command-based tools.

### 3. Regular Expression Patterns

We could use regular expressions for more powerful matching instead of relying on globs or prefix matching.

This was rejected because we can't expect users to understand regex to operate permissions. Also, glob is powerful for paths, and prefix matching gets us most of the way there.


## Impact of not doing this

Without this feature:
- Users will continue to face the all-or-nothing choice at the tool level
- Security-conscious users will be prompted more frequently than necessary
- Users may avoid using Amazon Q for certain tasks due to prompt fatigue


# Unresolved questions

[unresolved-questions]: #unresolved-questions

1. **Pattern Syntax Details**: What specific pattern syntax should we use? How will pattern matching hold up with paths on different operating systems?

2. **Rule Management Interface**: What's the most user-friendly way to allow users to manage (list, edit, delete) existing rules?

3. **User Experience Refinement**: What's the best way to present pattern-based permissions in the UI to make them understandable to users? How do we provide enough feedback without overwhelming users?

4. **Tool-Specific Parameter Handling**: Should different tools have different parameter types for pattern matching (e.g., `--path` for filesystem tools, `--command` for execute_bash)? How do we maintain consistency while addressing tool-specific needs?


# Future possibilities

[future-possibilities]: #future-possibilities

1. **Time-Limited Permissions**: Allow permissions to expire after a certain time or number of uses.

2. **Permission Auditing**: Provide a log of permission grants and tool uses for review.

3. **Integration with Profiles/Persistent Storage**: Allow saving trusted paths/commands as part of user profiles.

4. **Command Suggestions**: Suggest common commands to trust based on usage patterns.

5. **Risk Assessment**: Provide risk assessments for commands before trusting them.

6. **AWS Resource Handling**: A more complicated rule system for `use_aws` that can granularize up to the AWS resource to operate on.

7. **Command Argument and Paths**: We can extend the CLI to accept arguments and filepaths. That way, we can match on any versions of "rm -rf" (e.g. "r, -fr") without the user having to specify specific rules. We can restrict commands to certain directories as well.

7. **Rule Test Command**: A new command for users to test their permissions: `/tools test-rule <tool> <command/path>`.
