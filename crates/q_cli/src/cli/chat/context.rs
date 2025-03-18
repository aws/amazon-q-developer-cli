use std::path::{
    Path,
    PathBuf,
};
use std::{
    env,
    fs,
};

use dirs_next as dirs;
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
    /// Directory where context configurations are stored.
    config_dir: PathBuf,

    /// Directory where profile configurations are stored.
    profiles_dir: PathBuf,

    /// Global context configuration that applies to all profiles.
    pub global_config: ContextConfig,

    /// Name of the current active profile.
    pub current_profile: String,

    /// Context configuration for the current profile.
    pub profile_config: ContextConfig,
}

#[allow(clippy::verbose_file_reads)]
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
    pub fn new() -> Result<Self> {
        // Get the home directory
        let home_dir = dirs::home_dir().ok_or_else(|| eyre!("Could not determine home directory"))?;

        // Set up the configuration directories
        let config_dir = home_dir.join(".aws").join("amazonq").join("context");

        let profiles_dir = config_dir.join("profiles");

        // Create directories if they don't exist
        fs::create_dir_all(&profiles_dir)?;

        // Load global configuration
        let global_config = Self::load_global_config(&config_dir)?;

        // Load default profile
        let current_profile = "default".to_string();
        let profile_config = Self::load_profile_config(&profiles_dir, &current_profile)?;

        Ok(Self {
            config_dir,
            profiles_dir,
            global_config,
            current_profile,
            profile_config,
        })
    }

    /// Get global configuration by calling the standalone function
    fn load_global_config(config_dir: &Path) -> Result<ContextConfig> {
        load_global_config(config_dir)
    }

    /// Get profile configuration by calling the standalone function
    fn load_profile_config(profiles_dir: &Path, profile: &str) -> Result<ContextConfig> {
        load_profile_config(profiles_dir, profile)
    }

    /// Save the current configuration to disk.
    ///
    /// # Arguments
    /// * `global` - If true, save the global configuration; otherwise, save the current profile
    ///   configuration
    ///
    /// # Returns
    /// A Result indicating success or an error
    fn save_config(&self, global: bool) -> Result<()> {
        if global {
            // Save global configuration
            let global_path = self.config_dir.join("global.json");
            let contents = serde_json::to_string_pretty(&self.global_config)
                .map_err(|e| eyre!("Failed to serialize global configuration: {}", e))?;

            fs::write(&global_path, contents)?;
        } else {
            // Save profile configuration
            let profile_path = self.profiles_dir.join(format!("{}.json", self.current_profile));
            let contents = serde_json::to_string_pretty(&self.profile_config)
                .map_err(|e| eyre!("Failed to serialize profile configuration: {}", e))?;

            fs::write(&profile_path, contents)?;
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
    pub fn add_paths(&mut self, paths: Vec<String>, global: bool, force: bool) -> Result<()> {
        // Get reference to the appropriate config
        let config = if global {
            &mut self.global_config
        } else {
            &mut self.profile_config
        };

        // Validate paths exist before adding them
        if !force {
            let cwd = env::current_dir()?;
            let mut context_files = Vec::new();

            // Check each path to make sure it exists or matches at least one file
            for path in &paths {
                // We're using a temporary context_files vector just for validation
                // Pass is_validation=true to ensure we error if glob patterns don't match any files
                match Self::process_path(path, &cwd, &mut context_files, false, true) {
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
        self.save_config(global)?;

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
    pub fn remove_paths(&mut self, paths: Vec<String>, global: bool) -> Result<()> {
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
        self.save_config(global)?;

        Ok(())
    }

    /// List all available profiles.
    ///
    /// # Returns
    /// A Result containing a vector of profile names, with "default" always first
    pub fn list_profiles(&self) -> Result<Vec<String>> {
        let mut profiles = Vec::new();

        // Always include default profile
        profiles.push("default".to_string());

        // Read profile directory and extract profile names
        if self.profiles_dir.exists() {
            for entry in fs::read_dir(&self.profiles_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                    if let Some(filename) = path.file_stem() {
                        let profile_name = filename.to_string_lossy().to_string();
                        if profile_name != "default" {
                            profiles.push(profile_name);
                        }
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
    pub fn clear(&mut self, global: bool) -> Result<()> {
        // Clear the appropriate config
        if global {
            self.global_config.paths.clear();
        } else {
            self.profile_config.paths.clear();
        }

        // Save the updated configuration
        self.save_config(global)?;

        Ok(())
    }

    /// Create a new profile.
    ///
    /// # Arguments
    /// * `name` - Name of the profile to create
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub fn create_profile(&self, name: &str) -> Result<()> {
        // Validate profile name
        Self::validate_profile_name(name)?;

        // Check if profile already exists
        let profile_path = self.profiles_dir.join(format!("{}.json", name));
        if profile_path.exists() {
            return Err(eyre!("Profile '{}' already exists", name));
        }

        // Create empty profile configuration
        let config = ContextConfig::default();
        let contents = serde_json::to_string_pretty(&config)
            .map_err(|e| eyre!("Failed to serialize profile configuration: {}", e))?;

        // Create the file
        fs::write(&profile_path, contents)?;

        Ok(())
    }

    /// Delete a profile.
    ///
    /// # Arguments
    /// * `name` - Name of the profile to delete
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub fn delete_profile(&self, name: &str) -> Result<()> {
        // Cannot delete default profile
        if name == "default" {
            return Err(eyre!("Cannot delete the default profile"));
        }

        // Cannot delete active profile
        if name == self.current_profile {
            return Err(eyre!(
                "Cannot delete the active profile. Switch to another profile first"
            ));
        }

        // Check if profile exists
        let profile_path = self.profiles_dir.join(format!("{}.json", name));
        if !profile_path.exists() {
            return Err(eyre!("Profile '{}' does not exist", name));
        }

        // Delete the profile file
        fs::remove_file(&profile_path)?;

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
    pub fn rename_profile(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        // Validate profile names
        if old_name == "default" {
            return Err(eyre!("Cannot rename the default profile"));
        }

        if new_name == "default" {
            return Err(eyre!("Cannot rename to 'default' as it's a reserved profile name"));
        }

        // Validate new profile name
        Self::validate_profile_name(new_name)?;

        // Check if old profile exists
        let old_profile_path = self.profiles_dir.join(format!("{}.json", old_name));
        if !old_profile_path.exists() {
            return Err(eyre!("Profile '{}' not found", old_name));
        }

        // Check if new profile name already exists
        let new_profile_path = self.profiles_dir.join(format!("{}.json", new_name));
        if new_profile_path.exists() {
            return Err(eyre!("Profile '{}' already exists", new_name));
        }

        // Read the old profile configuration
        let profile_config = Self::load_profile_config(&self.profiles_dir, old_name)?;

        // Write the configuration to the new profile file
        let contents = serde_json::to_string_pretty(&profile_config)?;
        fs::write(&new_profile_path, contents)?;

        // Delete the old profile file
        fs::remove_file(&old_profile_path)?;

        // If the current profile is being renamed, update the current_profile field
        if self.current_profile == old_name {
            self.current_profile = new_name.to_string();
            self.profile_config = profile_config;
        }

        Ok(())
    }

    /// Switch to a different profile.
    ///
    /// # Arguments
    /// * `name` - Name of the profile to switch to
    /// * `create` - If true, create the profile if it doesn't exist
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub fn switch_profile(&mut self, name: &str, create: bool) -> Result<()> {
        // Validate profile name
        Self::validate_profile_name(name)?;

        // Special handling for default profile - it always exists
        if name == "default" {
            // Load the default profile configuration
            let profile_config = Self::load_profile_config(&self.profiles_dir, name)?;

            // Update the current profile
            self.current_profile = name.to_string();
            self.profile_config = profile_config;

            return Ok(());
        }

        // Check if profile exists
        let profile_path = self.profiles_dir.join(format!("{}.json", name));
        if !profile_path.exists() {
            if create {
                // Create the profile if requested
                self.create_profile(name)?;
            } else {
                return Err(eyre!("Profile '{}' does not exist. Use --create to create it", name));
            }
        }

        // Load the profile configuration
        let profile_config = Self::load_profile_config(&self.profiles_dir, name)?;

        // Update the current profile
        self.current_profile = name.to_string();
        self.profile_config = profile_config;

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
    pub fn get_context_files(&self, force: bool) -> Result<Vec<(String, String)>> {
        let mut context_files = Vec::new();
        let cwd = env::current_dir()?;

        // Process global paths first
        for path in &self.global_config.paths {
            // Use is_validation=false for get_context_files to handle non-matching globs gracefully
            Self::process_path(path, &cwd, &mut context_files, force, false)?;
        }

        // Then process profile-specific paths
        for path in &self.profile_config.paths {
            // Use is_validation=false for get_context_files to handle non-matching globs gracefully
            Self::process_path(path, &cwd, &mut context_files, force, false)?;
        }

        Ok(context_files)
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
    /// * `cwd` - The current working directory for resolving relative paths
    /// * `context_files` - The collection to add files to
    /// * `force` - If true, include paths that don't exist yet
    /// * `is_validation` - If true, error when glob patterns don't match; if false, silently skip
    ///
    /// # Returns
    /// A Result indicating success or an error
    fn process_path(
        path: &str,
        cwd: &Path,
        context_files: &mut Vec<(String, String)>,
        force: bool,
        is_validation: bool,
    ) -> Result<()> {
        // Expand ~ to home directory
        let expanded_path = if path.starts_with('~') {
            if let Some(home_dir) = dirs::home_dir() {
                home_dir.join(&path[2..]).to_string_lossy().to_string()
            } else {
                return Err(eyre!("Could not determine home directory"));
            }
        } else {
            path.to_string()
        };

        // Handle absolute, relative paths, and glob patterns
        let full_path = if expanded_path.starts_with('/') {
            // Absolute path
            expanded_path
        } else {
            // Relative path
            cwd.join(&expanded_path).to_string_lossy().to_string()
        };

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
                                    Self::add_file_to_context(&path, context_files)?;
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
                    Self::add_file_to_context(path, context_files)?;
                } else if path.is_dir() {
                    // For directories, add all files in the directory (non-recursive)
                    for entry in fs::read_dir(path)? {
                        let entry = entry?;
                        let path = entry.path();
                        if path.is_file() {
                            Self::add_file_to_context(&path, context_files)?;
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
    fn add_file_to_context(path: &Path, context_files: &mut Vec<(String, String)>) -> Result<()> {
        // Get the filename as a string
        let filename = path.to_string_lossy().to_string();

        // Read the file content
        let content = fs::read_to_string(path)?;

        // Add to the context collection
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
}

#[cfg(test)]
mod tests {
    use std::env;

    use tempfile::tempdir;

    use super::*;

    // Helper function to create a test ContextManager with temporary directories
    pub fn create_test_context_manager() -> Result<(ContextManager, tempfile::TempDir)> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let config_dir = temp_dir.path().join("context");
        let profiles_dir = config_dir.join("profiles");

        // Create the directory structure
        fs::create_dir_all(&profiles_dir)?;

        // Create a ContextManager with test directories
        let manager = ContextManager {
            config_dir,
            profiles_dir,
            global_config: ContextConfig {
                paths: vec!["~/.aws/amazonq/rules/**/*.md".to_string(), "AmazonQ.md".to_string()],
            },
            current_profile: "default".to_string(),
            profile_config: ContextConfig::default(),
        };

        Ok((manager, temp_dir))
    }

    #[test]
    fn test_load_global_config_new() -> Result<()> {
        // Create a temporary directory
        let temp_dir = tempdir()?;
        let config_dir = temp_dir.path();

        // Load global config from a directory that doesn't have a config file
        let config = ContextManager::load_global_config(config_dir)?;

        // Verify default paths are set
        assert_eq!(config.paths.len(), 2);
        assert_eq!(config.paths[0], "~/.aws/amazonq/rules/**/*.md");
        assert_eq!(config.paths[1], "AmazonQ.md");

        Ok(())
    }

    #[test]
    fn test_load_global_config_existing() -> Result<()> {
        // Create a temporary directory
        let temp_dir = tempdir()?;
        let config_dir = temp_dir.path();

        // Create a global.json file with custom paths
        let global_path = config_dir.join("global.json");
        let test_config = ContextConfig {
            paths: vec!["test/path1.md".to_string(), "test/path2.md".to_string()],
        };
        let contents = serde_json::to_string_pretty(&test_config)?;

        let mut file = File::create(&global_path)?;
        file.write_all(contents.as_bytes())?;

        // Load the global config
        let config = ContextManager::load_global_config(config_dir)?;

        // Verify custom paths are loaded
        assert_eq!(config.paths.len(), 2);
        assert_eq!(config.paths[0], "test/path1.md");
        assert_eq!(config.paths[1], "test/path2.md");

        Ok(())
    }

    #[test]
    fn test_load_profile_config_new() -> Result<()> {
        // Create a temporary directory
        let temp_dir = tempdir()?;
        let profiles_dir = temp_dir.path();

        // Load profile config for a profile that doesn't exist
        let config = ContextManager::load_profile_config(profiles_dir, "test_profile")?;

        // Verify it's an empty config
        assert_eq!(config.paths.len(), 0);

        Ok(())
    }

    #[test]
    fn test_load_profile_config_existing() -> Result<()> {
        // Create a temporary directory
        let temp_dir = tempdir()?;
        let profiles_dir = temp_dir.path();

        // Create a profile config file
        let profile_path = profiles_dir.join("test_profile.json");
        let test_config = ContextConfig {
            paths: vec!["profile/path1.md".to_string(), "profile/path2.md".to_string()],
        };
        let contents = serde_json::to_string_pretty(&test_config)?;

        let mut file = File::create(&profile_path)?;
        file.write_all(contents.as_bytes())?;

        // Load the profile config
        let config = ContextManager::load_profile_config(profiles_dir, "test_profile")?;

        // Verify custom paths are loaded
        assert_eq!(config.paths.len(), 2);
        assert_eq!(config.paths[0], "profile/path1.md");
        assert_eq!(config.paths[1], "profile/path2.md");

        Ok(())
    }

    #[test]
    fn test_save_config_global() -> Result<()> {
        // Create a test context manager
        let (mut manager, _temp_dir) = create_test_context_manager()?;

        // Modify the global config
        manager.global_config.paths = vec!["new/global/path.md".to_string()];

        // Save the global config
        manager.save_config(true)?;

        // Verify the file was created
        let global_path = manager.config_dir.join("global.json");
        assert!(global_path.exists());

        // Read the file and verify its contents
        let mut file = File::open(&global_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let saved_config: ContextConfig = serde_json::from_str(&contents)?;
        assert_eq!(saved_config.paths.len(), 1);
        assert_eq!(saved_config.paths[0], "new/global/path.md");

        Ok(())
    }

    #[test]
    fn test_save_config_profile() -> Result<()> {
        // Create a test context manager
        let (mut manager, _temp_dir) = create_test_context_manager()?;

        // Modify the profile config
        manager.profile_config.paths = vec!["new/profile/path.md".to_string()];

        // Save the profile config
        manager.save_config(false)?;

        // Verify the file was created
        let profile_path = manager.profiles_dir.join("default.json");
        assert!(profile_path.exists());

        // Read the file and verify its contents
        let mut file = File::open(&profile_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let saved_config: ContextConfig = serde_json::from_str(&contents)?;
        assert_eq!(saved_config.paths.len(), 1);
        assert_eq!(saved_config.paths[0], "new/profile/path.md");

        Ok(())
    }

    #[test]
    fn test_new_creates_directories() -> Result<()> {
        // Override home directory for testing
        let temp_dir = tempdir()?;
        let home_dir = temp_dir.path();

        // Set the HOME environment variable to our temp directory
        env::set_var("HOME", home_dir.to_str().unwrap());

        // Create a new ContextManager
        let _manager = ContextManager::new()?;

        // Verify directories were created
        let config_dir = home_dir.join(".aws").join("amazonq").join("context");
        let profiles_dir = config_dir.join("profiles");

        assert!(config_dir.exists());
        assert!(profiles_dir.exists());

        Ok(())
    }

    #[test]
    fn test_add_paths_global() -> Result<()> {
        // Create a test context manager
        let (mut manager, _temp_dir) = create_test_context_manager()?;

        // Add paths to global config
        let paths = vec!["test/path1.md".to_string(), "test/path2.md".to_string()];
        manager.add_paths(paths, true, true)?;

        // Verify paths were added
        assert_eq!(manager.global_config.paths.len(), 4);
        assert_eq!(manager.global_config.paths[2], "test/path1.md");
        assert_eq!(manager.global_config.paths[3], "test/path2.md");

        // Verify the file was created
        let global_path = manager.config_dir.join("global.json");
        assert!(global_path.exists());

        Ok(())
    }

    #[test]
    fn test_add_paths_profile() -> Result<()> {
        // Create a test context manager
        let (mut manager, _temp_dir) = create_test_context_manager()?;

        // Add paths to profile config
        let paths = vec!["test/path1.md".to_string(), "test/path2.md".to_string()];
        manager.add_paths(paths, false, true)?;

        // Verify paths were added
        assert_eq!(manager.profile_config.paths.len(), 2);
        assert_eq!(manager.profile_config.paths[0], "test/path1.md");
        assert_eq!(manager.profile_config.paths[1], "test/path2.md");

        // Verify the file was created
        let profile_path = manager.profiles_dir.join("default.json");
        assert!(profile_path.exists());

        Ok(())
    }

    #[test]
    fn test_add_paths_duplicate() -> Result<()> {
        // Create a test context manager
        let (mut manager, _temp_dir) = create_test_context_manager()?;

        // Add a path to profile config
        let paths = vec!["test/path1.md".to_string()];
        manager.add_paths(paths.clone(), false, true)?;

        // Try to add the same path again
        let result = manager.add_paths(paths, false, true);

        // Verify it returns an error
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("already exists"));

        Ok(())
    }

    #[test]
    fn test_remove_paths_global() -> Result<()> {
        // Create a test context manager
        let (mut manager, _temp_dir) = create_test_context_manager()?;

        // Remove a path from global config
        let paths = vec!["AmazonQ.md".to_string()];
        manager.remove_paths(paths, true)?;

        // Verify path was removed
        assert_eq!(manager.global_config.paths.len(), 1);
        assert_eq!(manager.global_config.paths[0], "~/.aws/amazonq/rules/**/*.md");

        Ok(())
    }

    #[test]
    fn test_remove_paths_profile() -> Result<()> {
        // Create a test context manager
        let (mut manager, _temp_dir) = create_test_context_manager()?;

        // Add paths to profile config
        let add_paths = vec!["test/path1.md".to_string(), "test/path2.md".to_string()];
        manager.add_paths(add_paths, false, true)?;

        // Remove a path
        let remove_paths = vec!["test/path1.md".to_string()];
        manager.remove_paths(remove_paths, false)?;

        // Verify path was removed
        assert_eq!(manager.profile_config.paths.len(), 1);
        assert_eq!(manager.profile_config.paths[0], "test/path2.md");

        Ok(())
    }

    #[test]
    fn test_remove_paths_not_found() -> Result<()> {
        // Create a test context manager
        let (mut manager, _temp_dir) = create_test_context_manager()?;

        // Try to remove a path that doesn't exist
        let paths = vec!["nonexistent/path.md".to_string()];
        let result = manager.remove_paths(paths, true);

        // Verify it returns an error
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("None of the specified paths were found"));

        Ok(())
    }

    #[test]
    fn test_clear_global() -> Result<()> {
        // Create a test context manager
        let (mut manager, _temp_dir) = tests::create_test_context_manager()?;

        // Clear global config
        manager.clear(true)?;

        // Verify paths were cleared
        assert_eq!(manager.global_config.paths.len(), 0);

        Ok(())
    }

    #[test]
    fn test_clear_profile() -> Result<()> {
        // Create a test context manager
        let (mut manager, _temp_dir) = tests::create_test_context_manager()?;

        // Add paths to profile config
        let paths = vec!["test/path1.md".to_string(), "test/path2.md".to_string()];
        manager.add_paths(paths, false, true)?;

        // Clear profile config
        manager.clear(false)?;

        // Verify paths were cleared
        assert_eq!(manager.profile_config.paths.len(), 0);

        Ok(())
    }
}

#[test]
fn test_list_profiles() -> Result<()> {
    // Create a test context manager
    let (manager, _temp_dir) = tests::create_test_context_manager()?;

    // Create some test profiles
    let profiles_dir = &manager.profiles_dir;

    // Create profile files
    let profile_names = ["default", "test-profile", "another-profile", "z-profile"];
    for name in &profile_names {
        let config = ContextConfig::default();
        let contents = serde_json::to_string_pretty(&config)?;
        let path = profiles_dir.join(format!("{}.json", name));
        let mut file = File::create(&path)?;
        file.write_all(contents.as_bytes())?;
    }

    // List profiles
    let profiles = manager.list_profiles()?;

    // Verify profiles are listed with default first and others alphabetically
    assert_eq!(profiles.len(), 4);
    assert_eq!(profiles[0], "default");
    assert_eq!(profiles[1], "another-profile");
    assert_eq!(profiles[2], "test-profile");
    assert_eq!(profiles[3], "z-profile");

    Ok(())
}

#[test]
fn test_create_profile() -> Result<()> {
    // Create a test context manager
    let (manager, _temp_dir) = tests::create_test_context_manager()?;

    // Create a new profile
    manager.create_profile("test-profile")?;

    // Verify the profile file was created
    let profile_path = manager.profiles_dir.join("test-profile.json");
    assert!(profile_path.exists());

    // Verify the profile has an empty paths list
    let mut file = File::open(&profile_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let config: ContextConfig = serde_json::from_str(&contents)?;
    assert_eq!(config.paths.len(), 0);

    Ok(())
}

#[test]
fn test_create_profile_already_exists() -> Result<()> {
    // Create a test context manager
    let (manager, _temp_dir) = tests::create_test_context_manager()?;

    // Create a profile
    manager.create_profile("test-profile")?;

    // Try to create the same profile again
    let result = manager.create_profile("test-profile");

    // Verify it returns an error
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("already exists"));

    Ok(())
}

#[test]
fn test_create_profile_invalid_name() -> Result<()> {
    // Create a test context manager
    let (manager, _temp_dir) = tests::create_test_context_manager()?;

    // Try to create a profile with an invalid name
    let result = manager.create_profile("invalid/name");

    // Verify it returns an error
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("must start with an alphanumeric character"));

    // Try to create a profile with a name starting with underscore
    let result = manager.create_profile("_invalid");

    // Verify it returns an error
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("must start with an alphanumeric character"));

    // Try to create a profile with a name starting with hyphen
    let result = manager.create_profile("-invalid");

    // Verify it returns an error
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("must start with an alphanumeric character"));

    Ok(())
}

#[test]
fn test_delete_profile() -> Result<()> {
    // Create a test context manager
    let (manager, _temp_dir) = tests::create_test_context_manager()?;

    // Create a profile
    manager.create_profile("test-profile")?;

    // Verify the profile file exists
    let profile_path = manager.profiles_dir.join("test-profile.json");
    assert!(profile_path.exists());

    // Delete the profile
    manager.delete_profile("test-profile")?;

    // Verify the profile file was deleted
    assert!(!profile_path.exists());

    Ok(())
}

#[test]
fn test_rename_profile() -> Result<()> {
    // Create a test context manager
    let (mut manager, _temp_dir) = tests::create_test_context_manager()?;

    // Create a test profile
    manager.create_profile("test-profile")?;

    // Add a path to the profile
    manager.switch_profile("test-profile", false)?;
    manager.add_paths(vec!["test/path".to_string()], false, true)?;

    // Test renaming the profile
    manager.rename_profile("test-profile", "new-profile")?;

    // Verify the old profile file is gone
    let old_profile_path = manager.profiles_dir.join("test-profile.json");
    assert!(!old_profile_path.exists());

    // Verify the new profile file exists
    let new_profile_path = manager.profiles_dir.join("new-profile.json");
    assert!(new_profile_path.exists());

    // Verify the content was transferred
    let mut file = File::open(&new_profile_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let config: ContextConfig = serde_json::from_str(&contents)?;
    assert_eq!(config.paths, vec!["test/path".to_string()]);

    // Verify the current profile was updated
    assert_eq!(manager.current_profile, "new-profile");

    Ok(())
}

#[test]
fn test_rename_nonexistent_profile() -> Result<()> {
    // Create a test context manager
    let (mut manager, _temp_dir) = tests::create_test_context_manager()?;

    // Test renaming a nonexistent profile
    let result = manager.rename_profile("nonexistent", "new-profile");

    // Verify it returns an error
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("not found"));

    Ok(())
}

#[test]
fn test_rename_to_existing_profile() -> Result<()> {
    // Create a test context manager
    let (mut manager, _temp_dir) = tests::create_test_context_manager()?;

    // Create two test profiles
    manager.create_profile("test-profile1")?;
    manager.create_profile("test-profile2")?;

    // Test renaming to an existing profile
    let result = manager.rename_profile("test-profile1", "test-profile2");

    // Verify it returns an error
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("already exists"));

    Ok(())
}

#[test]
fn test_rename_default_profile() -> Result<()> {
    // Create a test context manager
    let (mut manager, _temp_dir) = tests::create_test_context_manager()?;

    // Test renaming the default profile
    let result = manager.rename_profile("default", "new-profile");

    // Verify it returns an error
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Cannot rename the default profile"));

    Ok(())
}

#[test]
fn test_rename_to_default_profile() -> Result<()> {
    // Create a test context manager
    let (mut manager, _temp_dir) = tests::create_test_context_manager()?;

    // Create a test profile
    manager.create_profile("test-profile")?;

    // Test renaming to "default"
    let result = manager.rename_profile("test-profile", "default");

    // Verify it returns an error
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("reserved profile name"));

    Ok(())
}

#[test]
fn test_delete_profile_default() -> Result<()> {
    // Create a test context manager
    let (manager, _temp_dir) = tests::create_test_context_manager()?;

    // Try to delete the default profile
    let result = manager.delete_profile("default");

    // Verify it returns an error
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Cannot delete the default profile"));

    Ok(())
}

#[test]
fn test_delete_profile_active() -> Result<()> {
    // Create a test context manager
    let (mut manager, _temp_dir) = tests::create_test_context_manager()?;

    // Create a profile
    manager.create_profile("test-profile")?;

    // Switch to the profile
    manager.switch_profile("test-profile", false)?;

    // Try to delete the active profile
    let result = manager.delete_profile("test-profile");

    // Verify it returns an error
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Cannot delete the active profile"));

    Ok(())
}

#[test]
fn test_delete_profile_not_exists() -> Result<()> {
    // Create a test context manager
    let (manager, _temp_dir) = tests::create_test_context_manager()?;

    // Try to delete a profile that doesn't exist
    let result = manager.delete_profile("nonexistent");

    // Verify it returns an error
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("does not exist"));

    Ok(())
}

#[test]
fn test_switch_profile() -> Result<()> {
    // Create a test context manager
    let (mut manager, _temp_dir) = tests::create_test_context_manager()?;

    // Create a profile
    manager.create_profile("test-profile")?;

    // Add a path to the profile
    let profile_path = manager.profiles_dir.join("test-profile.json");
    let test_config = ContextConfig {
        paths: vec!["test/path.md".to_string()],
    };
    let contents = serde_json::to_string_pretty(&test_config)?;
    let mut file = File::create(&profile_path)?;
    file.write_all(contents.as_bytes())?;

    // Switch to the profile
    manager.switch_profile("test-profile", false)?;

    // Verify the current profile was updated
    assert_eq!(manager.current_profile, "test-profile");
    assert_eq!(manager.profile_config.paths.len(), 1);
    assert_eq!(manager.profile_config.paths[0], "test/path.md");

    Ok(())
}

#[test]
fn test_switch_profile_create() -> Result<()> {
    // Create a test context manager
    let (mut manager, _temp_dir) = tests::create_test_context_manager()?;

    // Switch to a profile that doesn't exist with create flag
    manager.switch_profile("new-profile", true)?;

    // Verify the profile was created and switched to
    assert_eq!(manager.current_profile, "new-profile");
    assert_eq!(manager.profile_config.paths.len(), 0);

    // Verify the profile file was created
    let profile_path = manager.profiles_dir.join("new-profile.json");
    assert!(profile_path.exists());

    Ok(())
}

#[test]
fn test_switch_profile_not_exists() -> Result<()> {
    // Create a test context manager
    let (mut manager, _temp_dir) = tests::create_test_context_manager()?;

    // Try to switch to a profile that doesn't exist without create flag
    let result = manager.switch_profile("nonexistent", false);

    // Verify it returns an error
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("does not exist"));

    Ok(())
}

#[test]
fn test_validate_profile_name() -> Result<()> {
    // Create a test context manager
    let (_manager, _temp_dir) = tests::create_test_context_manager()?;

    // Test valid names
    assert!(ContextManager::validate_profile_name("valid").is_ok());
    assert!(ContextManager::validate_profile_name("valid-name").is_ok());
    assert!(ContextManager::validate_profile_name("valid_name").is_ok());
    assert!(ContextManager::validate_profile_name("valid123").is_ok());
    assert!(ContextManager::validate_profile_name("1valid").is_ok());
    assert!(ContextManager::validate_profile_name("9test").is_ok());

    // Test invalid names
    assert!(ContextManager::validate_profile_name("").is_err());
    assert!(ContextManager::validate_profile_name("invalid/name").is_err());
    assert!(ContextManager::validate_profile_name("invalid.name").is_err());
    assert!(ContextManager::validate_profile_name("invalid name").is_err());
    assert!(ContextManager::validate_profile_name("_invalid").is_err());
    assert!(ContextManager::validate_profile_name("-invalid").is_err());

    Ok(())
}

#[test]
fn test_get_context_files() -> Result<()> {
    // Create a test context manager
    let (mut manager, temp_dir) = tests::create_test_context_manager()?;

    // Create some test files
    let test_dir = temp_dir.path().join("test_files");
    fs::create_dir_all(&test_dir)?;

    // Create file 1
    let file1_path = test_dir.join("file1.md");
    let mut file1 = File::create(&file1_path)?;
    file1.write_all(b"Content of file 1")?;

    // Create file 2
    let file2_path = test_dir.join("file2.md");
    let mut file2 = File::create(&file2_path)?;
    file2.write_all(b"Content of file 2")?;

    // Create a subdirectory with a file
    let subdir = test_dir.join("subdir");
    fs::create_dir_all(&subdir)?;
    let file3_path = subdir.join("file3.md");
    let mut file3 = File::create(&file3_path)?;
    file3.write_all(b"Content of file 3")?;

    // Add paths to global and profile configs
    manager.global_config.paths = vec![file1_path.to_string_lossy().to_string()];
    manager.profile_config.paths = vec![file2_path.to_string_lossy().to_string()];

    // Get context files
    let context_files = manager.get_context_files(false)?;

    // Verify files were added
    assert_eq!(context_files.len(), 2);

    // Verify file 1 (global)
    assert_eq!(context_files[0].0, file1_path.to_string_lossy().to_string());
    assert_eq!(context_files[0].1, "Content of file 1");

    // Verify file 2 (profile)
    assert_eq!(context_files[1].0, file2_path.to_string_lossy().to_string());
    assert_eq!(context_files[1].1, "Content of file 2");

    Ok(())
}

#[test]
fn test_process_path_glob() -> Result<()> {
    // Create a test context manager
    let (_manager, temp_dir) = tests::create_test_context_manager()?;

    // Create some test files
    let test_dir = temp_dir.path().join("test_files");
    fs::create_dir_all(&test_dir)?;

    // Create multiple markdown files
    for i in 1..=3 {
        let file_path = test_dir.join(format!("file{}.md", i));
        let mut file = File::create(&file_path)?;
        file.write_all(format!("Content of file {}", i).as_bytes())?;
    }

    // Create a text file (different extension)
    let text_file = test_dir.join("file.txt");
    let mut file = File::create(&text_file)?;
    file.write_all(b"Content of text file")?;

    // Process a glob pattern that matches markdown files
    let mut context_files = Vec::new();
    let glob_pattern = format!("{}/*.md", test_dir.to_string_lossy());
    ContextManager::process_path(&glob_pattern, &env::current_dir()?, &mut context_files, false, false)?;

    // Verify only markdown files were added
    assert_eq!(context_files.len(), 3);

    // Verify the text file was not included
    let text_file_name = text_file.to_string_lossy().to_string();
    assert!(!context_files.iter().any(|(name, _)| name == &text_file_name));

    Ok(())
}

#[test]
fn test_process_path_directory() -> Result<()> {
    // Create a test context manager
    let (_manager, temp_dir) = tests::create_test_context_manager()?;

    // Create a directory with files
    let test_dir = temp_dir.path().join("test_dir");
    fs::create_dir_all(&test_dir)?;

    // Create files in the directory
    for i in 1..=3 {
        let file_path = test_dir.join(format!("file{}.txt", i));
        let mut file = File::create(&file_path)?;
        file.write_all(format!("Content of file {}", i).as_bytes())?;
    }

    // Create a subdirectory with a file (should not be included)
    let subdir = test_dir.join("subdir");
    fs::create_dir_all(&subdir)?;
    let subfile = subdir.join("subfile.txt");
    let mut file = File::create(&subfile)?;
    file.write_all(b"Content of subfile")?;

    // Process the directory
    let mut context_files = Vec::new();
    ContextManager::process_path(
        &test_dir.to_string_lossy(),
        &env::current_dir()?,
        &mut context_files,
        false,
        false,
    )?;

    // Verify only files in the directory were added (not subdirectory files)
    assert_eq!(context_files.len(), 3);

    // Verify the subfile was not included
    let subfile_name = subfile.to_string_lossy().to_string();
    assert!(!context_files.iter().any(|(name, _)| name == &subfile_name));

    Ok(())
}

#[test]
fn test_add_file_to_context() -> Result<()> {
    // Create a test context manager
    let (_manager, temp_dir) = tests::create_test_context_manager()?;

    // Create a test file
    let file_path = temp_dir.path().join("test_file.txt");
    let mut file = File::create(&file_path)?;
    file.write_all(b"Test file content")?;

    // Add the file to context
    let mut context_files = Vec::new();
    ContextManager::add_file_to_context(&file_path, &mut context_files)?;

    // Verify the file was added correctly
    assert_eq!(context_files.len(), 1);
    assert_eq!(context_files[0].0, file_path.to_string_lossy().to_string());
    assert_eq!(context_files[0].1, "Test file content");

    Ok(())
}

#[test]
fn test_home_directory_expansion() -> Result<()> {
    // Create a test context manager
    let (_manager, temp_dir) = tests::create_test_context_manager()?;

    // Set the HOME environment variable to our temp directory
    env::set_var("HOME", temp_dir.path().to_str().unwrap());

    // Create a test file in the "home" directory
    let file_path = temp_dir.path().join("home_file.txt");
    let mut file = File::create(&file_path)?;
    file.write_all(b"Home file content")?;

    // Process a path with ~ expansion
    let mut context_files = Vec::new();
    ContextManager::process_path(
        "~/home_file.txt",
        &env::current_dir()?,
        &mut context_files,
        false,
        false,
    )?;

    // Verify the file was added correctly
    assert_eq!(context_files.len(), 1);
    assert_eq!(context_files[0].0, file_path.to_string_lossy().to_string());
    assert_eq!(context_files[0].1, "Home file content");

    Ok(())
}

#[test]
fn test_relative_path_resolution() -> Result<()> {
    // Create a test context manager
    let (_manager, temp_dir) = tests::create_test_context_manager()?;

    // Create a test file
    let file_path = temp_dir.path().join("relative_file.txt");
    let mut file = File::create(&file_path)?;
    file.write_all(b"Relative file content")?;

    // Get the current directory
    let current_dir = env::current_dir()?;

    // Change to the temp directory
    env::set_current_dir(temp_dir.path())?;

    // Process a relative path
    let mut context_files = Vec::new();
    ContextManager::process_path("relative_file.txt", &temp_dir.path(), &mut context_files, false, false)?;

    // Restore the current directory
    env::set_current_dir(current_dir)?;

    // Verify the file was added correctly
    assert_eq!(context_files.len(), 1);
    assert_eq!(context_files[0].0, file_path.to_string_lossy().to_string());
    assert_eq!(context_files[0].1, "Relative file content");

    Ok(())
}
#[test]
fn test_process_path_glob_validation() -> Result<()> {
    // Create a test context manager
    let (_manager, temp_dir) = tests::create_test_context_manager()?;

    // Create a test directory
    let test_dir = temp_dir.path().join("test_glob_validation");
    fs::create_dir_all(&test_dir)?;

    // Create a glob pattern that doesn't match any files
    let glob_pattern = format!("{}/*.nonexistent", test_dir.to_string_lossy());

    // Test with is_validation=true (should error)
    let mut context_files = Vec::new();
    let result = ContextManager::process_path(&glob_pattern, &env::current_dir()?, &mut context_files, false, true);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("No files found matching glob pattern"));

    // Test with is_validation=false (should silently skip)
    let mut context_files = Vec::new();
    let result = ContextManager::process_path(&glob_pattern, &env::current_dir()?, &mut context_files, false, false);
    assert!(result.is_ok());
    assert_eq!(context_files.len(), 0); // No files should be added

    Ok(())
}

#[test]
fn test_process_path_nonexistent_file_validation() -> Result<()> {
    // Create a test context manager
    let (_manager, temp_dir) = tests::create_test_context_manager()?;

    // Create a path to a non-existent file
    let nonexistent_file = temp_dir.path().join("nonexistent_file.txt");
    let nonexistent_path = nonexistent_file.to_string_lossy().to_string();

    // Test with is_validation=true (should error)
    let mut context_files = Vec::new();
    let result = ContextManager::process_path(&nonexistent_path, &env::current_dir()?, &mut context_files, false, true);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Path") && err.contains("does not exist"));

    // Test with is_validation=false (should silently skip)
    let mut context_files = Vec::new();
    let result = ContextManager::process_path(
        &nonexistent_path,
        &env::current_dir()?,
        &mut context_files,
        false,
        false,
    );
    assert!(result.is_ok());
    assert_eq!(context_files.len(), 0); // No files should be added

    // Test with force=true (should add placeholder)
    let mut context_files = Vec::new();
    let result = ContextManager::process_path(&nonexistent_path, &env::current_dir()?, &mut context_files, true, false);
    assert!(result.is_ok());
    assert_eq!(context_files.len(), 1); // Should add placeholder
    assert!(context_files[0].1.contains("does not exist yet"));

    Ok(())
}
/// Load the global context configuration.
///
/// If the global configuration file doesn't exist, returns a default configuration.
///
/// # Arguments
/// * `config_dir` - The directory where the global configuration is stored
///
/// # Returns
/// A Result containing the global ContextConfig or an error
fn load_global_config(config_dir: &Path) -> Result<ContextConfig> {
    let global_path = config_dir.join("global.json");

    if global_path.exists() {
        // Read and parse the existing configuration file
        let contents = fs::read_to_string(&global_path)?;

        let config: ContextConfig =
            serde_json::from_str(&contents).map_err(|e| eyre!("Failed to parse global configuration: {}", e))?;

        Ok(config)
    } else {
        // Return default global configuration with predefined paths
        Ok(ContextConfig {
            paths: vec![
                ".amazonq/rules/**/*.md".to_string(),
                "README.md".to_string(),
                "AmazonQ.md".to_string(),
            ],
        })
    }
}

/// Load a profile's context configuration.
///
/// If the profile configuration file doesn't exist, creates a default configuration.
///
/// # Arguments
/// * `profiles_dir` - The directory where profile configurations are stored
/// * `profile` - The name of the profile to load
///
/// # Returns
/// A Result containing the profile's ContextConfig or an error
fn load_profile_config(profiles_dir: &Path, profile: &str) -> Result<ContextConfig> {
    let profile_path = profiles_dir.join(format!("{}.json", profile));

    if profile_path.exists() {
        // Read and parse the existing configuration file
        let contents = fs::read_to_string(&profile_path)?;

        let config: ContextConfig =
            serde_json::from_str(&contents).map_err(|e| eyre!("Failed to parse profile configuration: {}", e))?;

        Ok(config)
    } else {
        // Return empty configuration for new profiles
        Ok(ContextConfig::default())
    }
}
