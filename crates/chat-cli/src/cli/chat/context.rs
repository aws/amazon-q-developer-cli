use std::collections::HashMap;
use std::io::Write;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;

use eyre::{
    Result,
    eyre,
};
use glob::glob;
use regex::Regex;
use serde::{
    Deserialize,
    Serialize,
};
use tracing::debug;

use super::hooks::{
    Hook,
    HookExecutor,
};
use crate::util::directories;
use crate::platform::Context;

pub const AMAZONQ_FILENAME: &str = "AmazonQ.md";

/// Configuration for context files, containing paths to include in the context.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ContextConfig {
    /// List of file paths or glob patterns to include in the context.
    pub paths: Vec<String>,

    /// Map of Hook Name to [`Hook`]. The hook name serves as the hook's ID.
    pub hooks: HashMap<String, Hook>,
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

    pub hook_executor: HookExecutor,
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
            hook_executor: HookExecutor::new(),
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
        let mut all_paths = self.global_config.paths.clone();
        all_paths.append(&mut self.profile_config.paths.clone());

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
            if all_paths.contains(&path) {
                return Err(eyre!("Rule '{}' already exists.", path));
            }
            if global {
                self.global_config.paths.push(path);
            } else {
                self.profile_config.paths.push(path);
            }
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
        let config = self.get_config_mut(global);

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

    /// List all available profiles using blocking operations.
    ///
    /// Similar to list_profiles but uses synchronous filesystem operations.
    ///
    /// # Returns
    /// A Result containing a vector of profile names, with "default" always first
    pub fn list_profiles_blocking(&self) -> Result<Vec<String>> {
        let mut profiles = Vec::new();

        // Always include default profile
        profiles.push("default".to_string());

        // Read profile directory and extract profile names
        let profiles_dir = directories::chat_profiles_dir(&self.ctx)?;
        if profiles_dir.exists() {
            for entry in std::fs::read_dir(profiles_dir)? {
                let entry = entry?;
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
        self.hook_executor.profile_cache.clear();

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

        self.collect_context_files(&self.global_config.paths, &mut context_files, force)
            .await?;
        self.collect_context_files(&self.profile_config.paths, &mut context_files, force)
            .await?;

        context_files.sort_by(|a, b| a.0.cmp(&b.0));
        context_files.dedup_by(|a, b| a.0 == b.0);

        Ok(context_files)
    }

    pub async fn get_context_files_by_path(&self, force: bool, path: &str) -> Result<Vec<(String, String)>> {
        let mut context_files = Vec::new();
        process_path(&self.ctx, path, &mut context_files, force, true).await?;
        Ok(context_files)
    }

    /// Get all context files from the global configuration.
    pub async fn get_global_context_files(&self, force: bool) -> Result<Vec<(String, String)>> {
        let mut context_files = Vec::new();

        self.collect_context_files(&self.global_config.paths, &mut context_files, force)
            .await?;

        Ok(context_files)
    }

    /// Get all context files from the current profile configuration.
    pub async fn get_current_profile_context_files(&self, force: bool) -> Result<Vec<(String, String)>> {
        let mut context_files = Vec::new();

        self.collect_context_files(&self.profile_config.paths, &mut context_files, force)
            .await?;

        Ok(context_files)
    }

    async fn collect_context_files(
        &self,
        paths: &[String],
        context_files: &mut Vec<(String, String)>,
        force: bool,
    ) -> Result<()> {
        for path in paths {
            // Use is_validation=false to handle non-matching globs gracefully
            process_path(&self.ctx, path, context_files, force, false).await?;
        }
        Ok(())
    }

    fn get_config_mut(&mut self, global: bool) -> &mut ContextConfig {
        if global {
            &mut self.global_config
        } else {
            &mut self.profile_config
        }
    }

    /// Add hooks to the context config. If another hook with the same name already exists, throw an
    /// error.
    ///
    /// # Arguments
    /// * `hook` - name of the hook to delete
    /// * `global` - If true, the add to the global config. If false, add to the current profile
    ///   config.
    /// * `conversation_start` - If true, add the hook to conversation_start. Otherwise, it will be
    ///   added to per_prompt.
    pub async fn add_hook(&mut self, name: String, hook: Hook, global: bool) -> Result<()> {
        let config = self.get_config_mut(global);

        if config.hooks.contains_key(&name) {
            return Err(eyre!("name already exists."));
        }

        config.hooks.insert(name, hook);
        self.save_config(global).await
    }

    /// Delete hook(s) by name
    /// # Arguments
    /// * `name` - name of the hook to delete
    /// * `global` - If true, the delete from the global config. If false, delete from the current
    ///   profile config
    pub async fn remove_hook(&mut self, name: &str, global: bool) -> Result<()> {
        let config = self.get_config_mut(global);

        if !config.hooks.contains_key(name) {
            return Err(eyre!("does not exist."));
        }

        config.hooks.remove(name);

        self.save_config(global).await
    }

    /// Sets the "disabled" field on any [`Hook`] with the given name
    /// # Arguments
    /// * `disable` - Set "disabled" field to this value
    pub async fn set_hook_disabled(&mut self, name: &str, global: bool, disable: bool) -> Result<()> {
        let config = self.get_config_mut(global);

        if !config.hooks.contains_key(name) {
            return Err(eyre!("does not exist."));
        }

        if let Some(hook) = config.hooks.get_mut(name) {
            hook.disabled = disable;
        }

        self.save_config(global).await
    }

    /// Sets the "disabled" field on all [`Hook`]s
    /// # Arguments
    /// * `disable` - Set all "disabled" fields to this value
    pub async fn set_all_hooks_disabled(&mut self, global: bool, disable: bool) -> Result<()> {
        let config = self.get_config_mut(global);

        config.hooks.iter_mut().for_each(|(_, h)| h.disabled = disable);

        self.save_config(global).await
    }

    /// Run all the currently enabled hooks from both the global and profile contexts.
    /// Skipped hooks (disabled) will not appear in the output.
    /// # Arguments
    /// * `updates` - output stream to write hook run status to if Some, else do nothing if None
    /// # Returns
    /// A vector containing pairs of a [`Hook`] definition and its execution output
    pub async fn run_hooks(&mut self, updates: Option<&mut impl Write>) -> Vec<(Hook, String)> {
        let mut hooks: Vec<&Hook> = Vec::new();

        // Set internal hook states
        let configs = [
            (&mut self.global_config.hooks, true),
            (&mut self.profile_config.hooks, false),
        ];

        for (hook_list, is_global) in configs {
            hooks.extend(hook_list.iter_mut().map(|(name, h)| {
                h.name = name.to_string();
                h.is_global = is_global;
                &*h
            }));
        }

        self.hook_executor.run_hooks(hooks, updates).await
    }
}

fn profile_dir_path(ctx: &Context, profile_name: &str) -> Result<PathBuf> {
    Ok(directories::chat_profiles_dir(ctx)?.join(profile_name))
}

/// Path to the context config file for `profile_name`.
pub fn profile_context_path(ctx: &Context, profile_name: &str) -> Result<PathBuf> {
    Ok(directories::chat_profiles_dir(ctx)?
        .join(profile_name)
        .join("context.json"))
}

/// Load the global context configuration.
///
/// If the global configuration file doesn't exist, returns a default configuration.
async fn load_global_config(ctx: &Context) -> Result<ContextConfig> {
    let global_path = directories::chat_global_context_path(ctx)?;
    debug!(?global_path, "loading profile config");
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
            hooks: HashMap::new(),
        })
    }
}

/// Load a profile's context configuration.
///
/// If the profile configuration file doesn't exist, creates a default configuration.
async fn load_profile_config(ctx: &Context, profile_name: &str) -> Result<ContextConfig> {
    let profile_path = profile_context_path(ctx, profile_name)?;
    debug!(?profile_path, "loading profile config");
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
    use std::io::Stdout;

    use super::super::hooks::HookTrigger;
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

    #[tokio::test]
    async fn test_add_hook() -> Result<()> {
        let mut manager = create_test_context_manager().await?;
        let hook = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());

        // Test adding hook to profile config
        manager.add_hook("test_hook".to_string(), hook.clone(), false).await?;
        assert!(manager.profile_config.hooks.contains_key("test_hook"));

        // Test adding hook to global config
        manager.add_hook("global_hook".to_string(), hook.clone(), true).await?;
        assert!(manager.global_config.hooks.contains_key("global_hook"));

        // Test adding duplicate hook name
        assert!(manager.add_hook("test_hook".to_string(), hook, false).await.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_hook() -> Result<()> {
        let mut manager = create_test_context_manager().await?;
        let hook = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());

        manager.add_hook("test_hook".to_string(), hook, false).await?;

        // Test removing existing hook
        manager.remove_hook("test_hook", false).await?;
        assert!(!manager.profile_config.hooks.contains_key("test_hook"));

        // Test removing non-existent hook
        assert!(manager.remove_hook("test_hook", false).await.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_set_hook_disabled() -> Result<()> {
        let mut manager = create_test_context_manager().await?;
        let hook = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());

        manager.add_hook("test_hook".to_string(), hook, false).await?;

        // Test disabling hook
        manager.set_hook_disabled("test_hook", false, true).await?;
        assert!(manager.profile_config.hooks.get("test_hook").unwrap().disabled);

        // Test enabling hook
        manager.set_hook_disabled("test_hook", false, false).await?;
        assert!(!manager.profile_config.hooks.get("test_hook").unwrap().disabled);

        // Test with non-existent hook
        assert!(manager.set_hook_disabled("nonexistent", false, true).await.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_set_all_hooks_disabled() -> Result<()> {
        let mut manager = create_test_context_manager().await?;
        let hook1 = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());
        let hook2 = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());

        manager.add_hook("hook1".to_string(), hook1, false).await?;
        manager.add_hook("hook2".to_string(), hook2, false).await?;

        // Test disabling all hooks
        manager.set_all_hooks_disabled(false, true).await?;
        assert!(manager.profile_config.hooks.values().all(|h| h.disabled));

        // Test enabling all hooks
        manager.set_all_hooks_disabled(false, false).await?;
        assert!(manager.profile_config.hooks.values().all(|h| !h.disabled));

        Ok(())
    }

    #[tokio::test]
    async fn test_run_hooks() -> Result<()> {
        let mut manager = create_test_context_manager().await?;
        let hook1 = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());
        let hook2 = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());

        manager.add_hook("hook1".to_string(), hook1, false).await?;
        manager.add_hook("hook2".to_string(), hook2, false).await?;

        // Run the hooks
        let results = manager.run_hooks(None::<&mut Stdout>).await;
        assert_eq!(results.len(), 2); // Should include both hooks

        Ok(())
    }

    #[tokio::test]
    async fn test_hooks_across_profiles() -> Result<()> {
        let mut manager = create_test_context_manager().await?;
        let hook1 = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());
        let hook2 = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());

        manager.add_hook("profile_hook".to_string(), hook1, false).await?;
        manager.add_hook("global_hook".to_string(), hook2, true).await?;

        let results = manager.run_hooks(None::<&mut Stdout>).await;
        assert_eq!(results.len(), 2); // Should include both hooks

        // Create and switch to a new profile
        manager.create_profile("test_profile").await?;
        manager.switch_profile("test_profile").await?;

        let results = manager.run_hooks(None::<&mut Stdout>).await;
        assert_eq!(results.len(), 1); // Should include global hook
        assert_eq!(results[0].0.name, "global_hook");

        Ok(())
    }
}
