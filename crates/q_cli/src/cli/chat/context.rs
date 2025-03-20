use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;

use eyre::{
    Result,
    eyre,
};
use fig_os_shim::Context;
use fig_util::directories;
use glob::glob;
use regex::Regex;
use serde::{
    Deserialize,
    Serialize,
};

pub const AMAZONQ_FILENAME: &str = "AmazonQ.md";

/// Configuration for context files, containing paths to include in the context.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextConfig {
    /// List of file paths or glob patterns to include in the context.
    pub paths: Vec<String>,
}

#[allow(dead_code)]
/// Manager for context files and profiles.
#[derive(Debug, Clone)]
pub struct ContextManager {
    ctx: Arc<Context>,

    /// Global context configuration that applies to all profiles.
    pub global_config: ContextConfig,

    /// Name of the current active profile.
    pub current_profile: String,

    /// Context configuration for the current profile.
    pub profile_config: ContextConfig,
}

#[allow(dead_code)]
impl ContextManager {
    /// Create a new ContextManager with default settings.
    ///
    /// This will:
    /// 1. Create the necessary directories if they don't exist
    /// 2. Load the global configuration
    /// 3. Load the default profile configuration
    ///
    /// # Returns
    /// A Result containing the new ContextManager or an error
    pub async fn new(ctx: Arc<Context>) -> Result<Self> {
        let profiles_dir = directories::chat_profiles_dir(&ctx)?;

        ctx.fs().create_dir_all(&profiles_dir).await?;

        let global_config = load_global_config(&ctx).await?;
        let current_profile = "default".to_string();
        let profile_config = load_profile_config(&ctx, &current_profile).await?;

        Ok(Self {
            ctx,
            global_config,
            current_profile,
            profile_config,
        })
    }

    /// Save the current configuration to disk.
    ///
    /// # Arguments
    /// * `global` - If true, save the global configuration; otherwise, save the current profile
    ///   configuration
    ///
    /// # Returns
    /// A Result indicating success or an error
    async fn save_config(&self, global: bool) -> Result<()> {
        if global {
            let global_path = directories::chat_global_context_path(&self.ctx)?;
            let contents = serde_json::to_string_pretty(&self.global_config)
                .map_err(|e| eyre!("Failed to serialize global configuration: {}", e))?;

            self.ctx.fs().write(&global_path, contents).await?;
        } else {
            let profile_path = profile_context_path(&self.ctx, &self.current_profile)?;
            if let Some(parent) = profile_path.parent() {
                self.ctx.fs().create_dir_all(parent).await?;
            }
            let contents = serde_json::to_string_pretty(&self.profile_config)
                .map_err(|e| eyre!("Failed to serialize profile configuration: {}", e))?;

            self.ctx.fs().write(&profile_path, contents).await?;
        }

        Ok(())
    }

    /// Add paths to the context configuration.
    ///
    /// # Arguments
    /// * `paths` - List of paths to add
    /// * `global` - If true, add to global configuration; otherwise, add to current profile
    ///   configuration
    /// * `force` - If true, skip validation that the path exists
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub async fn add_paths(&mut self, paths: Vec<String>, global: bool, force: bool) -> Result<()> {
        // Get reference to the appropriate config
        let config = if global {
            &mut self.global_config
        } else {
            &mut self.profile_config
        };

        // Validate paths exist before adding them
        if !force {
            let mut context_files = Vec::new();

            // Check each path to make sure it exists or matches at least one file
            for path in &paths {
                // We're using a temporary context_files vector just for validation
                // Pass is_validation=true to ensure we error if glob patterns don't match any files
                match process_path(&self.ctx, path, &mut context_files, false, true).await {
                    Ok(_) => {}, // Path is valid
                    Err(e) => return Err(eyre!("Invalid path '{}': {}. Use --force to add anyway.", path, e)),
                }
            }
        }

        // Add each path, checking for duplicates
        for path in paths {
            if config.paths.contains(&path) {
                return Err(eyre!("Path '{}' already exists in the context", path));
            }
            config.paths.push(path);
        }

        // Save the updated configuration
        self.save_config(global).await?;

        Ok(())
    }

    /// Remove paths from the context configuration.
    ///
    /// # Arguments
    /// * `paths` - List of paths to remove
    /// * `global` - If true, remove from global configuration; otherwise, remove from current
    ///   profile configuration
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub async fn remove_paths(&mut self, paths: Vec<String>, global: bool) -> Result<()> {
        // Get reference to the appropriate config
        let config = if global {
            &mut self.global_config
        } else {
            &mut self.profile_config
        };

        // Track if any paths were removed
        let mut removed_any = false;

        // Remove each path if it exists
        for path in paths {
            let original_len = config.paths.len();
            config.paths.retain(|p| p != &path);

            if config.paths.len() < original_len {
                removed_any = true;
            }
        }

        if !removed_any {
            return Err(eyre!("None of the specified paths were found in the context"));
        }

        // Save the updated configuration
        self.save_config(global).await?;

        Ok(())
    }

    /// List all available profiles.
    ///
    /// # Returns
    /// A Result containing a vector of profile names, with "default" always first
    pub async fn list_profiles(&self) -> Result<Vec<String>> {
        let mut profiles = Vec::new();

        // Always include default profile
        profiles.push("default".to_string());

        // Read profile directory and extract profile names
        let profiles_dir = directories::chat_profiles_dir(&self.ctx)?;
        if profiles_dir.exists() {
            let mut read_dir = self.ctx.fs().read_dir(&profiles_dir).await?;
            while let Some(entry) = read_dir.next_entry().await? {
                let path = entry.path();
                if let (true, Some(name)) = (path.is_dir(), path.file_name()) {
                    if name != "default" {
                        profiles.push(name.to_string_lossy().to_string());
                    }
                }
            }
        }

        // Sort non-default profiles alphabetically
        if profiles.len() > 1 {
            profiles[1..].sort();
        }

        Ok(profiles)
    }

    /// Clear all paths from the context configuration.
    ///
    /// # Arguments
    /// * `global` - If true, clear global configuration; otherwise, clear current profile
    ///   configuration
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub async fn clear(&mut self, global: bool) -> Result<()> {
        // Clear the appropriate config
        if global {
            self.global_config.paths.clear();
        } else {
            self.profile_config.paths.clear();
        }

        // Save the updated configuration
        self.save_config(global).await?;

        Ok(())
    }

    /// Create a new profile.
    ///
    /// # Arguments
    /// * `name` - Name of the profile to create
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub async fn create_profile(&self, name: &str) -> Result<()> {
        validate_profile_name(name)?;

        // Check if profile already exists
        let profile_path = profile_context_path(&self.ctx, name)?;
        if profile_path.exists() {
            return Err(eyre!("Profile '{}' already exists", name));
        }

        // Create empty profile configuration
        let config = ContextConfig::default();
        let contents = serde_json::to_string_pretty(&config)
            .map_err(|e| eyre!("Failed to serialize profile configuration: {}", e))?;

        // Create the file
        if let Some(parent) = profile_path.parent() {
            self.ctx.fs().create_dir_all(parent).await?;
        }
        self.ctx.fs().write(&profile_path, contents).await?;

        Ok(())
    }

    /// Delete a profile.
    ///
    /// # Arguments
    /// * `name` - Name of the profile to delete
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub async fn delete_profile(&self, name: &str) -> Result<()> {
        if name == "default" {
            return Err(eyre!("Cannot delete the default profile"));
        } else if name == self.current_profile {
            return Err(eyre!(
                "Cannot delete the active profile. Switch to another profile first"
            ));
        }

        let profile_path = profile_dir_path(&self.ctx, name)?;
        if !profile_path.exists() {
            return Err(eyre!("Profile '{}' does not exist", name));
        }

        self.ctx.fs().remove_dir_all(&profile_path).await?;

        Ok(())
    }

    /// Rename a profile.
    ///
    /// # Arguments
    /// * `old_name` - Current name of the profile
    /// * `new_name` - New name for the profile
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub async fn rename_profile(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        // Validate profile names
        if old_name == "default" {
            return Err(eyre!("Cannot rename the default profile"));
        }
        if new_name == "default" {
            return Err(eyre!("Cannot rename to 'default' as it's a reserved profile name"));
        }

        validate_profile_name(new_name)?;

        let old_profile_path = profile_dir_path(&self.ctx, old_name)?;
        if !old_profile_path.exists() {
            return Err(eyre!("Profile '{}' not found", old_name));
        }

        let new_profile_path = profile_dir_path(&self.ctx, new_name)?;
        if new_profile_path.exists() {
            return Err(eyre!("Profile '{}' already exists", new_name));
        }

        self.ctx.fs().rename(&old_profile_path, &new_profile_path).await?;

        // If the current profile is being renamed, update the current_profile field
        if self.current_profile == old_name {
            self.current_profile = new_name.to_string();
            self.profile_config = load_profile_config(&self.ctx, new_name).await?;
        }

        Ok(())
    }

    /// Switch to a different profile.
    ///
    /// # Arguments
    /// * `name` - Name of the profile to switch to
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub async fn switch_profile(&mut self, name: &str) -> Result<()> {
        validate_profile_name(name)?;

        // Special handling for default profile - it always exists
        if name == "default" {
            // Load the default profile configuration
            let profile_config = load_profile_config(&self.ctx, name).await?;

            // Update the current profile
            self.current_profile = name.to_string();
            self.profile_config = profile_config;

            return Ok(());
        }

        // Check if profile exists
        let profile_path = profile_context_path(&self.ctx, name)?;
        if !profile_path.exists() {
            return Err(eyre!("Profile '{}' does not exist. Use 'create' to create it", name));
        }

        // Update the current profile
        self.current_profile = name.to_string();
        self.profile_config = load_profile_config(&self.ctx, name).await?;

        Ok(())
    }

    /// Get all context files (global + profile-specific).
    ///
    /// This method:
    /// 1. Processes all paths in the global and profile configurations
    /// 2. Expands glob patterns to include matching files
    /// 3. Reads the content of each file
    /// 4. Returns a vector of (filename, content) pairs
    ///
    /// # Arguments
    /// * `force` - If true, include paths that don't exist yet
    ///
    /// # Returns
    /// A Result containing a vector of (filename, content) pairs or an error
    pub async fn get_context_files(&self, force: bool) -> Result<Vec<(String, String)>> {
        let mut context_files = Vec::new();

        // Process global paths first
        for path in &self.global_config.paths {
            // Use is_validation=false for get_context_files to handle non-matching globs gracefully
            process_path(&self.ctx, path, &mut context_files, force, false).await?;
        }

        // Then process profile-specific paths
        for path in &self.profile_config.paths {
            // Use is_validation=false for get_context_files to handle non-matching globs gracefully
            process_path(&self.ctx, path, &mut context_files, force, false).await?;
        }

        Ok(context_files)
    }
}

fn profile_dir_path(ctx: &Context, profile_name: &str) -> Result<PathBuf> {
    Ok(directories::chat_profiles_dir(ctx)?.join(profile_name))
}

/// Path to the context config file for `profile_name`.
fn profile_context_path(ctx: &Context, profile_name: &str) -> Result<PathBuf> {
    Ok(directories::chat_profiles_dir(ctx)?
        .join(profile_name)
        .join("context.json"))
}

/// Load the global context configuration.
///
/// If the global configuration file doesn't exist, returns a default configuration.
async fn load_global_config(ctx: &Context) -> Result<ContextConfig> {
    let global_path = directories::chat_global_context_path(&ctx)?;
    if ctx.fs().exists(&global_path) {
        let contents = ctx.fs().read_to_string(&global_path).await?;
        let config: ContextConfig =
            serde_json::from_str(&contents).map_err(|e| eyre!("Failed to parse global configuration: {}", e))?;
        Ok(config)
    } else {
        // Return default global configuration with predefined paths
        Ok(ContextConfig {
            paths: vec![
                ".amazonq/rules/**/*.md".to_string(),
                "README.md".to_string(),
                AMAZONQ_FILENAME.to_string(),
            ],
        })
    }
}

/// Load a profile's context configuration.
///
/// If the profile configuration file doesn't exist, creates a default configuration.
async fn load_profile_config(ctx: &Context, profile_name: &str) -> Result<ContextConfig> {
    let profile_path = profile_context_path(ctx, profile_name)?;
    if ctx.fs().exists(&profile_path) {
        let contents = ctx.fs().read_to_string(&profile_path).await?;
        let config: ContextConfig =
            serde_json::from_str(&contents).map_err(|e| eyre!("Failed to parse profile configuration: {}", e))?;
        Ok(config)
    } else {
        // Return empty configuration for new profiles
        Ok(ContextConfig::default())
    }
}

/// Process a path, handling glob patterns and file types.
///
/// This method:
/// 1. Expands the path (handling ~ for home directory)
/// 2. If the path contains glob patterns, expands them
/// 3. For each resulting path, adds the file to the context collection
/// 4. Handles directories by including all files in the directory (non-recursive)
/// 5. With force=true, includes paths that don't exist yet
///
/// # Arguments
/// * `path` - The path to process
/// * `context_files` - The collection to add files to
/// * `force` - If true, include paths that don't exist yet
/// * `is_validation` - If true, error when glob patterns don't match; if false, silently skip
///
/// # Returns
/// A Result indicating success or an error
async fn process_path(
    ctx: &Context,
    path: &str,
    context_files: &mut Vec<(String, String)>,
    force: bool,
    is_validation: bool,
) -> Result<()> {
    // Expand ~ to home directory
    let expanded_path = if path.starts_with('~') {
        if let Some(home_dir) = ctx.env().home() {
            home_dir.join(&path[2..]).to_string_lossy().to_string()
        } else {
            return Err(eyre!("Could not determine home directory"));
        }
    } else {
        path.to_string()
    };

    // Handle absolute, relative paths, and glob patterns
    let full_path = if expanded_path.starts_with('/') {
        expanded_path
    } else {
        ctx.env()
            .current_dir()?
            .join(&expanded_path)
            .to_string_lossy()
            .to_string()
    };

    // Required in chroot testing scenarios so that we can use `Path::exists`.
    let full_path = ctx.fs().chroot_path_str(full_path);

    // Check if the path contains glob patterns
    if full_path.contains('*') || full_path.contains('?') || full_path.contains('[') {
        // Expand glob pattern
        match glob(&full_path) {
            Ok(entries) => {
                let mut found_any = false;

                for entry in entries {
                    match entry {
                        Ok(path) => {
                            if path.is_file() {
                                add_file_to_context(ctx, &path, context_files).await?;
                                found_any = true;
                            }
                        },
                        Err(e) => return Err(eyre!("Glob error: {}", e)),
                    }
                }

                if !found_any && !force && is_validation {
                    // When validating paths (e.g., for /context add), error if no files match
                    return Err(eyre!("No files found matching glob pattern '{}'", full_path));
                }
                // When just showing expanded files (e.g., for /context show --expand),
                // silently skip non-matching patterns (don't add anything to context_files)
            },
            Err(e) => return Err(eyre!("Invalid glob pattern '{}': {}", full_path, e)),
        }
    } else {
        // Regular path
        let path = Path::new(&full_path);
        if path.exists() {
            if path.is_file() {
                add_file_to_context(ctx, path, context_files).await?;
            } else if path.is_dir() {
                // For directories, add all files in the directory (non-recursive)
                let mut read_dir = ctx.fs().read_dir(path).await?;
                while let Some(entry) = read_dir.next_entry().await? {
                    let path = entry.path();
                    if path.is_file() {
                        add_file_to_context(ctx, &path, context_files).await?;
                    }
                }
            }
        } else if !force && is_validation {
            // When validating paths (e.g., for /context add), error if the path doesn't exist
            return Err(eyre!("Path '{}' does not exist", full_path));
        } else if force {
            // When using --force, we'll add the path even though it doesn't exist
            // This allows users to add paths that will exist in the future
            context_files.push((full_path.clone(), format!("(Path '{}' does not exist yet)", full_path)));
        }
        // When just showing expanded files (e.g., for /context show --expand),
        // silently skip non-existent paths if is_validation is false
    }

    Ok(())
}

/// Add a file to the context collection.
///
/// This method:
/// 1. Reads the content of the file
/// 2. Adds the (filename, content) pair to the context collection
///
/// # Arguments
/// * `path` - The path to the file
/// * `context_files` - The collection to add the file to
///
/// # Returns
/// A Result indicating success or an error
async fn add_file_to_context(ctx: &Context, path: &Path, context_files: &mut Vec<(String, String)>) -> Result<()> {
    let filename = path.to_string_lossy().to_string();
    let content = ctx.fs().read_to_string(path).await?;
    context_files.push((filename, content));
    Ok(())
}

/// Validate a profile name.
///
/// Profile names can only contain alphanumeric characters, hyphens, and underscores.
///
/// # Arguments
/// * `name` - Name to validate
///
/// # Returns
/// A Result indicating if the name is valid
fn validate_profile_name(name: &str) -> Result<()> {
    // Check if name is empty
    if name.is_empty() {
        return Err(eyre!("Profile name cannot be empty"));
    }

    // Check if name contains only allowed characters and starts with an alphanumeric character
    let re = Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_-]*$").unwrap();
    if !re.is_match(name) {
        return Err(eyre!(
            "Profile name must start with an alphanumeric character and can only contain alphanumeric characters, hyphens, and underscores"
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a test ContextManager with Context
    pub async fn create_test_context_manager() -> Result<ContextManager> {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let manager = ContextManager::new(ctx).await?;
        Ok(manager)
    }

    #[tokio::test]
    async fn test_validate_profile_name() {
        // Test valid names
        assert!(validate_profile_name("valid").is_ok());
        assert!(validate_profile_name("valid-name").is_ok());
        assert!(validate_profile_name("valid_name").is_ok());
        assert!(validate_profile_name("valid123").is_ok());
        assert!(validate_profile_name("1valid").is_ok());
        assert!(validate_profile_name("9test").is_ok());

        // Test invalid names
        assert!(validate_profile_name("").is_err());
        assert!(validate_profile_name("invalid/name").is_err());
        assert!(validate_profile_name("invalid.name").is_err());
        assert!(validate_profile_name("invalid name").is_err());
        assert!(validate_profile_name("_invalid").is_err());
        assert!(validate_profile_name("-invalid").is_err());
    }

    #[tokio::test]
    async fn test_profile_ops() -> Result<()> {
        let mut manager = create_test_context_manager().await?;
        let ctx = Arc::clone(&manager.ctx);

        assert_eq!(manager.current_profile, "default");

        // Create ops
        manager.create_profile("test_profile").await?;
        assert!(profile_context_path(&ctx, "test_profile")?.exists());
        assert!(manager.create_profile("test_profile").await.is_err());
        manager.create_profile("alt").await?;

        // Listing
        let profiles = manager.list_profiles().await?;
        assert!(profiles.contains(&"default".to_string()));
        assert!(profiles.contains(&"test_profile".to_string()));
        assert!(profiles.contains(&"alt".to_string()));

        // Switching
        manager.switch_profile("test_profile").await?;
        assert!(manager.switch_profile("notexists").await.is_err());

        // Renaming
        manager.rename_profile("alt", "renamed").await?;
        assert!(!profile_context_path(&ctx, "alt")?.exists());
        assert!(profile_context_path(&ctx, "renamed")?.exists());

        // Delete ops
        assert!(manager.delete_profile("test_profile").await.is_err());
        manager.switch_profile("default").await?;
        manager.delete_profile("test_profile").await?;
        assert!(!profile_context_path(&ctx, "test_profile")?.exists());
        assert!(manager.delete_profile("test_profile").await.is_err());
        assert!(manager.delete_profile("default").await.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_path_ops() -> Result<()> {
        let mut manager = create_test_context_manager().await?;
        let ctx = Arc::clone(&manager.ctx);

        // Create some test files for matching.
        ctx.fs().create_dir_all("test").await?;
        ctx.fs().write("test/p1.md", "p1").await?;
        ctx.fs().write("test/p2.md", "p2").await?;

        assert!(
            manager.get_context_files(false).await?.is_empty(),
            "no files should be returned for an empty profile when force is false"
        );
        assert_eq!(
            manager.get_context_files(true).await?.len(),
            2,
            "default non-glob global files should be included when force is true"
        );

        manager.add_paths(vec!["test/*.md".to_string()], false, false).await?;
        let files = manager.get_context_files(false).await?;
        assert!(files[0].0.ends_with("p1.md"));
        assert_eq!(files[0].1, "p1");
        assert!(files[1].0.ends_with("p2.md"));
        assert_eq!(files[1].1, "p2");

        assert!(
            manager
                .add_paths(vec!["test/*.txt".to_string()], false, false)
                .await
                .is_err(),
            "adding a glob with no matching and without force should fail"
        );

        Ok(())
    }
}
