use std::fs;
use std::path::{
    Path,
    PathBuf,
};

use uuid::Uuid;

use crate::error::Result;
use crate::types::ProgressStatus;

/// Create a context directory based on persistence setting
///
/// # Arguments
///
/// * `base_dir` - Base directory for persistent contexts
/// * `id` - Context ID
/// * `persistent` - Whether this is a persistent context
///
/// # Returns
///
/// The path to the created directory
pub fn create_context_directory(base_dir: &Path, id: &str, persistent: bool) -> Result<PathBuf> {
    let context_dir = if persistent {
        let context_dir = base_dir.join(id);
        fs::create_dir_all(&context_dir)?;
        context_dir
    } else {
        // For volatile contexts, use a temporary directory
        let temp_dir = std::env::temp_dir().join("memory_bank").join(id);
        fs::create_dir_all(&temp_dir)?;
        temp_dir
    };

    Ok(context_dir)
}

/// Generate a unique context ID
///
/// # Returns
///
/// A new UUID as a string
pub fn generate_context_id() -> String {
    Uuid::new_v4().to_string()
}

/// Count files in a directory with progress updates
///
/// # Arguments
///
/// * `dir_path` - Path to the directory
/// * `progress_callback` - Optional callback for progress updates
///
/// # Returns
///
/// The number of files found
pub fn count_files_in_directory<F>(dir_path: &Path, progress_callback: &Option<F>) -> Result<usize>
where
    F: Fn(ProgressStatus) + Send + 'static,
{
    // Notify progress: Getting file count
    if let Some(callback) = progress_callback {
        callback(ProgressStatus::CountingFiles);
    }

    // Count files first to provide progress information
    let mut file_count = 0;
    for entry in walkdir::WalkDir::new(dir_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();

        // Skip hidden files, but allow common configuration files
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if filename.starts_with('.') {
                // Allow common configuration files
                let is_config_file = filename.ends_with("rc") 
                    || filename.ends_with("config")
                    || matches!(filename, ".gitignore" | ".env" | ".dockerignore");
                
                if !is_config_file {
                    continue;
                }
            }
        }

        file_count += 1;
    }

    Ok(file_count)
}

/// Save JSON data to a file
///
/// # Arguments
///
/// * `path` - Path to save the file
/// * `data` - Data to save
///
/// # Returns
///
/// Result indicating success or failure
pub fn save_json_to_file<T: serde::Serialize>(path: &Path, data: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(data)?;
    fs::write(path, json)?;
    Ok(())
}

/// Load JSON data from a file
///
/// # Arguments
///
/// * `path` - Path to the file
///
/// # Returns
///
/// The loaded data or default if the file doesn't exist
pub fn load_json_from_file<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T> {
    if path.exists() {
        let json_str = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&json_str).unwrap_or_default())
    } else {
        Ok(T::default())
    }
}
