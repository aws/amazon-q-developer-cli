// security.rs
// Security features for Amazon Q CLI automatic naming feature

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::os::unix::fs::PermissionsExt;
use regex::Regex;

/// Security settings for file operations
#[derive(Debug, Clone)]
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

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            redact_sensitive: false,
            prevent_overwrite: false,
            file_permissions: 0o600, // rw-------
            directory_permissions: 0o700, // rwx------
            max_path_depth: 10,
            follow_symlinks: false,
        }
    }
}

/// Error type for security operations
#[derive(Debug)]
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

impl From<io::Error> for SecurityError {
    fn from(err: io::Error) -> Self {
        SecurityError::Io(err)
    }
}

impl std::fmt::Display for SecurityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityError::Io(err) => write!(f, "I/O error: {}", err),
            SecurityError::PathTraversal(path) => write!(f, "Path traversal attempt: {:?}", path),
            SecurityError::FileExists(path) => write!(f, "File already exists: {:?}", path),
            SecurityError::PathTooDeep(path) => write!(f, "Path too deep: {:?}", path),
            SecurityError::InvalidPath(path) => write!(f, "Invalid path: {}", path),
            SecurityError::SymlinkNotAllowed(path) => write!(f, "Symlink not allowed: {:?}", path),
        }
    }
}

impl std::error::Error for SecurityError {}

/// Validate and secure a file path
pub fn validate_path(path: &Path, settings: &SecuritySettings) -> Result<PathBuf, SecurityError> {
    // Check for null bytes
    let path_str = path.to_string_lossy();
    if path_str.contains('\0') {
        return Err(SecurityError::InvalidPath(path_str.to_string()));
    }
    
    // Check path depth
    let depth = path.components().count();
    if depth > settings.max_path_depth {
        return Err(SecurityError::PathTooDeep(path.to_path_buf()));
    }
    
    // Check for symlinks if not allowed
    if !settings.follow_symlinks {
        let mut current = PathBuf::new();
        for component in path.components() {
            current.push(component);
            if current.exists() && fs::symlink_metadata(&current)?.file_type().is_symlink() {
                return Err(SecurityError::SymlinkNotAllowed(current));
            }
        }
    }
    
    // Check if file exists and overwrite is prevented
    if settings.prevent_overwrite && path.exists() && path.is_file() {
        return Err(SecurityError::FileExists(path.to_path_buf()));
    }
    
    Ok(path.to_path_buf())
}

/// Create a directory with secure permissions
pub fn create_secure_directory(path: &Path, settings: &SecuritySettings) -> Result<(), SecurityError> {
    // Create the directory if it doesn't exist
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    
    // Set directory permissions
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(settings.directory_permissions);
        fs::set_permissions(path, perms)?;
    }
    
    Ok(())
}

/// Write to a file with secure permissions
pub fn write_secure_file(path: &Path, content: &str, settings: &SecuritySettings) -> Result<(), SecurityError> {
    // Validate the path
    let path = validate_path(path, settings)?;
    
    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        create_secure_directory(parent, settings)?;
    }
    
    // Write the content
    fs::write(&path, content)?;
    
    // Set file permissions
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(settings.file_permissions);
        fs::set_permissions(&path, perms)?;
    }
    
    Ok(())
}

/// Redact sensitive information from text
pub fn redact_sensitive_information(text: &str) -> String {
    let mut redacted = text.to_string();
    
    // Redact credit card numbers
    let cc_regex = Regex::new(r"\b(?:\d{4}[-\s]?){3}\d{4}\b").unwrap();
    redacted = cc_regex.replace_all(&redacted, "[REDACTED CREDIT CARD]").to_string();
    
    // Redact social security numbers
    let ssn_regex = Regex::new(r"\b\d{3}[-\s]?\d{2}[-\s]?\d{4}\b").unwrap();
    redacted = ssn_regex.replace_all(&redacted, "[REDACTED SSN]").to_string();
    
    // Redact API keys and tokens
    let api_key_regex = Regex::new(r"\b(?:[A-Za-z0-9+/]{40}|[A-Za-z0-9+/]{64}|[A-Za-z0-9+/]{32})\b").unwrap();
    redacted = api_key_regex.replace_all(&redacted, "[REDACTED API KEY]").to_string();
    
    // Redact AWS access keys
    let aws_key_regex = Regex::new(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b").unwrap();
    redacted = aws_key_regex.replace_all(&redacted, "[REDACTED AWS KEY]").to_string();
    
    // Redact AWS secret keys
    let aws_secret_regex = Regex::new(r"\b[A-Za-z0-9/+]{40}\b").unwrap();
    redacted = aws_secret_regex.replace_all(&redacted, "[REDACTED AWS SECRET]").to_string();
    
    // Redact passwords
    let password_regex = Regex::new(r"(?i)password\s*[=:]\s*\S+").unwrap();
    redacted = password_regex.replace_all(&redacted, "password = [REDACTED]").to_string();
    
    // Redact private keys
    let private_key_regex = Regex::new(r"-----BEGIN (?:RSA |DSA |EC )?PRIVATE KEY-----[^-]*-----END (?:RSA |DSA |EC )?PRIVATE KEY-----").unwrap();
    redacted = private_key_regex.replace_all(&redacted, "[REDACTED PRIVATE KEY]").to_string();
    
    redacted
}

/// Generate a unique filename to avoid overwriting
pub fn generate_unique_filename(path: &Path) -> PathBuf {
    let file_stem = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    
    let extension = path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    
    let mut counter = 1;
    let mut unique_path = path.to_path_buf();
    
    while unique_path.exists() {
        let new_filename = format!("{}_{}.{}", file_stem, counter, extension);
        unique_path = parent.join(new_filename);
        counter += 1;
    }
    
    unique_path
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_validate_path() {
        let settings = SecuritySettings::default();
        
        // Valid path
        let valid_path = Path::new("/tmp/test.txt");
        assert!(validate_path(valid_path, &settings).is_ok());
        
        // Path with null byte
        let null_path = Path::new("/tmp/test\0.txt");
        assert!(validate_path(null_path, &settings).is_err());
        
        // Path too deep
        let mut deep_path = PathBuf::new();
        for i in 0..20 {
            deep_path.push(format!("dir{}", i));
        }
        deep_path.push("file.txt");
        assert!(validate_path(&deep_path, &settings).is_err());
    }
    
    #[test]
    fn test_create_secure_directory() {
        let temp_dir = tempdir().unwrap();
        let settings = SecuritySettings::default();
        
        let dir_path = temp_dir.path().join("secure_dir");
        assert!(create_secure_directory(&dir_path, &settings).is_ok());
        assert!(dir_path.exists());
        
        // Check permissions on Unix systems
        #[cfg(unix)]
        {
            let metadata = fs::metadata(&dir_path).unwrap();
            let permissions = metadata.permissions();
            assert_eq!(permissions.mode() & 0o777, settings.directory_permissions);
        }
    }
    
    #[test]
    fn test_write_secure_file() {
        let temp_dir = tempdir().unwrap();
        let settings = SecuritySettings::default();
        
        let file_path = temp_dir.path().join("secure_file.txt");
        assert!(write_secure_file(&file_path, "test content", &settings).is_ok());
        assert!(file_path.exists());
        
        // Check content
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "test content");
        
        // Check permissions on Unix systems
        #[cfg(unix)]
        {
            let metadata = fs::metadata(&file_path).unwrap();
            let permissions = metadata.permissions();
            assert_eq!(permissions.mode() & 0o777, settings.file_permissions);
        }
        
        // Test prevent_overwrite
        let mut settings_no_overwrite = settings.clone();
        settings_no_overwrite.prevent_overwrite = true;
        assert!(write_secure_file(&file_path, "new content", &settings_no_overwrite).is_err());
    }
    
    #[test]
    fn test_redact_sensitive_information() {
        // Test credit card redaction
        let text_with_cc = "My credit card is 1234-5678-9012-3456";
        let redacted_cc = redact_sensitive_information(text_with_cc);
        assert!(!redacted_cc.contains("1234-5678-9012-3456"));
        assert!(redacted_cc.contains("[REDACTED CREDIT CARD]"));
        
        // Test SSN redaction
        let text_with_ssn = "My SSN is 123-45-6789";
        let redacted_ssn = redact_sensitive_information(text_with_ssn);
        assert!(!redacted_ssn.contains("123-45-6789"));
        assert!(redacted_ssn.contains("[REDACTED SSN]"));
        
        // Test API key redaction
        let text_with_api_key = "My API key is abcdefghijklmnopqrstuvwxyz1234567890abcdef";
        let redacted_api_key = redact_sensitive_information(text_with_api_key);
        assert!(!redacted_api_key.contains("abcdefghijklmnopqrstuvwxyz1234567890abcdef"));
        assert!(redacted_api_key.contains("[REDACTED API KEY]"));
        
        // Test AWS key redaction
        let text_with_aws_key = "My AWS key is AKIAIOSFODNN7EXAMPLE";
        let redacted_aws_key = redact_sensitive_information(text_with_aws_key);
        assert!(!redacted_aws_key.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(redacted_aws_key.contains("[REDACTED AWS KEY]"));
        
        // Test password redaction
        let text_with_password = "password = secret123";
        let redacted_password = redact_sensitive_information(text_with_password);
        assert!(!redacted_password.contains("secret123"));
        assert!(redacted_password.contains("[REDACTED]"));
    }
    
    #[test]
    fn test_generate_unique_filename() {
        let temp_dir = tempdir().unwrap();
        
        // Create a file
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "original content").unwrap();
        
        // Generate a unique filename
        let unique_path = generate_unique_filename(&file_path);
        assert_ne!(unique_path, file_path);
        assert!(!unique_path.exists());
        
        // Create multiple files and check uniqueness
        fs::write(&unique_path, "new content").unwrap();
        let another_unique_path = generate_unique_filename(&file_path);
        assert_ne!(another_unique_path, file_path);
        assert_ne!(another_unique_path, unique_path);
        assert!(!another_unique_path.exists());
    }
}
