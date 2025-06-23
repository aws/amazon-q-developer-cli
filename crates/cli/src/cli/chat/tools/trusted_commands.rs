use regex::Regex;
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::warn;
use glob::Pattern;

use crate::platform::Context;
use super::trusted_commands_config::{load_config, locate_config_file};

/// Represents a trusted command configuration entry.
/// 
/// This enum defines the two types of trusted command patterns that users can configure:
/// 1. Match: A glob-style pattern that supports wildcards (*) for flexible matching
/// 2. Regex: A regular expression pattern for more complex matching rules
/// 
/// # Security Considerations
/// - Even if a command matches a trusted pattern, it will still require acceptance if it contains
///   dangerous patterns like redirections (>), command substitutions ($(), `cmd`), or chained commands (&&, ||)
/// - The trusted commands feature is designed as a convenience for frequently used safe commands,
///   not as a security boundary
/// - Users should be careful about which commands they add to their trusted list
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum TrustedCommand {
    /// A glob-style pattern match
    #[serde(rename = "match")]
    Match {
        /// The glob-style pattern to match commands
        command: String,
        /// Optional description for documentation purposes
        description: Option<String>,
    },
    /// A regex pattern to match commands
    #[serde(rename = "regex")]
    Regex {
        /// The regex pattern string to match commands
        command: String,
        /// Optional description for documentation purposes
        description: Option<String>,
    },
}

/// The root configuration structure for trusted commands.
/// 
/// This structure represents the JSON configuration file that users can create
/// to define their trusted commands. The file should be located at
/// `~/.aws/amazonq/trusted_commands.json`.
/// 
/// # Example JSON Configuration
/// ```json
/// {
///   "trusted_commands": [
///     {
///       "type": "match",
///       "command": "npm *",
///       "description": "All npm commands"
///     },
///     {
///       "type": "regex",
///       "command": "^git\\s+(status|log|diff)",
///       "description": "Git read-only commands"
///     }
///   ]
/// }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct TrustedCommandsConfig {
    /// List of trusted commands defined by the user
    pub trusted_commands: Vec<TrustedCommand>,
}

/// Processed configuration for efficient command matching.
/// 
/// This structure contains the processed version of the user's trusted commands configuration,
/// with compiled regex patterns and compiled glob patterns for efficient lookup.
/// 
/// It's used by the CLI to quickly determine if a command is trusted without
/// having to reprocess the configuration each time.
#[derive(Debug, Clone)]
pub struct ProcessedTrustedCommands {
    /// List of compiled glob patterns with their descriptions
    compiled_patterns: Vec<(Pattern, Option<String>)>,
    /// List of compiled regex patterns with their descriptions
    regex_patterns: Vec<(Regex, Option<String>)>,
}

impl ProcessedTrustedCommands {
    /// Create a new processed configuration from the raw config.
    /// 
    /// This compiles regex patterns and glob patterns for efficient lookup.
    /// Invalid patterns are ignored with a warning.
    pub fn new(config: TrustedCommandsConfig) -> Self {
        let mut compiled_patterns = Vec::new();
        let mut regex_patterns = Vec::new();

        for trusted_command in config.trusted_commands {
            match trusted_command {
                TrustedCommand::Match { command, description } => {
                    match Pattern::new(&command) {
                        Ok(pattern) => {
                            compiled_patterns.push((pattern, description));
                        }
                        Err(err) => {
                            // Log the error but continue processing other patterns
                            warn!("Invalid glob pattern in trusted commands: {}, error: {}", command, err);
                        }
                    }
                }
                TrustedCommand::Regex { command, description } => {
                    match Regex::new(&command) {
                        Ok(regex) => {
                            regex_patterns.push((regex, description));
                        }
                        Err(err) => {
                            // Log the error but continue processing other patterns
                            warn!("Invalid regex pattern in trusted commands: {}, error: {}", command, err);
                        }
                    }
                }
            }
        }

        Self {
            compiled_patterns,
            regex_patterns,
        }
    }

    /// Check if a command is trusted according to the configuration.
    pub fn is_trusted(&self, command: &str) -> bool {
        // Check compiled glob patterns first
        for (pattern, _) in &self.compiled_patterns {
            if pattern.matches(command) {
                return true;
            }
        }

        // Then check regex patterns
        for (pattern, _) in &self.regex_patterns {
            if pattern.is_match(command) {
                return true;
            }
        }

        false
    }
    
    // The glob_match function has been replaced with the glob crate's Pattern::matches method
}

/// Cache entry for trusted commands configuration
struct CacheEntry {
    /// The processed configuration
    config: Option<ProcessedTrustedCommands>,
    /// When the configuration was last loaded
    last_loaded: Instant,
}

/// Global cache for trusted commands configuration
struct TrustedCommandsCache {
    /// The cached configuration
    entry: Option<CacheEntry>,
    /// How long to keep the cache valid
    ttl: Duration,
}

impl TrustedCommandsCache {
    /// Create a new cache with the specified TTL
    fn new(ttl: Duration) -> Self {
        Self {
            entry: None,
            ttl,
        }
    }

    /// Check if the cache is valid
    fn is_valid(&self) -> bool {
        match &self.entry {
            Some(entry) => entry.last_loaded.elapsed() < self.ttl,
            None => false,
        }
    }

    // We're not using a separate get method anymore since we check validity inline
    // in the is_command_trusted function

    /// Update the cache with a new configuration
    fn update(&mut self, config: Option<ProcessedTrustedCommands>) {
        self.entry = Some(CacheEntry {
            config,
            last_loaded: Instant::now(),
        });
    }
}

// Default TTL for the cache (1 minute)
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(60);

// Use lazy_static to create a thread-safe global cache
use lazy_static::lazy_static;

lazy_static! {
    static ref CACHE: Arc<Mutex<TrustedCommandsCache>> = Arc::new(Mutex::new(
        TrustedCommandsCache::new(DEFAULT_CACHE_TTL)
    ));
}

// We can use CACHE directly

/// Check if a command is trusted according to the user's configuration.
///
/// This function uses a cached configuration if available and valid,
/// otherwise it loads the configuration from disk.
///
/// # Security Model
/// The trusted commands feature follows these security principles:
/// 1. Defense in depth: Even if a command is in the trusted list, it will still be checked for dangerous patterns
/// 2. Least privilege: By default, no commands are trusted beyond the built-in read-only commands
/// 3. User control: Users explicitly define which commands they trust in their configuration
/// 4. Fail-safe defaults: If the configuration file is missing or invalid, the system falls back to requiring acceptance
///
/// # Caching
/// For performance reasons, the trusted commands configuration is cached in memory with a TTL (Time To Live).
/// This means that changes to the configuration file may not take effect immediately.
///
/// # Arguments
///
/// * `ctx` - The platform context
/// * `command` - The command to check
///
/// # Returns
///
/// `true` if the command is trusted, `false` otherwise
pub async fn is_command_trusted(ctx: &Context, command: &str) -> bool {
    // Try to get the configuration from the cache
    {
        let cache_guard = CACHE.lock().unwrap();
        if let Some(entry) = &cache_guard.entry {
            if cache_guard.is_valid() {
                if let Some(config) = &entry.config {
                    return config.is_trusted(command);
                }
            }
        }
    }
    
    // Cache miss, load the configuration from disk
    let config_path = locate_config_file(ctx);
    let config_result = load_config(ctx, &config_path).await;
    
    // Update the cache with the new configuration
    let mut cache_guard = CACHE.lock().unwrap();
    cache_guard.update(config_result.ok());
    
    // Check if the command is trusted
    if let Some(entry) = &cache_guard.entry {
        if let Some(config) = &entry.config {
            return config.is_trusted(command);
        }
    }
    
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_trusted_command_match() {
        let json = r#"{
            "type": "match",
            "command": "npm *",
            "description": "All npm commands"
        }"#;

        let command: TrustedCommand = serde_json::from_str(json).unwrap();
        match command {
            TrustedCommand::Match { command, description } => {
                assert_eq!(command, "npm *");
                assert_eq!(description, Some("All npm commands".to_string()));
            }
            _ => panic!("Expected Match variant"),
        }
    }

    #[test]
    fn test_deserialize_trusted_command_regex() {
        let json = r#"{
            "type": "regex",
            "command": "^git\\s+commit(\\s+(-[am]|--all|--message)(\\s+[\"'][^\"']*[\"'])?)*$",
            "description": "Git commit commands"
        }"#;

        let command: TrustedCommand = serde_json::from_str(json).unwrap();
        match command {
            TrustedCommand::Regex { command, description } => {
                assert_eq!(command, "^git\\s+commit(\\s+(-[am]|--all|--message)(\\s+[\"'][^\"']*[\"'])?)*$");
                assert_eq!(description, Some("Git commit commands".to_string()));
            }
            _ => panic!("Expected Regex variant"),
        }
    }

    #[test]
    fn test_deserialize_trusted_commands_config() {
        let json = r#"{
            "trusted_commands": [
                {
                    "type": "match",
                    "command": "make",
                    "description": "Run makefile"
                },
                {
                    "type": "regex",
                    "command": "^git\\s+commit(\\s+(-[am]|--all|--message)(\\s+[\"'][^\"']*[\"'])?)*$",
                    "description": "Git commit commands"
                }
            ]
        }"#;

        let config: TrustedCommandsConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.trusted_commands.len(), 2);
    }

    #[test]
    fn test_processed_trusted_commands() {
        let config = TrustedCommandsConfig {
            trusted_commands: vec![
                TrustedCommand::Match {
                    command: "make".to_string(),
                    description: Some("Run makefile".to_string()),
                },
                TrustedCommand::Match {
                    command: "npm *".to_string(),
                    description: Some("All npm commands".to_string()),
                },
                TrustedCommand::Regex {
                    command: "^git\\s+commit(\\s+(-[am]|--all|--message)(\\s+[\"'][^\"']*[\"'])?)*$".to_string(),
                    description: Some("Git commit commands".to_string()),
                },
                TrustedCommand::Regex {
                    command: "[".to_string(), // Invalid regex
                    description: None,
                },
            ],
        };

        let processed = ProcessedTrustedCommands::new(config);
        
        // Check exact match using glob pattern
        assert!(processed.is_trusted("make"));
        assert!(!processed.is_trusted("make all"));
        
        // Check glob match
        assert!(processed.is_trusted("npm install"));
        assert!(processed.is_trusted("npm run build"));
        assert!(!processed.is_trusted("yarn install"));
        
        // Check regex match
        assert!(processed.is_trusted("git commit"));
        assert!(processed.is_trusted("git commit -m 'test'"));
        assert!(processed.is_trusted("git commit --message 'test commit'"));
        assert!(!processed.is_trusted("git push"));
        
        // Invalid regex should be ignored
        assert_eq!(processed.regex_patterns.len(), 1);
    }
    
    #[test]
    fn test_glob_pattern_matching() {
        // Test basic pattern matching
        assert!(Pattern::new("npm *").unwrap().matches("npm install"));
        assert!(Pattern::new("npm *").unwrap().matches("npm run build"));
        assert!(!Pattern::new("npm *").unwrap().matches("yarn install"));
        
        // Test suffix matching
        assert!(Pattern::new("* install").unwrap().matches("npm install"));
        assert!(Pattern::new("* install").unwrap().matches("yarn install"));
        assert!(!Pattern::new("* install").unwrap().matches("npm run build"));
        
        // Test middle wildcard
        assert!(Pattern::new("git * commit").unwrap().matches("git add . && git commit"));
        assert!(!Pattern::new("git * commit").unwrap().matches("git push"));
        
        // Test exact match (no wildcards)
        assert!(Pattern::new("npm run build").unwrap().matches("npm run build"));
        assert!(!Pattern::new("npm run build").unwrap().matches("npm run test"));
        
        // Test wildcard only
        assert!(Pattern::new("*").unwrap().matches("any command"));
        
        // Test multiple wildcards - this is the key improvement over our custom implementation
        assert!(Pattern::new("git * commit*").unwrap().matches("git add . && git commit -m 'test'"));
        assert!(Pattern::new("git*commit*").unwrap().matches("git commit -m 'test'"));
        assert!(Pattern::new("*git*commit*").unwrap().matches("cd repo && git add . && git commit -m 'test'"));
        assert!(!Pattern::new("git*commit*").unwrap().matches("git push origin main"));
    }
    
    #[test]
    fn test_trusted_commands_config() {
        // Create a sample configuration
        let config = TrustedCommandsConfig {
            trusted_commands: vec![
                TrustedCommand::Match {
                    command: "npm *".to_string(),
                    description: Some("All npm commands".to_string()),
                },
                TrustedCommand::Match {
                    command: "git*commit*".to_string(),
                    description: Some("Git commit commands with any arguments".to_string()),
                },
                TrustedCommand::Regex {
                    command: "^git (status|log|diff)".to_string(),
                    description: Some("Git read-only commands".to_string()),
                },
            ],
        };

        // Process the configuration
        let processed = ProcessedTrustedCommands::new(config);

        // Test glob matches
        assert!(processed.is_trusted("npm run build"));
        assert!(processed.is_trusted("npm run test"));
        assert!(processed.is_trusted("npm install"));
        assert!(!processed.is_trusted("yarn install"));
        
        // Test multiple wildcard matches
        assert!(processed.is_trusted("git add . && git commit -m 'test'"));
        assert!(processed.is_trusted("git commit -m 'test'"));
        assert!(!processed.is_trusted("git push origin main"));

        // Test regex matches
        assert!(processed.is_trusted("git status"));
        assert!(processed.is_trusted("git log"));
        assert!(processed.is_trusted("git diff"));
        assert!(!processed.is_trusted("git push"));
    }

    #[test]
    fn test_invalid_regex() {
        // Create a configuration with an invalid regex
        let config = TrustedCommandsConfig {
            trusted_commands: vec![
                TrustedCommand::Match {
                    command: "npm *".to_string(),
                    description: Some("All npm commands".to_string()),
                },
                TrustedCommand::Regex {
                    command: "[".to_string(), // Invalid regex
                    description: Some("Invalid regex pattern".to_string()),
                },
            ],
        };

        // Process the configuration - should not panic
        let processed = ProcessedTrustedCommands::new(config);

        // The invalid regex should be ignored
        assert!(processed.is_trusted("npm run build"));
        assert!(!processed.is_trusted("[")); // Should not match the invalid regex
    }

    #[test]
    fn test_json_deserialization() {
        let json = r#"{
            "trusted_commands": [
                {
                    "type": "match",
                    "command": "npm *",
                    "description": "All npm commands"
                },
                {
                    "type": "regex",
                    "command": "^git (status|log|diff)",
                    "description": "Git read-only commands"
                }
            ]
        }"#;

        let config: TrustedCommandsConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.trusted_commands.len(), 2);

        let processed = ProcessedTrustedCommands::new(config);
        assert!(processed.is_trusted("npm run build"));
        assert!(processed.is_trusted("npm install"));
        assert!(processed.is_trusted("git status"));
        assert!(!processed.is_trusted("git push"));
    }

    #[tokio::test]
    async fn test_cache_functionality() {
        // Create a test context
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        
        // Create a test configuration file
        let config_path = locate_config_file(&ctx);
        let dir_path = config_path.parent().unwrap();
        ctx.fs().create_dir_all(dir_path).await.unwrap();
        
        let valid_json = r#"{
            "trusted_commands": [
                {
                    "type": "match",
                    "command": "npm run build",
                    "description": "Build the project"
                }
            ]
        }"#;
        
        ctx.fs().write(&config_path, valid_json).await.unwrap();
        
        // Reset the cache for testing
        {
            let mut cache_guard = CACHE.lock().unwrap();
            cache_guard.entry = None;
        }
        
        // First call should load from disk
        assert!(is_command_trusted(&ctx, "npm run build").await);
        
        // Second call should use the cache
        assert!(is_command_trusted(&ctx, "npm run build").await);
        
        // Modify the file - this shouldn't affect the cached result yet
        let updated_json = r#"{
            "trusted_commands": [
                {
                    "type": "match",
                    "command": "npm run test",
                    "description": "Run tests"
                }
            ]
        }"#;
        
        ctx.fs().write(&config_path, updated_json).await.unwrap();
        
        // Should still return true for "npm run build" because it's using the cached value
        assert!(is_command_trusted(&ctx, "npm run build").await);
        
        // Should return false for "npm run test" because the cache hasn't been updated
        assert!(!is_command_trusted(&ctx, "npm run test").await);
        
        // Force cache invalidation by manipulating the last_loaded time
        {
            let mut cache_guard = CACHE.lock().unwrap();
            if let Some(entry) = &mut cache_guard.entry {
                entry.last_loaded = Instant::now() - Duration::from_secs(600); // 10 minutes ago
            }
        }
        
        // Now it should load from disk again and return the updated values
        assert!(!is_command_trusted(&ctx, "npm run build").await);
        assert!(is_command_trusted(&ctx, "npm run test").await);
    }
}