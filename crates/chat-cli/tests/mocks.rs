// tests/mocks.rs
// Mock objects for testing the Amazon Q CLI automatic naming feature

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use crate::conversation::{Conversation, Message};

/// Creates a mock conversation for testing
pub fn create_mock_conversation(conversation_type: &str) -> Conversation {
    match conversation_type {
        "empty" => Conversation::new("test-empty".to_string()),
        
        "simple" => {
            let mut conv = Conversation::new("test-simple".to_string());
            conv.add_user_message("Hello".to_string())
                .add_assistant_message("Hi there! How can I help you today?".to_string(), None);
            conv
        },
        
        "amazon_q_cli" => {
            let mut conv = Conversation::new("test-amazon-q-cli".to_string());
            conv.add_user_message("I need help with Amazon Q CLI".to_string())
                .add_assistant_message("Sure, what do you want to know about Amazon Q CLI?".to_string(), None)
                .add_user_message("How do I save conversations automatically?".to_string())
                .add_assistant_message("Currently, you need to use the /save command with a filename.".to_string(), None)
                .add_user_message("Can we make it automatic?".to_string())
                .add_assistant_message("That would require implementing a new feature. Let me explain how it could work...".to_string(), None);
            conv
        },
        
        "feature_request" => {
            let mut conv = Conversation::new("test-feature-request".to_string());
            conv.add_user_message("I think Amazon Q CLI should automatically name saved conversations".to_string())
                .add_assistant_message("That's an interesting feature request. How would you like it to work?".to_string(), None)
                .add_user_message("It should generate names based on the conversation content".to_string())
                .add_assistant_message("That makes sense. The system could analyze the conversation to extract key topics and use those for the filename.".to_string(), None)
                .add_user_message("Yes, exactly! And it should include the date and time too.".to_string())
                .add_assistant_message("Great idea. So the format could be something like 'Q_[Topic]_[Subtopic]_[Type] - DDMMMYY-HHMM'.".to_string(), None);
            conv
        },
        
        "technical" => {
            let mut conv = Conversation::new("test-technical".to_string());
            conv.add_user_message("How do I implement a Rust function to parse JSON?".to_string())
                .add_assistant_message(r#"
You can use the serde_json crate. Here's an example:

```rust
use serde_json::{Result, Value};

fn parse_json(json_str: &str) -> Result<Value> {
    let v: Value = serde_json::from_str(json_str)?;
    Ok(v)
}
```

This function takes a JSON string and returns a `Result` containing the parsed JSON value.
"#.to_string(), None)
                .add_user_message("Can you show me how to handle errors?".to_string())
                .add_assistant_message(r#"
Sure, here's how you can handle errors:

```rust
fn parse_json_with_error_handling(json_str: &str) -> Result<Value> {
    match serde_json::from_str(json_str) {
        Ok(v) => Ok(v),
        Err(e) => {
            eprintln!("Error parsing JSON: {}", e);
            Err(e)
        }
    }
}
```

This will print the error message before returning the error.
"#.to_string(), None);
            conv
        },
        
        "multi_topic" => {
            let mut conv = Conversation::new("test-multi-topic".to_string());
            conv.add_user_message("How do I use Amazon Q CLI?".to_string())
                .add_assistant_message("Amazon Q CLI is a command-line interface for interacting with Amazon Q. You can install it and use various commands.".to_string(), None)
                .add_user_message("What about AWS Lambda functions?".to_string())
                .add_assistant_message("AWS Lambda is a serverless compute service that lets you run code without provisioning or managing servers.".to_string(), None)
                .add_user_message("Can I use Amazon Q CLI with Lambda?".to_string())
                .add_assistant_message("Yes, you can use Amazon Q CLI to get help with Lambda function development and deployment.".to_string(), None);
            conv
        },
        
        "very_long" => {
            let mut conv = Conversation::new("test-very-long".to_string());
            let long_text = "A".repeat(5000);
            conv.add_user_message(format!("Here's a long text: {}", long_text))
                .add_assistant_message("That's indeed a very long text.".to_string(), None);
            conv
        },
        
        _ => Conversation::new("test-default".to_string()),
    }
}

/// Mock file system for testing
pub struct MockFileSystem {
    files: HashMap<PathBuf, Vec<u8>>,
    directories: Vec<PathBuf>,
    error: Option<io::Error>,
}

impl MockFileSystem {
    /// Create a new mock file system
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            directories: vec![PathBuf::from("/")],
            error: None,
        }
    }
    
    /// Set an error to be returned by file operations
    pub fn set_error(&mut self, error: Option<io::Error>) {
        self.error = error;
    }
    
    /// Write to a file
    pub fn write_file(&mut self, path: &PathBuf, content: &[u8]) -> io::Result<()> {
        if let Some(ref err) = self.error {
            return Err(io::Error::new(err.kind(), err.to_string()));
        }
        
        // Check if the parent directory exists
        if let Some(parent) = path.parent() {
            if !self.directory_exists(parent) {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Directory not found: {}", parent.display())
                ));
            }
        }
        
        self.files.insert(path.clone(), content.to_vec());
        Ok(())
    }
    
    /// Read from a file
    pub fn read_file(&self, path: &PathBuf) -> io::Result<Vec<u8>> {
        if let Some(ref err) = self.error {
            return Err(io::Error::new(err.kind(), err.to_string()));
        }
        
        self.files.get(path).cloned().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("File not found: {}", path.display())
            )
        })
    }
    
    /// Create a directory
    pub fn create_directory(&mut self, path: &PathBuf) -> io::Result<()> {
        if let Some(ref err) = self.error {
            return Err(io::Error::new(err.kind(), err.to_string()));
        }
        
        // Check if the parent directory exists
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !self.directory_exists(parent) {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Parent directory not found: {}", parent.display())
                ));
            }
        }
        
        self.directories.push(path.clone());
        Ok(())
    }
    
    /// Check if a directory exists
    pub fn directory_exists(&self, path: &PathBuf) -> bool {
        self.directories.contains(path)
    }
    
    /// Check if a file exists
    pub fn file_exists(&self, path: &PathBuf) -> bool {
        self.files.contains_key(path)
    }
}

/// Mock configuration system for testing
pub struct MockConfigSystem {
    config: HashMap<String, String>,
    error: Option<io::Error>,
}

impl MockConfigSystem {
    /// Create a new mock configuration system
    pub fn new() -> Self {
        let mut config = HashMap::new();
        config.insert("save.default_path".to_string(), "~/qChats".to_string());
        
        Self {
            config,
            error: None,
        }
    }
    
    /// Set an error to be returned by configuration operations
    pub fn set_error(&mut self, error: Option<io::Error>) {
        self.error = error;
    }
    
    /// Get a configuration value
    pub fn get(&self, key: &str) -> io::Result<Option<String>> {
        if let Some(ref err) = self.error {
            return Err(io::Error::new(err.kind(), err.to_string()));
        }
        
        Ok(self.config.get(key).cloned())
    }
    
    /// Set a configuration value
    pub fn set(&mut self, key: &str, value: &str) -> io::Result<()> {
        if let Some(ref err) = self.error {
            return Err(io::Error::new(err.kind(), err.to_string()));
        }
        
        self.config.insert(key.to_string(), value.to_string());
        Ok(())
    }
}
