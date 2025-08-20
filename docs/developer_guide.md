# Amazon Q CLI Automatic Naming Feature - Developer Guide

## Architecture Overview

The Automatic Naming feature for Amazon Q CLI is designed with a modular architecture that separates concerns and allows for easy extension and maintenance. The main components are:

1. **Conversation Model**: Represents the structure of a conversation with messages and metadata.
2. **Topic Extractor**: Analyzes conversation content to extract main topics, subtopics, and action types.
3. **Filename Generator**: Generates filenames based on extracted topics and configuration settings.
4. **Save Configuration**: Manages user configuration for saving conversations.
5. **Save Command**: Handles the save command and integrates all components.
6. **Security**: Provides security features for file operations.

## Module Descriptions

### Conversation Module (`conversation.rs`)

The Conversation module defines the structure of a conversation and provides methods for working with conversations.

```rust
pub struct Conversation {
    pub id: String,
    pub messages: Vec<Message>,
    pub metadata: HashMap<String, String>,
}

pub struct Message {
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}
```

Key methods:
- `new(id: String) -> Conversation`: Creates a new conversation with the given ID.
- `add_user_message(content: String) -> &mut Self`: Adds a user message to the conversation.
- `add_assistant_message(content: String, tool_calls: Option<Vec<ToolCall>>) -> &mut Self`: Adds an assistant message to the conversation.
- `user_messages() -> Vec<&Message>`: Returns a vector of references to user messages.
- `assistant_messages() -> Vec<&Message>`: Returns a vector of references to assistant messages.
- `add_metadata(key: &str, value: &str)`: Adds metadata to the conversation.
- `get_metadata(key: &str) -> Option<&str>`: Gets metadata from the conversation.

### Topic Extractor Module (`topic_extractor.rs`)

The Topic Extractor module analyzes conversation content to extract main topics, subtopics, and action types. It provides three levels of extraction:

1. **Basic Extractor** (`basic.rs`): Simple keyword-based extraction.
2. **Enhanced Extractor** (`enhanced.rs`): Improved extraction with better context awareness.
3. **Advanced Extractor** (`advanced.rs`): Sophisticated extraction with NLP techniques.

```rust
pub fn extract_topics(conversation: &Conversation) -> (String, String, String)
```

The function returns a tuple of `(main_topic, sub_topic, action_type)`.

### Filename Generator Module (`filename_generator.rs`)

The Filename Generator module generates filenames based on extracted topics and configuration settings.

```rust
pub fn generate_filename(conversation: &Conversation) -> String
pub fn generate_filename_with_extractor(conversation: &Conversation, extractor: &TopicExtractorFn) -> String
pub fn generate_filename_with_config(conversation: &Conversation, config: &SaveConfig) -> String
pub fn generate_filename_with_template(conversation: &Conversation, config: &SaveConfig, template_name: &str) -> String
```

### Save Configuration Module (`save_config.rs`)

The Save Configuration module manages user configuration for saving conversations.

```rust
pub struct SaveConfig {
    config_path: PathBuf,
    default_path: String,
    filename_format: FilenameFormat,
    prefix: String,
    separator: String,
    date_format: String,
    topic_extractor_name: String,
    templates: HashMap<String, FilenameFormat>,
    metadata: HashMap<String, String>,
    mock_fs_error: Option<io::Error>,
}

pub enum FilenameFormat {
    Default,
    Custom(String),
}
```

Key methods:
- `new(config_path: P) -> Self`: Creates a new save configuration.
- `get_default_path() -> String`: Gets the default path for saving conversations.
- `set_default_path(path: &str) -> io::Result<()>`: Sets the default path for saving conversations.
- `get_filename_format() -> &FilenameFormat`: Gets the filename format.
- `set_filename_format(format: FilenameFormat) -> io::Result<()>`: Sets the filename format.
- `add_template(name: &str, format: FilenameFormat) -> io::Result<()>`: Adds a template for generating filenames.
- `get_template(name: &str) -> Option<&FilenameFormat>`: Gets a template for generating filenames.

### Save Command Module (`commands/save.rs`)

The Save Command module handles the save command and integrates all components.

```rust
pub fn handle_save_command(args: &[String], conversation: &Conversation, config: &SaveConfig) -> Result<String, SaveError>
pub fn handle_save_command_with_extractor(args: &[String], conversation: &Conversation, config: &SaveConfig, extractor: &fn(&Conversation) -> (String, String, String)) -> Result<String, SaveError>
```

### Security Module (`security.rs`)

The Security module provides security features for file operations.

```rust
pub struct SecuritySettings {
    pub redact_sensitive: bool,
    pub prevent_overwrite: bool,
    pub file_permissions: u32,
    pub directory_permissions: u32,
    pub max_path_depth: usize,
    pub follow_symlinks: bool,
}

pub fn validate_path(path: &Path, settings: &SecuritySettings) -> Result<PathBuf, SecurityError>
pub fn create_secure_directory(path: &Path, settings: &SecuritySettings) -> Result<(), SecurityError>
pub fn write_secure_file(path: &Path, content: &str, settings: &SecuritySettings) -> Result<(), SecurityError>
pub fn redact_sensitive_information(text: &str) -> String
pub fn generate_unique_filename(path: &Path) -> PathBuf
```

## Integration Points

### Command Registration

The save command is registered with the command registry in `commands/mod.rs`:

```rust
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register_command(
        "save",
        "Save the current conversation",
        save::handle_save_command,
    );
}
```

### Topic Extractor Selection

The topic extractor is selected based on the configuration in `filename_generator.rs`:

```rust
fn get_topic_extractor(name: &str) -> TopicExtractorFn {
    match name {
        "basic" => basic::extract_topics,
        "enhanced" => enhanced::extract_topics,
        "advanced" => advanced::extract_topics,
        _ => topic_extractor::extract_topics,
    }
}
```

### Security Integration

Security features are integrated in the save command in `commands/save.rs`:

```rust
pub fn save_conversation_to_file(
    conversation: &Conversation,
    path: &Path,
    config: &SaveConfig,
    options: &HashMap<String, String>,
    security_settings: &SecuritySettings,
) -> Result<(), SaveError> {
    // Add custom metadata if specified
    let mut conversation_with_metadata = conversation.clone();
    
    // Add metadata from config
    for (key, value) in config.get_metadata() {
        conversation_with_metadata.add_metadata(key, value);
    }
    
    // Add metadata from options
    if let Some(metadata) = options.get("metadata") {
        for pair in metadata.split(',') {
            let parts: Vec<&str> = pair.split('=').collect();
            if parts.len() == 2 {
                conversation_with_metadata.add_metadata(parts[0], parts[1]);
            }
        }
    }
    
    // Redact sensitive information if enabled
    if security_settings.redact_sensitive {
        conversation_with_metadata = redact_conversation(&conversation_with_metadata);
    }
    
    // Serialize the conversation
    let content = serde_json::to_string_pretty(&conversation_with_metadata)?;
    
    // Write to file securely
    write_secure_file(path, &content, security_settings)?;
    
    Ok(())
}
```

## Extension Points

### Adding a New Topic Extractor

To add a new topic extractor:

1. Create a new module in `topic_extractor/` (e.g., `topic_extractor/custom.rs`).
2. Implement the `extract_topics` function with the signature:
   ```rust
   pub fn extract_topics(conversation: &Conversation) -> (String, String, String)
   ```
3. Update the `get_topic_extractor` function in `filename_generator.rs` to include the new extractor:
   ```rust
   fn get_topic_extractor(name: &str) -> TopicExtractorFn {
       match name {
           "basic" => basic::extract_topics,
           "enhanced" => enhanced::extract_topics,
           "advanced" => advanced::extract_topics,
           "custom" => custom::extract_topics,
           _ => topic_extractor::extract_topics,
       }
   }
   ```

### Adding a New Filename Format

To add a new filename format:

1. Update the `FilenameFormat` enum in `save_config.rs` to include the new format:
   ```rust
   pub enum FilenameFormat {
       Default,
       Custom(String),
       NewFormat,
   }
   ```
2. Update the `generate_filename_with_config` function in `filename_generator.rs` to handle the new format:
   ```rust
   pub fn generate_filename_with_config(
       conversation: &Conversation,
       config: &SaveConfig,
   ) -> String {
       // ...
       match config.get_filename_format() {
           FilenameFormat::Default => {
               // ...
           },
           FilenameFormat::Custom(format) => {
               // ...
           },
           FilenameFormat::NewFormat => {
               // Handle the new format
           },
       }
       // ...
   }
   ```

### Adding a New Security Feature

To add a new security feature:

1. Update the `SecuritySettings` struct in `security.rs` to include the new feature:
   ```rust
   pub struct SecuritySettings {
       pub redact_sensitive: bool,
       pub prevent_overwrite: bool,
       pub file_permissions: u32,
       pub directory_permissions: u32,
       pub max_path_depth: usize,
       pub follow_symlinks: bool,
       pub new_feature: bool,
   }
   ```
2. Update the `create_security_settings` function in `commands/save.rs` to handle the new feature:
   ```rust
   fn create_security_settings(options: &HashMap<String, String>, config: &SaveConfig) -> SecuritySettings {
       let mut settings = SecuritySettings::default();
       
       // ...
       
       // Set new_feature from options or config
       settings.new_feature = options.contains_key("new-feature") || 
           config.get_metadata().get("new_feature").map_or(false, |v| v == "true");
       
       settings
   }
   ```
3. Implement the functionality for the new feature in `security.rs`.

## Testing

### Unit Tests

Each module includes unit tests that verify the functionality of individual components. To run the unit tests:

```bash
cargo test
```

### Integration Tests

Integration tests verify that all components work together correctly. There are three integration checkpoints:

1. **Integration Checkpoint 1**: Verifies that the filename generator and topic extractor work together correctly.
2. **Integration Checkpoint 2**: Verifies that the save command, filename generator, and topic extractor work together correctly.
3. **Integration Checkpoint 3**: Verifies that all components, including the advanced topic extractor and security features, work together correctly.

To run the integration tests:

```bash
cargo test --test integration
```

### Security Tests

Security tests verify that the security features work correctly. To run the security tests:

```bash
cargo test --test security
```

## Best Practices

1. **Follow the modular architecture**: Keep concerns separated and use the existing extension points.
2. **Write comprehensive tests**: Each new feature should have unit tests, integration tests, and security tests as appropriate.
3. **Handle errors gracefully**: Use the `Result` type and provide meaningful error messages.
4. **Document your code**: Add comments and documentation for new functions and modules.
5. **Consider security implications**: Any feature that deals with file operations should consider security implications.
6. **Maintain backward compatibility**: New features should not break existing functionality.
7. **Use the existing abstractions**: Use the existing abstractions like `TopicExtractorFn` and `FilenameFormat` when possible.

## Troubleshooting

### Common Issues

1. **File permission issues**: Check that the user has write permissions for the target directory.
2. **Path validation failures**: Check that the path is valid and does not contain invalid characters.
3. **Serialization errors**: Check that the conversation structure is valid and can be serialized to JSON.
4. **Topic extraction failures**: Check that the conversation has enough content for topic extraction.

### Debugging

1. **Enable debug logging**: Set the `RUST_LOG` environment variable to `debug` to enable debug logging.
2. **Use the `--verbose` flag**: Add the `--verbose` flag to the command to see more detailed output.
3. **Check the logs**: Check the logs in `~/.q/logs/` for more information about errors.

## API Documentation

For detailed API documentation, see the generated documentation:

```bash
cargo doc --open
```

This will generate and open the API documentation in your browser.
