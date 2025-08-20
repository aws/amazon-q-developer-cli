# Amazon Q CLI Automatic Naming Feature - API Documentation

## Conversation Module

### Conversation

```rust
pub struct Conversation {
    pub id: String,
    pub messages: Vec<Message>,
    pub metadata: HashMap<String, String>,
}
```

#### Methods

```rust
/// Create a new conversation with the given ID
pub fn new(id: String) -> Self

/// Add a user message to the conversation
pub fn add_user_message(&mut self, content: String) -> &mut Self

/// Add an assistant message to the conversation
pub fn add_assistant_message(&mut self, content: String, tool_calls: Option<Vec<ToolCall>>) -> &mut Self

/// Get all user messages in the conversation
pub fn user_messages(&self) -> Vec<&Message>

/// Get all assistant messages in the conversation
pub fn assistant_messages(&self) -> Vec<&Message>

/// Add metadata to the conversation
pub fn add_metadata(&mut self, key: &str, value: &str)

/// Get metadata from the conversation
pub fn get_metadata(&self, key: &str) -> Option<&str>
```

### Message

```rust
pub struct Message {
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}
```

### ToolCall

```rust
pub struct ToolCall {
    pub name: String,
    pub arguments: HashMap<String, serde_json::Value>,
}
```

## Topic Extractor Module

### Basic Extractor

```rust
/// Extract topics from a conversation using basic techniques
///
/// Returns a tuple of (main_topic, sub_topic, action_type)
pub fn extract_topics(conversation: &Conversation) -> (String, String, String)
```

### Enhanced Extractor

```rust
/// Extract topics from a conversation using enhanced techniques
///
/// Returns a tuple of (main_topic, sub_topic, action_type)
pub fn extract_topics(conversation: &Conversation) -> (String, String, String)

/// Extract keywords from a conversation
pub fn extract_keywords(conversation: &Conversation) -> Vec<String>

/// Analyze sentiment of a conversation
///
/// Returns a value between 0.0 (negative) and 1.0 (positive)
pub fn analyze_sentiment(conversation: &Conversation) -> f32

/// Detect the language of a conversation
///
/// Returns a language code (e.g., "en", "es", "fr")
pub fn detect_language(conversation: &Conversation) -> String
```

### Advanced Extractor

```rust
/// Extract topics from a conversation using advanced NLP techniques
///
/// Returns a tuple of (main_topic, sub_topic, action_type)
pub fn extract_topics(conversation: &Conversation) -> (String, String, String)

/// Extract keywords with language context awareness
fn extract_keywords_with_language(conversation: &Conversation, language: &str) -> Vec<String>

/// Extract technical terms from conversation
fn extract_technical_terms(conversation: &Conversation) -> Vec<String>

/// Analyze conversation structure to identify context
fn analyze_conversation_structure(conversation: &Conversation) -> HashMap<String, f32>

/// Perform topic modeling on a conversation with language context
fn perform_topic_modeling(conversation: &Conversation, language: &str) -> Vec<(String, f32)>

/// Apply latent semantic analysis
fn apply_latent_semantic_analysis(tf_idf_scores: &HashMap<String, f32>) -> HashMap<String, f32>

/// Determine the main topic from keywords with context awareness
fn determine_main_topic_with_context(keywords: &[String], context: &HashMap<String, f32>, language: &str) -> String

/// Determine the sub-topic from keywords with context awareness
fn determine_sub_topic_with_context(keywords: &[String], main_topic: &str, context: &HashMap<String, f32>, language: &str) -> String

/// Determine the action type from a conversation with context awareness
fn determine_action_type_with_context(conversation: &Conversation, context: &HashMap<String, f32>, language: &str) -> String

/// Refine topics for consistency and quality
fn refine_topics(main_topic: String, sub_topic: String, action_type: String, conversation: &Conversation) -> (String, String, String)
```

## Filename Generator Module

```rust
/// Type definition for topic extractor functions
pub type TopicExtractorFn = fn(&Conversation) -> (String, String, String);

/// Generate a filename for a conversation
pub fn generate_filename(conversation: &Conversation) -> String

/// Generate a filename for a conversation using a specific topic extractor
pub fn generate_filename_with_extractor(
    conversation: &Conversation,
    extractor: &TopicExtractorFn
) -> String

/// Generate a filename for a conversation using configuration settings
pub fn generate_filename_with_config(
    conversation: &Conversation,
    config: &SaveConfig
) -> String

/// Generate a filename for a conversation using a template
pub fn generate_filename_with_template(
    conversation: &Conversation,
    config: &SaveConfig,
    template_name: &str
) -> String

/// Get a topic extractor function by name
fn get_topic_extractor(name: &str) -> TopicExtractorFn

/// Format a date according to the specified format
fn format_date(date: &chrono::DateTime<chrono::Local>, format: &str) -> String

/// Sanitize a string for use in a filename
fn sanitize_for_filename(input: &str) -> String

/// Convert a month number to a three-letter abbreviation
fn month_to_abbr(month: u32) -> &'static str

/// Truncate a filename to a reasonable length
fn truncate_filename(filename: &str) -> String
```

## Save Configuration Module

```rust
/// Configuration for saving conversations
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

/// Format for generating filenames
pub enum FilenameFormat {
    /// Default format: Q_[MainTopic]_[SubTopic]_[ActionType] - DDMMMYY-HHMM
    Default,
    
    /// Custom format with placeholders
    Custom(String),
}

/// Create a new save configuration
pub fn new<P: AsRef<Path>>(config_path: P) -> Self

/// Get the default path for saving conversations
pub fn get_default_path(&self) -> String

/// Set the default path for saving conversations
pub fn set_default_path(&mut self, path: &str) -> io::Result<()>

/// Get the filename format
pub fn get_filename_format(&self) -> &FilenameFormat

/// Set the filename format
pub fn set_filename_format(&mut self, format: FilenameFormat) -> io::Result<()>

/// Get the prefix for filenames
pub fn get_prefix(&self) -> &str

/// Set the prefix for filenames
pub fn set_prefix(&mut self, prefix: &str) -> io::Result<()>

/// Get the separator for filename components
pub fn get_separator(&self) -> &str

/// Set the separator for filename components
pub fn set_separator(&mut self, separator: &str) -> io::Result<()>

/// Get the format for dates in filenames
pub fn get_date_format(&self) -> &str

/// Set the format for dates in filenames
pub fn set_date_format(&mut self, format: &str) -> io::Result<()>

/// Get the name of the topic extractor to use
pub fn get_topic_extractor_name(&self) -> &str

/// Set the name of the topic extractor to use
pub fn set_topic_extractor_name(&mut self, name: &str) -> io::Result<()>

/// Get a template for generating filenames
pub fn get_template(&self, name: &str) -> Option<&FilenameFormat>

/// Add a template for generating filenames
pub fn add_template(&mut self, name: &str, format: FilenameFormat) -> io::Result<()>

/// Remove a template for generating filenames
pub fn remove_template(&mut self, name: &str) -> io::Result<()>

/// Get all templates for generating filenames
pub fn get_templates(&self) -> &HashMap<String, FilenameFormat>

/// Get custom metadata for saved files
pub fn get_metadata(&self) -> &HashMap<String, String>

/// Add custom metadata for saved files
pub fn add_metadata(&mut self, key: &str, value: &str) -> io::Result<()>

/// Remove custom metadata for saved files
pub fn remove_metadata(&mut self, key: &str) -> io::Result<()>

/// Ensure the default path exists
pub fn ensure_default_path_exists(&self) -> io::Result<()>

/// Check if a path exists and is writable
pub fn is_path_writable<P: AsRef<Path>>(&self, path: P) -> bool

/// Create directories for a path if they don't exist
pub fn create_dirs_for_path<P: AsRef<Path>>(&self, path: P) -> io::Result<()>

/// Convert configuration to JSON
pub fn to_json(&self) -> Result<String, String>

/// Create configuration from JSON
pub fn from_json(json: &str) -> Result<Self, String>

/// Save configuration to a file
pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> io::Result<()>

/// Load configuration from a file
pub fn load_from_file<P: AsRef<Path>>(path: P) -> io::Result<Self>
```

## Save Command Module

```rust
/// Error type for save command operations
pub enum SaveError {
    /// I/O error
    Io(io::Error),
    /// Invalid path
    InvalidPath(String),
    /// Serialization error
    Serialization(serde_json::Error),
    /// Configuration error
    Config(String),
    /// Security error
    Security(SecurityError),
}

/// Handle the save command
pub fn handle_save_command(
    args: &[String],
    conversation: &Conversation,
    config: &SaveConfig,
) -> Result<String, SaveError>

/// Handle the save command with a specific topic extractor
pub fn handle_save_command_with_extractor(
    args: &[String],
    conversation: &Conversation,
    config: &SaveConfig,
    extractor: &fn(&Conversation) -> (String, String, String),
) -> Result<String, SaveError>

/// Parse save command options
fn parse_save_options(args: &[String]) -> (Vec<String>, HashMap<String, String>)

/// Create security settings from options and config
fn create_security_settings(options: &HashMap<String, String>, config: &SaveConfig) -> SecuritySettings

/// Save a conversation to a file
pub fn save_conversation_to_file(
    conversation: &Conversation,
    path: &Path,
    config: &SaveConfig,
    options: &HashMap<String, String>,
    security_settings: &SecuritySettings,
) -> Result<(), SaveError>

/// Redact sensitive information from a conversation
fn redact_conversation(conversation: &Conversation) -> Conversation
```

## Security Module

```rust
/// Security settings for file operations
pub struct SecuritySettings {
    /// Whether to redact sensitive information
    pub redact_sensitive: bool,
    
    /// Whether to prevent overwriting existing files
    pub prevent_overwrite: bool,
    
    /// File permissions to set (Unix mode)
    pub file_permissions: u32,
    
    /// Directory permissions to set (Unix mode)
    pub directory_permissions: u32,
    
    /// Maximum allowed path depth
    pub max_path_depth: usize,
    
    /// Whether to follow symlinks
    pub follow_symlinks: bool,
}

/// Error type for security operations
pub enum SecurityError {
    /// I/O error
    Io(io::Error),
    /// Path traversal attempt
    PathTraversal(PathBuf),
    /// File already exists
    FileExists(PathBuf),
    /// Path too deep
    PathTooDeep(PathBuf),
    /// Invalid path
    InvalidPath(String),
    /// Symlink not allowed
    SymlinkNotAllowed(PathBuf),
}

/// Validate and secure a file path
pub fn validate_path(path: &Path, settings: &SecuritySettings) -> Result<PathBuf, SecurityError>

/// Create a directory with secure permissions
pub fn create_secure_directory(path: &Path, settings: &SecuritySettings) -> Result<(), SecurityError>

/// Write to a file with secure permissions
pub fn write_secure_file(path: &Path, content: &str, settings: &SecuritySettings) -> Result<(), SecurityError>

/// Redact sensitive information from text
pub fn redact_sensitive_information(text: &str) -> String

/// Generate a unique filename to avoid overwriting
pub fn generate_unique_filename(path: &Path) -> PathBuf
```

## Command Registry Module

```rust
/// Command registry for registering and executing commands
pub struct CommandRegistry {
    commands: HashMap<String, Command>,
}

/// Command information
struct Command {
    name: String,
    description: String,
    handler: Box<dyn Fn(&[String], &Conversation, &SaveConfig) -> Result<String, SaveError>>,
}

/// Create a new command registry
pub fn new() -> Self

/// Register a command
pub fn register_command<F>(
    &mut self,
    name: &str,
    description: &str,
    handler: F,
) where
    F: Fn(&[String], &Conversation, &SaveConfig) -> Result<String, SaveError> + 'static

/// Execute a command
pub fn execute_command(
    &self,
    name: &str,
    args: &[String],
    conversation: &Conversation,
    config: &SaveConfig,
) -> Result<String, SaveError>

/// Get command help
pub fn get_command_help(&self, name: &str) -> Option<String>

/// Get all command help
pub fn get_all_command_help(&self) -> Vec<(String, String)>
```

## Integration Checkpoints

### Integration Checkpoint 1

```rust
/// Run the integration checkpoint
pub fn run_integration_checkpoint() -> Result<(), String>

/// Example usage of the integration checkpoint
pub fn example_usage()

/// Document issues found during the integration checkpoint
pub fn document_issues()
```

### Integration Checkpoint 2

```rust
/// Run the integration checkpoint
pub fn run_integration_checkpoint() -> Result<(), String>

/// Example usage of the integration checkpoint
pub fn example_usage()

/// Document issues found during the integration checkpoint
pub fn document_issues()
```

### Integration Checkpoint 3

```rust
/// Run the integration checkpoint
pub fn run_integration_checkpoint() -> Result<(), String>

/// Example usage of the integration checkpoint
pub fn example_usage()

/// Document issues found during the integration checkpoint
pub fn document_issues()
```

## Error Handling

### SaveError

```rust
pub enum SaveError {
    /// I/O error
    Io(io::Error),
    /// Invalid path
    InvalidPath(String),
    /// Serialization error
    Serialization(serde_json::Error),
    /// Configuration error
    Config(String),
    /// Security error
    Security(SecurityError),
}
```

### SecurityError

```rust
pub enum SecurityError {
    /// I/O error
    Io(io::Error),
    /// Path traversal attempt
    PathTraversal(PathBuf),
    /// File already exists
    FileExists(PathBuf),
    /// Path too deep
    PathTooDeep(PathBuf),
    /// Invalid path
    InvalidPath(String),
    /// Symlink not allowed
    SymlinkNotAllowed(PathBuf),
}
```

## Constants

```rust
/// Maximum filename length
const MAX_FILENAME_LENGTH: usize = 255;

/// Default file permissions (rw-------)
const DEFAULT_FILE_PERMISSIONS: u32 = 0o600;

/// Default directory permissions (rwx------)
const DEFAULT_DIRECTORY_PERMISSIONS: u32 = 0o700;

/// Default maximum path depth
const DEFAULT_MAX_PATH_DEPTH: usize = 10;
```

## Type Definitions

```rust
/// Type definition for topic extractor functions
pub type TopicExtractorFn = fn(&Conversation) -> (String, String, String);
```

## Traits

```rust
/// Trait for objects that can be converted to JSON
pub trait ToJson {
    /// Convert to JSON
    fn to_json(&self) -> Result<String, String>;
}

/// Trait for objects that can be created from JSON
pub trait FromJson: Sized {
    /// Create from JSON
    fn from_json(json: &str) -> Result<Self, String>;
}
```

## Enums

```rust
/// Format for generating filenames
pub enum FilenameFormat {
    /// Default format: Q_[MainTopic]_[SubTopic]_[ActionType] - DDMMMYY-HHMM
    Default,
    
    /// Custom format with placeholders
    Custom(String),
}
```

## Structs

```rust
/// Conversation
pub struct Conversation {
    pub id: String,
    pub messages: Vec<Message>,
    pub metadata: HashMap<String, String>,
}

/// Message
pub struct Message {
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// Tool Call
pub struct ToolCall {
    pub name: String,
    pub arguments: HashMap<String, serde_json::Value>,
}

/// Save Configuration
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

/// Security Settings
pub struct SecuritySettings {
    pub redact_sensitive: bool,
    pub prevent_overwrite: bool,
    pub file_permissions: u32,
    pub directory_permissions: u32,
    pub max_path_depth: usize,
    pub follow_symlinks: bool,
}

/// Command Registry
pub struct CommandRegistry {
    commands: HashMap<String, Command>,
}

/// Command
struct Command {
    name: String,
    description: String,
    handler: Box<dyn Fn(&[String], &Conversation, &SaveConfig) -> Result<String, SaveError>>,
}
```
