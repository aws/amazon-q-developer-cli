use std::path::{Path, PathBuf};
use eyre::{Result, Context as EyreContext};
use tracing::{debug, warn};
use tokio::io::AsyncReadExt;

use crate::platform::Context;
use super::trusted_commands::{TrustedCommandsConfig, ProcessedTrustedCommands};

/// Default filename for the trusted commands configuration.
/// 
/// The configuration file is expected to be at `~/.aws/amazonq/trusted_commands.json`.
pub const TRUSTED_COMMANDS_CONFIG_FILENAME: &str = "trusted_commands.json";

/// Locates the trusted commands configuration file.
///
/// The configuration file is expected to be at `~/.aws/amazonq/trusted_commands.json`.
/// This function handles platform-specific path differences.
///
/// # Arguments
///
/// * `ctx` - The platform context
///
/// # Returns
///
/// The path to the configuration file
pub fn locate_config_file(ctx: &Context) -> PathBuf {
    // Get the home directory from the context
    let home_dir = ctx.env().home().unwrap_or_else(|| {
        debug!("Home directory not found, using current directory");
        PathBuf::from(".")
    });

    // Construct the path to the configuration file
    home_dir.join(".aws").join("amazonq").join(TRUSTED_COMMANDS_CONFIG_FILENAME)
}

/// Checks if the configuration file exists.
///
/// # Arguments
///
/// * `config_path` - The path to the configuration file
///
/// # Returns
///
/// `true` if the file exists, `false` otherwise
pub async fn config_file_exists(ctx: &Context, config_path: &Path) -> bool {
    ctx.fs().exists(config_path)
}

/// Loads and parses the trusted commands configuration file.
///
/// # Arguments
///
/// * `ctx` - The platform context
/// * `config_path` - The path to the configuration file
///
/// # Returns
///
/// The processed trusted commands configuration if successful, or an error if the file
/// cannot be read or parsed.
pub async fn load_config(ctx: &Context, config_path: &Path) -> Result<ProcessedTrustedCommands> {
    // Check if the file exists
    if !config_file_exists(ctx, config_path).await {
        debug!("Trusted commands configuration file not found at: {}", config_path.display());
        return Err(eyre::eyre!("Configuration file not found"));
    }

    // Check file permissions
    check_file_permissions(ctx, config_path).await?;

    // Read the file content
    let content = read_config_file(ctx, config_path).await
        .wrap_err_with(|| format!("Failed to read trusted commands configuration file: {}", config_path.display()))?;

    // Parse the JSON content
    let config: TrustedCommandsConfig = parse_config(&content)
        .wrap_err_with(|| format!("Failed to parse trusted commands configuration file: {}", config_path.display()))?;

    // Process the configuration
    let processed_config = ProcessedTrustedCommands::new(config);
    debug!("Successfully loaded trusted commands configuration from: {}", config_path.display());

    Ok(processed_config)
}

/// Reads the content of the configuration file.
///
/// # Arguments
///
/// * `ctx` - The platform context
/// * `config_path` - The path to the configuration file
///
/// # Returns
///
/// The content of the file as a string if successful, or an error if the file cannot be read.
async fn read_config_file(ctx: &Context, config_path: &Path) -> Result<String> {
    let mut file = ctx.fs().open(config_path).await
        .wrap_err_with(|| format!("Failed to open configuration file: {}", config_path.display()))?;
    
    let mut content = String::new();
    file.read_to_string(&mut content).await
        .wrap_err_with(|| format!("Failed to read configuration file: {}", config_path.display()))?;
    
    Ok(content)
}

/// Parses the JSON content of the configuration file.
///
/// # Arguments
///
/// * `content` - The content of the configuration file as a string
///
/// # Returns
///
/// The parsed configuration if successful, or an error if the content cannot be parsed.
fn parse_config(content: &str) -> Result<TrustedCommandsConfig> {
    serde_json::from_str(content)
        .wrap_err("Invalid JSON format in trusted commands configuration file")
}

/// Checks if the configuration file has secure permissions.
///
/// The file should be readable and writable only by the owner.
/// On Unix systems, this means the file should have permissions 600 (rw-------)
/// or 400 (r--------).
///
/// # Security Considerations
/// Checking file permissions is an important security measure because the trusted commands
/// configuration determines which commands can be executed without user confirmation.
/// If other users can modify this file, they could potentially add malicious commands
/// to the trusted list.
///
/// This function doesn't prevent the configuration from being loaded if the permissions
/// are insecure, but it does log a warning to alert the user to the potential security risk.
///
/// # Platform-Specific Behavior
/// - On Unix systems: Checks if the file has permissions that allow group or others to read or write
/// - On Windows: Logs a general security recommendation (detailed permission checking not implemented)
///
/// # Arguments
///
/// * `ctx` - The platform context
/// * `config_path` - The path to the configuration file
///
/// # Returns
///
/// Ok if the permissions are secure, or a warning if they are not.
async fn check_file_permissions(ctx: &Context, config_path: &Path) -> Result<()> {
    debug!("Checking permissions for trusted commands configuration file: {}", config_path.display());
    
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        
        // Get file metadata to check permissions
        match ctx.fs().symlink_metadata(config_path).await {
            Ok(metadata) => {
                let permissions = metadata.permissions();
                let mode = permissions.mode();
                
                // Check if the file is readable or writable by group or others
                // 0o077 is the mask for group and others permissions (rwxrwx---)
                if (mode & 0o077) != 0 {
                    // File has insecure permissions
                    warn!("Security warning: The trusted commands configuration file has insecure permissions: {:o}", mode);
                    warn!("The file should be readable and writable only by the owner (mode 600 or 400)");
                    warn!("Other users may be able to read or modify your trusted commands configuration");
                    warn!("To fix this, run: chmod 600 {}", config_path.display());
                } else {
                    debug!("Trusted commands configuration file has secure permissions: {:o}", mode);
                }
            },
            Err(err) => {
                // Don't fail if we can't check permissions, just log a warning
                warn!("Could not check permissions for trusted commands configuration file: {}", err);
            }
        }
    }
    
    #[cfg(windows)]
    {
        // Windows permission checking is more complex and would require using Windows-specific APIs
        // For now, just log a general security recommendation
        warn!("Security best practice: Ensure your trusted commands configuration file has secure permissions");
        warn!("The file should be accessible only by your user account");
    }
    
    // Always return Ok to avoid breaking functionality if permission checking fails
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;


    #[tokio::test]
    async fn test_locate_config_file() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        
        let config_path = locate_config_file(&ctx);
        
        // Get the home directory from the context
        let home_dir = ctx.env().home().unwrap();
        
        // Expected path
        let expected_path = home_dir.join(".aws").join("amazonq").join(TRUSTED_COMMANDS_CONFIG_FILENAME);
        
        assert_eq!(config_path, expected_path);
    }

    #[tokio::test]
    async fn test_config_file_exists() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let config_path = locate_config_file(&ctx);
        
        // Create the directory structure
        let dir_path = config_path.parent().unwrap();
        ctx.fs().create_dir_all(dir_path).await.unwrap();
        
        // File doesn't exist yet
        assert!(!config_file_exists(&ctx, &config_path).await);
        
        // Create an empty file
        ctx.fs().write(&config_path, "").await.unwrap();
        
        // Now the file exists
        assert!(config_file_exists(&ctx, &config_path).await);
    }
    
    #[tokio::test]
    async fn test_load_config_file_not_found() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let config_path = locate_config_file(&ctx);
        
        // Try to load a non-existent file
        let result = load_config(&ctx, &config_path).await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_load_config_invalid_json() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let config_path = locate_config_file(&ctx);
        
        // Create the directory structure
        let dir_path = config_path.parent().unwrap();
        ctx.fs().create_dir_all(dir_path).await.unwrap();
        
        // Create a file with invalid JSON
        ctx.fs().write(&config_path, "{ invalid json }").await.unwrap();
        
        // Try to load the file with invalid JSON
        let result = load_config(&ctx, &config_path).await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_load_config_valid() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let config_path = locate_config_file(&ctx);
        
        // Create the directory structure
        let dir_path = config_path.parent().unwrap();
        ctx.fs().create_dir_all(dir_path).await.unwrap();
        
        // Create a file with valid JSON
        let valid_json = r#"{
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
        
        ctx.fs().write(&config_path, valid_json).await.unwrap();
        
        // Load the valid configuration
        let result = load_config(&ctx, &config_path).await;
        assert!(result.is_ok());
        
        let processed_config = result.unwrap();
        assert!(processed_config.is_trusted("npm run build"));
        assert!(processed_config.is_trusted("git status"));
        assert!(processed_config.is_trusted("git log"));
        assert!(!processed_config.is_trusted("git push"));
    }
    
    #[tokio::test]
    async fn test_load_config_invalid_regex() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let config_path = locate_config_file(&ctx);
        
        // Create the directory structure
        let dir_path = config_path.parent().unwrap();
        ctx.fs().create_dir_all(dir_path).await.unwrap();
        
        // Create a file with valid JSON but invalid regex
        let json_with_invalid_regex = r#"{
            "trusted_commands": [
                {
                    "type": "match",
                    "command": "npm *",
                    "description": "All npm commands"
                },
                {
                    "type": "regex",
                    "command": "[",
                    "description": "Invalid regex pattern"
                }
            ]
        }"#;
        
        ctx.fs().write(&config_path, json_with_invalid_regex).await.unwrap();
        
        // Load the configuration with invalid regex
        let result = load_config(&ctx, &config_path).await;
        assert!(result.is_ok()); // Should still load but ignore the invalid regex
        
        let processed_config = result.unwrap();
        assert!(processed_config.is_trusted("npm run build"));
        assert!(!processed_config.is_trusted("["));
    }
    
    #[cfg(unix)]
    #[tokio::test]
    async fn test_check_file_permissions() {
        // No need to import PermissionsExt here as it's not used in the test
        
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let config_path = locate_config_file(&ctx);
        
        // Create the directory structure
        let dir_path = config_path.parent().unwrap();
        ctx.fs().create_dir_all(dir_path).await.unwrap();
        
        // Create a file with secure permissions (600)
        ctx.fs().write(&config_path, "{}").await.unwrap();
        
        // This test is limited because we can't easily set permissions in the fake filesystem
        // But we can at least verify that the function doesn't fail
        let result = check_file_permissions(&ctx, &config_path).await;
        assert!(result.is_ok());
    }
}