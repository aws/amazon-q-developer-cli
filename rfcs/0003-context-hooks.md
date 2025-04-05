- Feature Name: `context_hooks`
- Start Date: 2025-04-02

# Summary

[summary]: #summary

Implement "context hooks" which allow users or MCP servers to pass context either at the start of a conversation or alongside each prompt in Amazon Q CLI. This feature enables automated context injection without requiring manual `/context` commands.

# Motivation

[motivation]: #motivation

Currently, users must manually add context to Amazon Q CLI conversations using the `/context` command. This approach has several limitations:

1. It requires explicit user action for each piece of context
2. It doesn't support automated workflows where context should be dynamically injected
3. It doesn't integrate well with external tools or systems that could provide valuable context
4. It creates friction when users want to maintain consistent context across multiple conversations

By implementing context hooks, we can address these limitations and enable more powerful use cases:

- Automatically include relevant project information at the start of conversations
- Dynamically inject git status, environment variables, or other contextual information with each prompt
- Allow IDE integrations to provide code context without manual steps
- Enable MCP servers to provide organization-specific context to guide responses
- Support workflow automation where context is programmatically determined

# Guide-level explanation

[guide-level-explanation]: #guide-level-explanation

## What are Context Hooks?

Context hooks are configurable functions that automatically provide relevant context to Amazon Q CLI conversations. They come in two varieties:

1. **Conversation Start Hooks**: Run once at the beginning of a conversation to provide initial context
2. **Per-Prompt Hooks**: Run alongside each prompt to provide up-to-date context

## Configuration

Context hooks are configured within the existing Amazon Q CLI configuration structure to maintain consistency and leverage the established profile system. The configuration is stored in:

1. **Global Configuration**: `~/.aws/amazonq/global_config.json` for system-wide hooks
2. **Profile Configuration**: `~/.aws/amazonq/profiles/<profile-name>/context.json` for profile-specific hooks

This approach ensures that hooks are properly integrated with the existing context management system and respect profile settings.

Example global configuration:

```json
{
  "context": {
    "hooks": {
      "conversation_start": [
        {
          "name": "project-info",
          "type": "script",
          "path": "~/.aws/amazonq/hooks/project-info.sh",
          "enabled": true
        },
        {
          "name": "company-guidelines",
          "type": "http",
          "url": "https://internal-server.example.com/context-api",
          "headers": {
            "Authorization": "Bearer ${CONTEXT_API_TOKEN}"
          },
          "enabled": true
        }
      ],
      "per_prompt": [
        {
          "name": "environment-vars",
          "type": "inline",
          "command": "env | grep AWS_",
          "enabled": false
        }
      ]
    }
  }
}
```

Example profile-specific configuration:

```json
{
  "hooks": {
    "conversation_start": [],
    "per_prompt": [
      {
        "name": "git-status",
        "type": "script",
        "path": "~/.aws/amazonq/hooks/git-status.sh",
        "enabled": true
      }
    ]
  }
}
```

### Simplified Command Syntax

For ease of use, hooks can also be configured using a simplified prefix notation in the command field:

```json
{
  "hooks": {
    "conversation_start": [
      {
        "name": "project-info",
        "command": "!~/.aws/amazonq/hooks/project-info.sh",
        "enabled": true
      },
      {
        "name": "company-guidelines",
        "command": "#context-api https://internal-server.example.com/context-api",
        "headers": {
          "Authorization": "Bearer ${CONTEXT_API_TOKEN}"
        },
        "enabled": true
      }
    ],
    "per_prompt": [
      {
        "name": "git-status",
        "command": "!git status --short",
        "enabled": true
      }
    ]
  }
}
```

Where:
- `!` prefix indicates a shell command
- `/` prefix indicates an internal command
- `#` prefix indicates a tool call

This simplified syntax makes it easier to read and write hook configurations, especially for common use cases.

## Hook Types

### Inline Hooks

Inline hooks execute a shell command directly:

```
env | grep AWS_

# Output:
AWS_PROFILE=development
AWS_REGION=us-west-2
```

> **Note**: Inline hooks will be implemented first, and we will evaluate whether the other hook types (Script and HTTP) are required based on user feedback and use cases.

### Script Hooks

Script hooks execute a local script that outputs context information:

```bash
#!/bin/bash
# ~/.config/amazon-q/hooks/project-info.sh

echo "# Project Information"
echo "Project: $(basename $(pwd))"
echo "Language: $(cat .tool-versions 2>/dev/null || echo "unknown")"
echo "Framework: $([ -f package.json ] && echo "Node.js" || echo "unknown")"
```

### HTTP Hooks

HTTP hooks make a request to an API endpoint that returns context information:

```
GET https://internal-server.example.com/context-api
Authorization: Bearer abc123

Response:
# Company Guidelines
- Follow the AWS Well-Architected Framework
- Use infrastructure as code
- Implement least privilege access
```

# Output:
AWS_PROFILE=development
AWS_REGION=us-west-2
```

## Using Context Hooks

Once configured, context hooks run automatically:

1. When starting a new conversation with `q chat`, conversation start hooks run
2. When sending a prompt, per-prompt hooks run

Users can see which hooks are active with the `/context hooks` command:

```
> /context hooks
Active context hooks:
- project-info (conversation start)
- git-status (per prompt)
```

Users can add inline hooks directly from the chat interface:

```
> /context hooks add git-status --type=per-prompt --command="git status --short"
Added hook: git-status (per-prompt)

> /context hooks add project-info --type=conversation-start --command="echo 'Project: $(basename $(pwd))'"
Added hook: project-info (conversation-start)
```

Users can temporarily disable hooks with the `/context hooks disable` command:

```
> /context hooks disable git-status
Disabled hook: git-status
```

And re-enable them with `/context hooks enable`:

```
> /context hooks enable git-status
Enabled hook: git-status
```

Users can also enable or disable all hooks at once:

```
> /context hooks disable-all
Disabled all hooks

> /context hooks enable-all
Enabled all hooks
```

When hooks are running, the UI shows feedback about their execution:

```
> Running hook: git-status...
> Hook git-status completed in 0.03s
```

### Context Visibility

Users can view how hooks contribute to the context with the `/context show` command:

```
> /context show
===============================
Current context:

global:
    .amazonq/rules/**/*.md
    README.md
    AmazonQ.md

profile:
    default

Expanded files (2):
    /Users/user/project/README.md
    /Users/user/project/AmazonQ.md

Context hooks (2):
    project-info (conversation start)
    git-status (per prompt)
```

Additional context visibility commands include:

- `/context show --hooks` - Show only hook contributions
- `/context show --previous` - Show context from previous prompt
- `/context show --always` - Always show context after each prompt

This visibility helps users understand what context is being provided to the LLM, making the system more transparent and easier to debug.

### Hook Persistence and Behavior

- **Conversation Start Hooks**: Evaluated once at the beginning of a conversation. Their context persists at the top of the conversation, ahead of the first prompt, even if earlier messages are compacted or cleared away.
- **Per-Prompt Hooks**: Evaluated on every user prompt and attached immediately before the human text.
- **Hook Disabling**: When hooks are disabled, they remain in the config file with a disabled flag and stay disabled until explicitly re-enabled.
- **Clear Command**: Running `/clear` will re-evaluate conversation start hooks, refreshing their context.

# Reference-level explanation

[reference-level-explanation]: #reference-level-explanation

## Architecture

The context hooks system consists of several components:

1. **Hook Configuration Manager**: Loads and validates hook configurations
2. **Hook Executor**: Executes hooks and captures their output
3. **Context Manager Integration**: Integrates hook output with the existing context system
4. **Hook Registry**: Maintains the state of enabled/disabled hooks

### Hook Configuration Manager

The Hook Configuration Manager is responsible for loading hook configurations from the YAML file and validating them:

```rust
pub struct HookConfig {
    pub name: String,
    pub hook_type: HookType,
    pub enabled: bool,
    // Common fields
    pub timeout_ms: Option<u64>,
    pub max_output_size: Option<usize>,
    pub criticality: Criticality,
    pub cache_ttl_seconds: Option<u64>,
    // Type-specific fields
    pub path: Option<String>,        // For script hooks
    pub command: Option<String>,     // For inline hooks
    pub url: Option<String>,         // For HTTP hooks
    pub headers: Option<HashMap<String, String>>, // For HTTP hooks
}

pub enum HookType {
    Script,
    Http,
    Inline,
}

pub enum Criticality {
    Fail,    // Hook failure will prevent prompt from being sent
    Warn,    // Hook failure will log a warning but allow prompt to be sent
    Ignore,  // Hook failure will be silently ignored
}

pub struct HookConfigManager {
    config_path: PathBuf,
    conversation_start_hooks: Vec<HookConfig>,
    per_prompt_hooks: Vec<HookConfig>,
}

impl HookConfigManager {
    pub fn new() -> Result<Self> {
        // Find and load config file
        // Parse YAML into HookConfig structs
        // Validate configurations
    }
    
    pub fn get_conversation_start_hooks(&self) -> &[HookConfig] {
        &self.conversation_start_hooks
    }
    
    pub fn get_per_prompt_hooks(&self) -> &[HookConfig] {
        &self.per_prompt_hooks
    }
    
    pub fn add_inline_hook(&self, name: &str, hook_type: &str, command: &str) -> Result<()> {
        // Create a new HookConfig for an inline hook
        let hook_type = match hook_type {
            "conversation-start" => HookType::Inline,
            "per-prompt" => HookType::Inline,
            _ => return Err(anyhow!("Invalid hook type: {}", hook_type)),
        };
        
        let hook = HookConfig {
            name: name.to_string(),
            hook_type,
            enabled: true,
            timeout_ms: Some(5000), // 5 second default timeout
            max_output_size: Some(10240), // 10KB default max output
            criticality: Criticality::Warn, // Default to warn on failure
            cache_ttl_seconds: Some(60), // Default 1 minute cache
            path: None,
            command: Some(command.to_string()),
            url: None,
            headers: None,
        };
        
        // Add to appropriate list based on type
        if hook_type == "conversation-start" {
            self.conversation_start_hooks.push(hook);
        } else {
            self.per_prompt_hooks.push(hook);
        }
        
        // Save configuration to file
        self.save_config()
    }
    
    fn save_config(&self) -> Result<()> {
        // Save configuration to file
        // This will be called after adding, enabling, or disabling hooks
    }
}
    conversation_start_hooks: Vec<HookConfig>,
    per_prompt_hooks: Vec<HookConfig>,
}

impl HookConfigManager {
    pub fn new() -> Result<Self> {
        // Find and load config file
        // Parse YAML into HookConfig structs
        // Validate configurations
    }
    
    pub fn get_conversation_start_hooks(&self) -> &[HookConfig] {
        &self.conversation_start_hooks
    }
    
    pub fn get_per_prompt_hooks(&self) -> &[HookConfig] {
        &self.per_prompt_hooks
    }
}
```

### Hook Executor

The Hook Executor runs hooks and captures their output:

```rust
struct CachedResult {
    output: String,
    timestamp: Instant,
}

pub struct HookExecutor {
    config_manager: Arc<HookConfigManager>,
    hook_registry: Arc<HookRegistry>,
    cache: DashMap<String, CachedResult>,
}

impl HookExecutor {
    pub fn new(
        config_manager: Arc<HookConfigManager>,
        hook_registry: Arc<HookRegistry>,
    ) -> Self {
        Self {
            config_manager,
            hook_registry,
            cache: DashMap::new(),
        }
    }
    
    pub async fn execute_conversation_start_hooks(&self) -> Result<Vec<ContextEntry>> {
        // Get enabled conversation start hooks
        let hooks = self.config_manager.get_conversation_start_hooks();
        let mut context_entries = Vec::new();
        
        // Create futures for all hooks to run in parallel
        let mut futures = Vec::new();
        for hook in hooks {
            if !self.hook_registry.is_hook_enabled(&hook.name) {
                continue;
            }
            
            let hook_clone = hook.clone();
            let self_clone = self.clone();
            futures.push(async move {
                (hook_clone.name.clone(), self_clone.execute_hook(&hook_clone).await)
            });
        }
        
        // Wait for all hooks to complete
        let results = futures::future::join_all(futures).await;
        
        // Process results
        for (name, result) in results {
            match result {
                Ok(output) => {
                    // Format output as context entry
                    let entry = ContextEntry::new(
                        format!("hook:{}", name),
                        output,
                        ContextSource::Hook(name),
                    );
                    context_entries.push(entry);
                }
                Err(e) => {
                    println!("Hook {} failed: {}", name, e);
                }
            }
        }
        
        Ok(context_entries)
    }
    
    pub async fn execute_per_prompt_hooks(&self) -> Result<Vec<ContextEntry>> {
        // Similar to execute_conversation_start_hooks but for per-prompt hooks
        // Get enabled per-prompt hooks
        let hooks = self.config_manager.get_per_prompt_hooks();
        let mut context_entries = Vec::new();
        
        // Create futures for all hooks to run in parallel
        let mut futures = Vec::new();
        for hook in hooks {
            if !self.hook_registry.is_hook_enabled(&hook.name) {
                continue;
            }
            
            println!("Running hook: {}...", hook.name);
            let start_time = Instant::now();
            
            match self.execute_hook(hook).await {
                Ok(output) => {
                    let elapsed = start_time.elapsed();
                    println!("Hook {} completed in {:?}", hook.name, elapsed);
                    
                    // Format output as context entry
                    let entry = ContextEntry::new(
                        format!("hook:{}", hook.name),
                        output,
                        ContextSource::Hook(hook.name.clone()),
                    );
                    context_entries.push(entry);
                }
                Err(e) => {
                    println!("Hook {} failed: {}", hook.name, e);
                }
            }
        }
        
        Ok(context_entries)
    }
    
    pub async fn execute_per_prompt_hooks(&self) -> Result<Vec<ContextEntry>> {
        // Get enabled per-prompt hooks
        let hooks = self.config_manager.get_per_prompt_hooks();
        let mut context_entries = Vec::new();
        
        for hook in hooks {
            if !self.hook_registry.is_hook_enabled(&hook.name) {
                continue;
            }
            
            println!("Running hook: {}...", hook.name);
            let start_time = Instant::now();
            
            match self.execute_hook(hook).await {
                Ok(output) => {
                    let elapsed = start_time.elapsed();
                    println!("Hook {} completed in {:?}", hook.name, elapsed);
                    
                    // Format output as context entry
                    let entry = ContextEntry::new(
                        format!("hook:{}", hook.name),
                        output,
                        ContextSource::Hook(hook.name.clone()),
                    );
                    context_entries.push(entry);
                }
                Err(e) => {
                    println!("Hook {} failed: {}", hook.name, e);
                }
            }
        }
        
        Ok(context_entries)
    }
    
    async fn execute_hook(&self, hook: &HookConfig) -> Result<String> {
        match hook.hook_type {
            HookType::Script => self.execute_script_hook(hook).await,
            HookType::Http => self.execute_http_hook(hook).await,
            HookType::Inline => self.execute_inline_hook(hook).await,
        }
    }
    
    async fn execute_script_hook(&self, hook: &HookConfig) -> Result<String> {
        // Execute script at hook.path
        // Capture stdout
        // Handle errors and timeouts
    }
    
    async fn execute_http_hook(&self, hook: &HookConfig) -> Result<String> {
        // Make HTTP request to hook.url with headers
        // Capture response body
        // Handle errors and timeouts
    }
    
    async fn execute_inline_hook(&self, hook: &HookConfig) -> Result<String> {
        // Check cache first if TTL is set
        if let Some(ttl) = hook.cache_ttl_seconds {
            if let Some(cached_result) = self.cache.get(&hook.name) {
                if cached_result.timestamp.elapsed().as_secs() < ttl {
                    return Ok(cached_result.output.clone());
                }
            }
        }
        
        // Execute hook.command in shell using tokio::process::Command
        let start = Instant::now();
        
        // Create a future with timeout
        let timeout_duration = Duration::from_millis(hook.timeout_ms.unwrap_or(5000));
        let command_future = async {
            Command::new("sh")
                .arg("-c")
                .arg(&hook.command.as_ref().unwrap())
                .output()
                .await
        };
        
        // Run with timeout
        let output = match tokio::time::timeout(timeout_duration, command_future).await {
            Ok(result) => result?,
            Err(_) => {
                // Handle timeout based on criticality
                match hook.criticality {
                    Criticality::Fail => {
                        return Err(anyhow!("Hook timed out after {:?}", timeout_duration));
                    },
                    Criticality::Warn => {
                        println!("Warning: Hook '{}' timed out after {:?}", hook.name, timeout_duration);
                        return Ok(format!("# Warning: Hook '{}' timed out", hook.name));
                    },
                    Criticality::Ignore => {
                        return Ok(String::new());
                    }
                }
            }
        };
            
        if !output.status.success() {
            // Handle command failure based on criticality
            match hook.criticality {
                Criticality::Fail => {
                    return Err(anyhow!("Command failed with status: {}", output.status));
                },
                Criticality::Warn => {
                    println!("Warning: Hook '{}' failed with status: {}", hook.name, output.status);
                    return Ok(format!("# Warning: Hook '{}' failed with status: {}", 
                        hook.name, output.status));
                },
                Criticality::Ignore => {
                    return Ok(String::new());
                }
            }
        }
        
        // Capture stdout and handle errors
        let stdout = String::from_utf8(output.stdout)?;
        
        // Apply size limits if configured
        if let Some(max_size) = hook.max_output_size {
            if stdout.len() > max_size {
                return Ok(format!("{}\n... (output truncated, exceeded {} bytes)", 
                    &stdout[..max_size], max_size));
            }
        }
        
        Ok(stdout)
        // Handle errors and timeouts
    }
}
```

### Context Manager Integration

The Context Manager Integration connects hook output to the existing context system:

```rust
pub struct ContextManagerIntegration {
    hook_executor: Arc<HookExecutor>,
    context_manager: Arc<ContextManager>,
}

impl ContextManagerIntegration {
    pub fn new(
        hook_executor: Arc<HookExecutor>,
        context_manager: Arc<ContextManager>,
    ) -> Self {
        Self {
            hook_executor,
            context_manager,
        }
    }
    
    pub async fn apply_conversation_start_hooks(&self) -> Result<()> {
        let context_entries = self.hook_executor.execute_conversation_start_hooks().await?;
        for entry in context_entries {
            self.context_manager.add_context(entry)?;
        }
        Ok(())
    }
    
    pub async fn apply_per_prompt_hooks(&self) -> Result<()> {
        let context_entries = self.hook_executor.execute_per_prompt_hooks().await?;
        for entry in context_entries {
            self.context_manager.add_context(entry)?;
        }
        Ok(())
    }
}
```

### Hook Registry

The Hook Registry maintains the state of enabled/disabled hooks:

```rust
pub struct HookRegistry {
    disabled_hooks: RwLock<HashSet<String>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            disabled_hooks: RwLock::new(HashSet::new()),
        }
    }
    
    pub fn disable_hook(&self, name: &str) -> Result<()> {
        let mut disabled_hooks = self.disabled_hooks.write().unwrap();
        disabled_hooks.insert(name.to_string());
        Ok(())
    }
    
    pub fn enable_hook(&self, name: &str) -> Result<()> {
        let mut disabled_hooks = self.disabled_hooks.write().unwrap();
        disabled_hooks.remove(name);
        Ok(())
    }
    
    pub fn disable_all_hooks(&self, config_manager: &HookConfigManager) -> Result<()> {
        let mut disabled_hooks = self.disabled_hooks.write().unwrap();
        
        // Disable all conversation start hooks
        for hook in config_manager.get_conversation_start_hooks() {
            disabled_hooks.insert(hook.name.clone());
        }
        
        // Disable all per-prompt hooks
        for hook in config_manager.get_per_prompt_hooks() {
            disabled_hooks.insert(hook.name.clone());
        }
        
        Ok(())
    }
    
    pub fn enable_all_hooks(&self) -> Result<()> {
        let mut disabled_hooks = self.disabled_hooks.write().unwrap();
        disabled_hooks.clear();
        Ok(())
    }
    
    pub fn is_hook_enabled(&self, name: &str) -> bool {
        let disabled_hooks = self.disabled_hooks.read().unwrap();
        !disabled_hooks.contains(name)
    }
}
```

## Integration with Chat Command

The context hooks system integrates with the existing chat command:

```rust
impl ChatCommand {
    pub async fn run(&self) -> Result<()> {
        // Initialize components
        let config_manager = Arc::new(HookConfigManager::new()?);
        let hook_registry = Arc::new(HookRegistry::new());
        let hook_executor = Arc::new(HookExecutor::new(config_manager.clone(), hook_registry.clone()));
        let context_integration = ContextManagerIntegration::new(hook_executor.clone(), self.context_manager.clone());
        
        // Apply conversation start hooks
        println!("Running conversation start hooks...");
        let start_time = Instant::now();
        context_integration.apply_conversation_start_hooks().await?;
        println!("Conversation start hooks completed in {:?}", start_time.elapsed());
        
        // Start chat loop
        loop {
            // Get user input
            let input = self.get_user_input().await?;
            
            // Check for hook commands
            if input.starts_with("/context hooks") {
                self.handle_hook_command(input, &hook_registry).await?;
                continue;
            }
            
            // Apply per-prompt hooks before sending prompt
            println!("Running per-prompt hooks...");
            let start_time = Instant::now();
            context_integration.apply_per_prompt_hooks().await?;
            println!("Per-prompt hooks completed in {:?}", start_time.elapsed());
            
            // Send prompt to LLM
            // ...
        }
    }
    
    async fn handle_hook_command(&self, input: String, hook_registry: &Arc<HookRegistry>) -> Result<()> {
        // Parse command: /hooks, /hooks disable <name>, /hooks enable <name>
        // Call appropriate hook_registry methods
        // Display feedback to user
    }
}
```

## Security Considerations

1. **Script Execution**: Scripts run with the same permissions as the Amazon Q CLI process
2. **HTTP Requests**: Sensitive information in headers (like tokens) should be stored securely
3. **Output Validation**: Hook output should be validated to prevent injection attacks
4. **Resource Limits**: Hooks should have timeouts and output size limits

## Performance Considerations

Context hooks can impact performance in several ways:

1. **Startup Time**: Conversation start hooks run before the first prompt, potentially increasing startup time
2. **Prompt Latency**: Per-prompt hooks run before each prompt, potentially increasing response time
3. **Execution Model**: For both conversation start and per-prompt hooks:
   - Hooks run asynchronously but the prompt will not be sent to the server until all hooks have completed (either successfully or with error/timeout)
   - Hooks run in parallel with configurable timeouts
   - Each hook has individual configuration for:
     - Criticality (fail on error, warn on error, ignore errors)
     - Cache TTL (time to live for cached results)
     - Timeout duration
4. **Caching**: Hook outputs can be cached with configurable TTL to avoid redundant executions

### Hook Configuration Example

```json
{
  "hooks": {
    "conversation_start": [
      {
        "name": "project-info",
        "type": "inline",
        "command": "echo 'Project: $(basename $(pwd))'",
        "enabled": true,
        "criticality": "warn",  // Options: fail, warn, ignore
        "cache_ttl_seconds": 300,  // 5 minutes
        "timeout_ms": 2000  // 2 seconds
      }
    ],
    "per_prompt": [
      {
        "name": "git-status",
        "type": "inline",
        "command": "git status --short",
        "enabled": true,
        "criticality": "ignore",
        "cache_ttl_seconds": 10,
        "timeout_ms": 1000  // 1 second
      }
    ]
  }
}
```

## MCP Integration

The context hooks system addresses several limitations with the current MCP protocol:

1. **Dynamic Context**: MCP servers can now provide context dynamically through HTTP hooks
2. **Per-Prompt Updates**: Context can be updated with each prompt without requiring client-side changes
3. **Protocol Extension**: This provides a standardized way for MCP servers to inject context without protocol changes
4. **Organizational Policies**: Companies can implement centralized context policies through MCP-provided hooks

# Drawbacks

[drawbacks]: #drawbacks

1. **Security Risks**: Executing arbitrary scripts and making HTTP requests introduces security risks
2. **Performance Impact**: Running hooks with each prompt could slow down the conversation flow
3. **Complexity**: Adds another configuration system that users need to understand
4. **Maintenance Burden**: More code to maintain and test
5. **Potential for Abuse**: Could be used to inject misleading context

# Rationale and alternatives

[rationale-and-alternatives]: #rationale-and-alternatives

## Why this design?

1. **Flexibility**: Supports multiple hook types (script, HTTP, inline) to cover various use cases
2. **Integration**: Builds on the existing context system rather than replacing it
3. **User Control**: Allows users to enable/disable hooks as needed
4. **Separation of Concerns**: Clearly separates conversation start hooks from per-prompt hooks

## Alternatives considered

### 1. Context Plugins

Instead of hooks, implement a plugin system where plugins can provide context:

```rust
trait ContextPlugin {
    fn get_conversation_start_context(&self) -> Result<String>;
    fn get_per_prompt_context(&self) -> Result<String>;
}
```

**Rationale for not choosing**: More complex to implement and would require a plugin loading system. Hooks are simpler and more aligned with the command-line nature of the tool.

### 2. Context Templates

Define context templates that can include dynamic elements:

```yaml
templates:
  project-info: |
    # Project: {{exec "basename $(pwd)"}}
    # Language: {{exec "cat .tool-versions 2>/dev/null || echo 'unknown'"}}
```

**Rationale for not choosing**: While powerful, this approach mixes template syntax with context content, which could be confusing. The hook system keeps the execution logic separate from the context content.

### 3. Environment Variables Only

Use environment variables to control context injection:

```bash
export Q_CONTEXT_HOOKS="project-info,git-status"
```

**Rationale for not choosing**: Limited flexibility compared to a configuration file. Doesn't support different hook types or per-hook configuration.

## Impact of not doing this

Without context hooks:

1. Users will continue to manually add context, leading to inconsistent usage
2. Integration with external tools and systems will remain limited
3. Automated workflows involving Amazon Q CLI will be harder to implement
4. The CLI will be less powerful for advanced users and organizations

# Unresolved questions

[unresolved-questions]: #unresolved-questions

1. **Hook Ordering**: Hooks are evaluated in the following order:
   - Per-Conversation hooks are evaluated once at the start of a conversation, in the order they appear in the configuration file
   - Per-Prompt hooks are evaluated with every prompt, in the order they appear in the configuration file
2. **Error Handling**: How should errors in hook execution be presented to users?
3. **Context Deduplication**: How should duplicate context from different hooks be handled?
4. **Hook Dependencies**: Should hooks be able to depend on other hooks?
5. **Hook Output Format**: Should hooks output plain text, or should they support structured formats like JSON?
6. **Hook Versioning**: How will we handle backward compatibility as the hook system evolves?

# Future possibilities

[future-possibilities]: #future-possibilities

1. **Hook Marketplace**: A central repository where users can discover and share useful hooks
2. **Hook Templates**: Pre-defined templates for common use cases (git info, AWS environment, etc.)
3. **Conditional Hooks**: Hooks that only run under certain conditions (e.g., in specific directories)
4. **Hook Chaining**: Allow hooks to be chained together, with the output of one hook feeding into another
5. **Interactive Hooks**: Hooks that can prompt the user for input
6. **Hook Scheduling**: Hooks that run on a schedule rather than at conversation start or per prompt
7. **IDE Integration**: Deeper integration with IDEs to provide rich code context
8. **Team Hooks**: Organization-wide hooks that can be centrally managed
9. **Context Visualization**: UI to show which hooks contributed which context
10. **Hook Analytics**: Track which hooks are most useful for improving responses

## Use Case Examples

### Project Information

```bash
#!/bin/bash
# ~/.config/amazon-q/hooks/project-info.sh

echo "# Project Information"
echo "Project: $(basename $(pwd))"
echo "Language: $(cat .tool-versions 2>/dev/null || echo "unknown")"
echo "Framework: $([ -f package.json ] && echo "Node.js" || echo "unknown")"
```

### Git Status

```bash
#!/bin/bash
# ~/.config/amazon-q/hooks/git-status.sh

if git rev-parse --is-inside-work-tree &>/dev/null; then
  echo "# Git Status"
  echo "Branch: $(git branch --show-current)"
  echo "```"
  git status --short
  echo "```"
fi
```

### Personalization

```bash
#!/bin/bash
# ~/.config/amazon-q/hooks/personalization.sh

q-personalization --list-memories
```
