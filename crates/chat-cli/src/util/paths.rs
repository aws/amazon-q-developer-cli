//! Hierarchical path management for the application

use std::env::VarError;
use std::path::{
    PathBuf,
    StripPrefixError,
};

use globset::{
    Glob,
    GlobSetBuilder,
};
use thiserror::Error;

use crate::os::Os;

#[derive(Debug, Error)]
pub enum DirectoryError {
    #[error("home directory not found")]
    NoHomeDirectory,
    #[cfg(unix)]
    #[error("runtime directory not found: neither XDG_RUNTIME_DIR nor TMPDIR were found")]
    NoRuntimeDirectory,
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    TimeFormat(#[from] time::error::Format),
    #[error(transparent)]
    Utf8FromPath(#[from] camino::FromPathError),
    #[error(transparent)]
    Utf8FromPathBuf(#[from] camino::FromPathBufError),
    #[error(transparent)]
    FromVecWithNul(#[from] std::ffi::FromVecWithNulError),
    #[error(transparent)]
    IntoString(#[from] std::ffi::IntoStringError),
    #[error(transparent)]
    StripPrefix(#[from] StripPrefixError),
    #[error(transparent)]
    PathExpand(#[from] shellexpand::LookupError<VarError>),
    #[error(transparent)]
    GlobCreation(#[from] globset::Error),
}

pub mod workspace {
    //! Project-level paths (relative to current working directory)
    pub const AGENTS_DIR: &str = ".amazonq/cli-agents";
    pub const PROMPTS_DIR: &str = ".amazonq/prompts";
    pub const MCP_CONFIG: &str = ".amazonq/mcp.json";
    pub const TODO_LISTS_DIR: &str = ".amazonq/cli-todo-lists";
    pub const SUBAGENTS_DIR: &str = ".amazonq/.subagents";
    pub const RULES_PATTERN: &str = ".amazonq/rules/**/*.md";

    // Default documentation files for agent resources
    pub const DEFAULT_AGENT_RESOURCES: &[&str] = &["file://AmazonQ.md", "file://AGENTS.md", "file://README.md"];
}

pub mod global {
    //! User-level paths (relative to home directory)
    pub const AGENTS_DIR: &str = ".aws/amazonq/cli-agents";
    pub const PROMPTS_DIR: &str = ".aws/amazonq/prompts";
    pub const MCP_CONFIG: &str = ".aws/amazonq/mcp.json";
    pub const SHADOW_REPO_DIR: &str = ".aws/amazonq/cli-checkouts";
    pub const CLI_BASH_HISTORY: &str = ".aws/amazonq/.cli_bash_history";
    pub const GLOBAL_CONTEXT: &str = ".aws/amazonq/global_context.json";
    pub const PROFILES_DIR: &str = ".aws/amazonq/profiles";
    pub const KNOWLEDGE_BASES_DIR: &str = ".aws/amazonq/knowledge_bases";
}

type Result<T, E = DirectoryError> = std::result::Result<T, E>;

/// The directory of the users home
/// - Linux: /home/Alice
/// - MacOS: /Users/Alice
/// - Windows: C:\Users\Alice
pub fn home_dir(#[cfg_attr(windows, allow(unused_variables))] os: &Os) -> Result<PathBuf> {
    #[cfg(unix)]
    match cfg!(test) {
        true => os
            .env
            .get("HOME")
            .map_err(|_err| DirectoryError::NoHomeDirectory)
            .and_then(|h| {
                if h.is_empty() {
                    Err(DirectoryError::NoHomeDirectory)
                } else {
                    Ok(h)
                }
            })
            .map(PathBuf::from)
            .map(|p| os.fs.chroot_path(p)),
        false => dirs::home_dir().ok_or(DirectoryError::NoHomeDirectory),
    }

    #[cfg(windows)]
    match cfg!(test) {
        true => os
            .env
            .get("USERPROFILE")
            .map_err(|_err| DirectoryError::NoHomeDirectory)
            .and_then(|h| {
                if h.is_empty() {
                    Err(DirectoryError::NoHomeDirectory)
                } else {
                    Ok(h)
                }
            })
            .map(PathBuf::from)
            .map(|p| os.fs.chroot_path(p)),
        false => dirs::home_dir().ok_or(DirectoryError::NoHomeDirectory),
    }
}

/// Get the macos tempdir from the `confstr` function
#[cfg(target_os = "macos")]
fn macos_tempdir() -> Result<PathBuf> {
    let len = unsafe { libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, std::ptr::null::<i8>().cast_mut(), 0) };
    let mut buf: Vec<u8> = vec![0; len];
    unsafe { libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, buf.as_mut_ptr().cast(), buf.len()) };
    let c_string = std::ffi::CString::from_vec_with_nul(buf)?;
    let str = c_string.into_string()?;
    Ok(PathBuf::from(str))
}

/// Runtime dir for logs and sockets
#[cfg(unix)]
pub fn runtime_dir() -> Result<PathBuf> {
    let mut dir = dirs::runtime_dir();
    dir = dir.or_else(|| std::env::var_os("TMPDIR").map(PathBuf::from));

    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            let macos_tempdir = macos_tempdir()?;
            dir = dir.or(Some(macos_tempdir));
        } else {
            dir = dir.or_else(|| Some(std::env::temp_dir()));
        }
    }

    dir.ok_or(DirectoryError::NoRuntimeDirectory)
}

/// The directory to all the logs
pub fn logs_dir() -> Result<PathBuf> {
    cfg_if::cfg_if! {
        if #[cfg(unix)] {
            Ok(runtime_dir()?.join("qlog"))
        } else if #[cfg(windows)] {
            Ok(std::env::temp_dir().join("amazon-q").join("logs"))
        }
    }
}

/// Canonicalizes path given by expanding the path given
pub fn canonicalizes_path(os: &Os, path_as_str: &str) -> Result<String> {
    canonicalizes_path_with_base(os, path_as_str, None)
}

/// Canonicalizes path with optional base directory for resolving relative paths
///
/// # Arguments
/// * `os` - The OS context for environment and filesystem access
/// * `path_as_str` - The path string to canonicalize
/// * `base_dir` - Optional base directory for resolving relative paths. When Some and path is
///   relative, the path will be resolved relative to this directory. When None or path is absolute,
///   uses existing behavior (resolve from CWD).
///
/// # Returns
/// The canonicalized absolute path as a String
pub fn canonicalizes_path_with_base(os: &Os, path_as_str: &str, base_dir: Option<&std::path::Path>) -> Result<String> {
    let context = |input: &str| Ok(os.env.get(input).ok());
    let home_dir_fn = || os.env.home().map(|p| p.to_string_lossy().to_string());

    let expanded = shellexpand::full_with_context(path_as_str, home_dir_fn, context)?;

    let path_buf = if !expanded.starts_with("/") {
        // Path is relative
        if let Some(base) = base_dir {
            // Resolve relative to provided base directory
            base.join(expanded.as_ref() as &str)
        } else {
            // Resolve relative to current working directory
            let current_dir = os.env.current_dir()?;
            current_dir.join(expanded.as_ref() as &str)
        }
    } else {
        // Path is absolute, use as-is
        PathBuf::from(expanded.as_ref() as &str)
    };

    match path_buf.canonicalize() {
        Ok(normalized) => Ok(normalized.as_path().to_string_lossy().to_string()),
        Err(_) => {
            let normalized = normalize_path(&path_buf);
            Ok(normalized.to_string_lossy().to_string())
        },
    }
}

/// Manually normalize a path by resolving . and .. components
fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {},
            std::path::Component::ParentDir => {
                components.pop();
            },
            _ => {
                components.push(component);
            },
        }
    }
    components.iter().collect()
}

/// Given a globset builder and a path, build globs for both the file and directory patterns
/// This is needed because by default glob does not match children of a dir so we need both
/// patterns to exist in a globset.
pub fn add_gitignore_globs(builder: &mut GlobSetBuilder, path: &str) -> Result<()> {
    let glob_for_file = Glob::new(path)?;
    let dir_pattern: String = format!("{}/**", path.trim_end_matches('/'));
    let glob_for_dir = Glob::new(&dir_pattern)?;
    builder.add(glob_for_file);
    builder.add(glob_for_dir);
    Ok(())
}

/// Generate a unique identifier for an agent based on its path and name
/// Path resolver with hierarchy-aware methods
pub struct PathResolver<'a> {
    os: &'a Os,
}

impl<'a> PathResolver<'a> {
    pub fn new(os: &'a Os) -> Self {
        Self { os }
    }

    /// Get workspace-scoped path resolver
    pub fn workspace(&self) -> WorkspacePaths<'_> {
        WorkspacePaths { os: self.os }
    }

    /// Get global-scoped path resolver  
    pub fn global(&self) -> GlobalPaths<'_> {
        GlobalPaths { os: self.os }
    }
}

/// Workspace-scoped path methods
pub struct WorkspacePaths<'a> {
    os: &'a Os,
}

impl<'a> WorkspacePaths<'a> {
    pub fn agents_dir(&self) -> Result<PathBuf> {
        Ok(self.os.env.current_dir()?.join(workspace::AGENTS_DIR))
    }

    pub fn prompts_dir(&self) -> Result<PathBuf> {
        Ok(self.os.env.current_dir()?.join(workspace::PROMPTS_DIR))
    }

    pub fn mcp_config(&self) -> Result<PathBuf> {
        Ok(self.os.env.current_dir()?.join(workspace::MCP_CONFIG))
    }

    pub fn todo_lists_dir(&self) -> Result<PathBuf> {
        Ok(self.os.env.current_dir()?.join(workspace::TODO_LISTS_DIR))
    }

    pub fn subagents_dir(&self) -> Result<PathBuf> {
        Ok(self.os.env.current_dir()?.join(workspace::SUBAGENTS_DIR))
    }

    pub async fn ensure_subagents_dir(&self) -> Result<PathBuf> {
        let dir = self.subagents_dir()?;
        if !dir.exists() {
            self.os.fs.create_dir_all(&dir).await?;
        }
        Ok(dir)
    }
}

/// Global-scoped path methods
pub struct GlobalPaths<'a> {
    os: &'a Os,
}

impl<'a> GlobalPaths<'a> {
    pub fn agents_dir(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::AGENTS_DIR))
    }

    pub fn prompts_dir(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::PROMPTS_DIR))
    }

    pub fn mcp_config(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::MCP_CONFIG))
    }

    pub fn shadow_repo_dir(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::SHADOW_REPO_DIR))
    }

    pub fn cli_bash_history(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::CLI_BASH_HISTORY))
    }

    pub fn global_context(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::GLOBAL_CONTEXT))
    }

    pub fn profiles_dir(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::PROFILES_DIR))
    }

    pub fn knowledge_bases_dir(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::KNOWLEDGE_BASES_DIR))
    }

    pub async fn ensure_agents_dir(&self) -> Result<PathBuf> {
        let dir = self.agents_dir()?;
        if !dir.exists() {
            self.os.fs.create_dir_all(&dir).await?;
        }
        Ok(dir)
    }

    pub fn settings_path() -> Result<PathBuf> {
        Ok(dirs::data_local_dir()
            .ok_or(DirectoryError::NoHomeDirectory)?
            .join("amazon-q")
            .join("settings.json"))
    }

    pub fn mcp_auth_dir(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(".aws").join("sso").join("cache"))
    }

    /// Static method for settings path that doesn't require Os (to avoid circular dependency)
    pub fn settings_path_static() -> Result<PathBuf> {
        Ok(dirs::data_local_dir()
            .ok_or(DirectoryError::NoHomeDirectory)?
            .join("amazon-q")
            .join("settings.json"))
    }

    /// Static method for database path that doesn't require Os (to avoid circular dependency)
    pub fn database_path_static() -> Result<PathBuf> {
        Ok(dirs::data_local_dir()
            .ok_or(DirectoryError::NoHomeDirectory)?
            .join("amazon-q")
            .join("data.sqlite3"))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::os::Os;

    #[tokio::test]
    async fn test_canonicalizes_path_with_base_absolute_path_ignores_base() {
        let os = Os::new().await.unwrap();

        // Create a test directory structure
        let test_dir = os.env.current_dir().unwrap().join("test_base");
        os.fs.create_dir_all(&test_dir).await.unwrap();

        let absolute_path = os.env.current_dir().unwrap().join("absolute_test");
        os.fs.create_dir_all(&absolute_path).await.unwrap();

        let absolute_path_str = absolute_path.to_string_lossy().to_string();

        // When base_dir is provided but path is absolute, base_dir should be ignored
        let result = canonicalizes_path_with_base(&os, &absolute_path_str, Some(&test_dir)).unwrap();

        // Result should match the absolute path, not be relative to base_dir
        assert_eq!(result, absolute_path.to_string_lossy().to_string());

        // Cleanup
        os.fs.remove_dir_all(&test_dir).await.ok();
        os.fs.remove_dir_all(&absolute_path).await.ok();
    }

    #[tokio::test]
    async fn test_canonicalizes_path_with_base_relative_path_with_base() {
        let os = Os::new().await.unwrap();

        // Create base directory and a subdirectory within it
        let base_dir = os.env.current_dir().unwrap().join("base_test");
        let sub_dir = base_dir.join("subdir");
        os.fs.create_dir_all(&sub_dir).await.unwrap();

        // Relative path should resolve from base_dir
        let result = canonicalizes_path_with_base(&os, "subdir", Some(&base_dir)).unwrap();

        assert_eq!(result, sub_dir.to_string_lossy().to_string());

        // Cleanup
        os.fs.remove_dir_all(&base_dir).await.ok();
    }

    #[tokio::test]
    async fn test_canonicalizes_path_with_base_relative_path_without_base() {
        let os = Os::new().await.unwrap();

        // Create a subdirectory in CWD
        let cwd = os.env.current_dir().unwrap();
        let sub_dir = cwd.join("cwd_subdir");
        os.fs.create_dir_all(&sub_dir).await.unwrap();

        // When base_dir is None, relative path should resolve from CWD
        let result = canonicalizes_path_with_base(&os, "cwd_subdir", None).unwrap();

        assert_eq!(result, sub_dir.to_string_lossy().to_string());

        // Cleanup
        os.fs.remove_dir_all(&sub_dir).await.ok();
    }

    #[tokio::test]
    async fn test_canonicalizes_path_with_base_tilde_expansion() {
        let os = Os::new().await.unwrap();

        // Test that tilde gets expanded correctly
        let result = canonicalizes_path_with_base(&os, "~", None).unwrap();

        // Should expand to home directory
        let home = os.env.home().unwrap();
        assert_eq!(result, home.to_string_lossy().to_string());
    }

    #[tokio::test]
    async fn test_canonicalizes_path_with_base_env_variable_expansion() {
        let os = Os::new().await.unwrap();

        // Set a test environment variable
        unsafe {
            os.env.set_var("TEST_PATH_VAR", "/test/env/path");
        }

        // Create the directory so canonicalization works
        let test_path = PathBuf::from("/test/env/path");
        os.fs.create_dir_all(&test_path).await.unwrap();

        // Test environment variable expansion
        let result = canonicalizes_path_with_base(&os, "$TEST_PATH_VAR", None).unwrap();

        assert_eq!(result, test_path.to_string_lossy().to_string());

        // Cleanup
        os.fs.remove_dir_all(&test_path).await.ok();
    }

    #[tokio::test]
    async fn test_canonicalizes_path_with_base_relative_with_dot_slash() {
        let os = Os::new().await.unwrap();

        // Create base directory and a subdirectory within it
        let base_dir = os.env.current_dir().unwrap().join("base_dot_test");
        let sub_dir = base_dir.join("src");
        os.fs.create_dir_all(&sub_dir).await.unwrap();

        // Relative path with ./ prefix should resolve from base_dir
        let result = canonicalizes_path_with_base(&os, "./src", Some(&base_dir)).unwrap();

        assert_eq!(result, sub_dir.to_string_lossy().to_string());

        // Cleanup
        os.fs.remove_dir_all(&base_dir).await.ok();
    }

    #[tokio::test]
    async fn test_canonicalizes_path_backward_compatibility() {
        let os = Os::new().await.unwrap();

        // Create a test directory in CWD
        let cwd = os.env.current_dir().unwrap();
        let test_dir = cwd.join("compat_test");
        os.fs.create_dir_all(&test_dir).await.unwrap();

        // Test that canonicalizes_path (without base) still works as before
        let result_old = canonicalizes_path(&os, "compat_test").unwrap();
        let result_new = canonicalizes_path_with_base(&os, "compat_test", None).unwrap();

        // Both should produce the same result
        assert_eq!(result_old, result_new);
        assert_eq!(result_old, test_dir.to_string_lossy().to_string());

        // Cleanup
        os.fs.remove_dir_all(&test_dir).await.ok();
    }

    #[tokio::test]
    async fn test_canonicalizes_path_with_base_nonexistent_path() {
        let os = Os::new().await.unwrap();

        let base_dir = os.env.current_dir().unwrap().join("base_nonexist");
        os.fs.create_dir_all(&base_dir).await.unwrap();

        // Path that doesn't exist should still be normalized (not canonicalized)
        let result = canonicalizes_path_with_base(&os, "nonexistent/path", Some(&base_dir)).unwrap();

        // Should return normalized path even if it doesn't exist
        let expected = base_dir.join("nonexistent/path");
        let normalized = normalize_path(&expected);
        assert_eq!(result, normalized.to_string_lossy().to_string());

        // Cleanup
        os.fs.remove_dir_all(&base_dir).await.ok();
    }
}
