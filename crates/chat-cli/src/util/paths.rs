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
use tracing::{
    debug,
    info,
    warn,
};

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
    pub const TODO_LISTS_DIR: &str = ".amazonq/cli-todo-lists";
    pub const SUBAGENTS_DIR: &str = ".amazonq/.subagents";
    pub const RULES_PATTERN: &str = "file://{}/**/*.md";

    // Default documentation files for agent resources
    pub const DEFAULT_AGENT_RESOURCES: &[&str] = &[
        "file://AmazonQ.md",
        "file://AGENTS.md",
        "file://README.md",
        "file://.kiro/steering/**/*.md",
    ];
}

type Result<T, E = DirectoryError> = std::result::Result<T, E>;

/// Trait for filesystem operations needed by migration logic
trait FileSystemChecker {
    fn exists(&self, path: &std::path::Path) -> bool;
}

/// Real filesystem implementation
struct RealFileSystem;

impl FileSystemChecker for RealFileSystem {
    fn exists(&self, path: &std::path::Path) -> bool {
        path.exists()
    }
}

/// Check if a kiro subpath should use data directory instead of home directory
fn should_use_data_dir(kiro_subpath: &str) -> bool {
    matches!(kiro_subpath, "knowledge_bases" | "cli-checkouts")
}

fn resolve_migrated_path_with_fs(
    fs: &dyn FileSystemChecker,
    kiro_base: &std::path::Path,
    amazonq_base: &std::path::Path,
    is_global: bool,
    amazonq_subpath: &str,
    kiro_subpath: &str,
) -> std::path::PathBuf {
    let scope = if is_global { "global" } else { "workspace" };

    debug!(
        "Checking migration paths for {} kiro_subpath={} amazonq_subpath={}: kiro={}, amazonq={}",
        scope,
        kiro_subpath,
        amazonq_subpath,
        kiro_base.display(),
        amazonq_base.display()
    );

    let (kiro_exists, amazonq_exists) = (fs.exists(kiro_base), fs.exists(amazonq_base));
    debug!(
        "Path existence check for {} kiro_subpath={} amazonq_subpath={}: kiro_exists={}, amazonq_exists={}",
        scope, kiro_subpath, amazonq_subpath, kiro_exists, amazonq_exists
    );

    let result_path = match (kiro_exists, amazonq_exists) {
        (true, false) => {
            info!("Using .kiro {} configuration at: {}", scope, kiro_base.display());
            kiro_base.join(kiro_subpath)
        },
        (false, true) => {
            warn!(
                "Migration notice: Using .amazonq {} configs at: {}",
                scope,
                amazonq_base.display()
            );
            amazonq_base.join(amazonq_subpath)
        },
        (true, true) => {
            warn!(
                "Config conflict: Both .amazonq and .kiro {} configs exist, using .kiro at: {}",
                scope,
                kiro_base.display()
            );
            kiro_base.join(kiro_subpath)
        },
        (false, false) => {
            debug!(
                "No existing configs found, defaulting to .kiro {} at: {}",
                scope,
                kiro_base.display()
            );
            kiro_base.join(kiro_subpath)
        },
    };

    debug!(
        "Resolved {} kiro_subpath={} amazonq_subpath={} path: {}",
        scope,
        kiro_subpath,
        amazonq_subpath,
        result_path.display()
    );
    result_path
}

fn resolve_global_migrated_path(os: &Os, amazonq_subpath: &str, kiro_subpath: &str) -> Result<PathBuf> {
    let fs = RealFileSystem;
    let home = home_dir(os)?;

    let kiro_base = if should_use_data_dir(kiro_subpath) {
        dirs::data_local_dir()
            .ok_or(DirectoryError::NoHomeDirectory)?
            .join("kiro-cli")
    } else {
        home.join(".kiro")
    };
    let amazonq_base = home.join(".aws/amazonq");

    Ok(resolve_migrated_path_with_fs(
        &fs,
        &kiro_base,
        &amazonq_base,
        true,
        amazonq_subpath,
        kiro_subpath,
    ))
}

fn resolve_local_migrated_path(os: &Os, amazonq_subpath: &str, kiro_subpath: &str) -> Result<PathBuf> {
    let fs = RealFileSystem;
    let current = os.env.current_dir()?;

    let kiro_base = current.join(".kiro");
    let amazonq_base = current.join(".amazonq");

    Ok(resolve_migrated_path_with_fs(
        &fs,
        &kiro_base,
        &amazonq_base,
        false,
        amazonq_subpath,
        kiro_subpath,
    ))
}

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
    let context = |input: &str| Ok(os.env.get(input).ok());
    let home_dir_fn = || os.env.home().map(|p| p.to_string_lossy().to_string());

    let expanded = shellexpand::full_with_context(path_as_str, home_dir_fn, context)?;
    let path_buf = if !expanded.starts_with("/") {
        let current_dir = os.env.current_dir()?;
        current_dir.join(expanded.as_ref() as &str)
    } else {
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
        resolve_local_migrated_path(self.os, "cli-agents", "agents")
    }

    pub fn prompts_dir(&self) -> Result<PathBuf> {
        resolve_local_migrated_path(self.os, "prompts", "prompts")
    }

    pub fn mcp_config(&self) -> Result<PathBuf> {
        resolve_local_migrated_path(self.os, "mcp.json", "settings/mcp.json")
    }

    pub fn rules_dir(&self) -> Result<PathBuf> {
        resolve_local_migrated_path(self.os, "rules", "rules")
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
        resolve_global_migrated_path(self.os, "cli-agents", "agents")
    }

    pub fn prompts_dir(&self) -> Result<PathBuf> {
        resolve_global_migrated_path(self.os, "prompts", "prompts")
    }

    pub fn mcp_config(&self) -> Result<PathBuf> {
        resolve_global_migrated_path(self.os, "mcp.json", "settings/mcp.json")
    }

    pub fn profiles_dir(&self) -> Result<PathBuf> {
        resolve_global_migrated_path(self.os, "profiles", "profiles")
    }

    pub fn shadow_repo_dir(&self) -> Result<PathBuf> {
        resolve_global_migrated_path(self.os, "cli-checkouts", "cli-checkouts")
    }

    pub fn cli_bash_history(&self) -> Result<PathBuf> {
        resolve_global_migrated_path(self.os, ".cli_bash_history", ".cli_bash_history")
    }

    pub fn global_context(&self) -> Result<PathBuf> {
        resolve_global_migrated_path(self.os, "global_context.json", "global_context.json")
    }

    pub fn knowledge_bases_dir(&self) -> Result<PathBuf> {
        resolve_global_migrated_path(self.os, "knowledge_bases", "knowledge_bases")
    }

    pub async fn ensure_agents_dir(&self) -> Result<PathBuf> {
        let dir = self.agents_dir()?;
        if !dir.exists() {
            self.os.fs.create_dir_all(&dir).await?;
        }
        Ok(dir)
    }

    pub fn settings_path() -> Result<PathBuf> {
        Ok(dirs::home_dir()
            .ok_or(DirectoryError::NoHomeDirectory)?
            .join(".kiro")
            .join("settings")
            .join("cli.json"))
    }

    pub fn mcp_auth_dir(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(".aws").join("sso").join("cache"))
    }

    /// Static method for database path that doesn't require Os (to avoid circular dependency)
    pub fn database_path_static() -> Result<PathBuf> {
        Ok(dirs::data_local_dir()
            .ok_or(DirectoryError::NoHomeDirectory)?
            .join("kiro-cli")
            .join("data.sqlite3"))
    }
}

#[cfg(test)]
mod migration_tests {
    use std::collections::HashSet;
    use std::path::{
        Path,
        PathBuf,
    };

    use super::*;

    /// Test filesystem implementation
    struct TestFileSystem {
        existing_paths: HashSet<PathBuf>,
    }

    impl TestFileSystem {
        fn new() -> Self {
            Self {
                existing_paths: HashSet::new(),
            }
        }

        fn add_path(&mut self, path: impl Into<PathBuf>) {
            self.existing_paths.insert(path.into());
        }
    }

    impl FileSystemChecker for TestFileSystem {
        fn exists(&self, path: &std::path::Path) -> bool {
            self.existing_paths.contains(path)
        }
    }

    #[test]
    fn test_kiro_only_workspace() {
        let mut fs = TestFileSystem::new();
        fs.add_path("/current/.kiro");

        let kiro_base = Path::new("/current/.kiro");
        let amazonq_base = Path::new("/current/.amazonq");

        let path = resolve_migrated_path_with_fs(&fs, kiro_base, amazonq_base, false, "cli-agents", "agents");
        assert_eq!(path, Path::new("/current/.kiro/agents"));
    }

    #[test]
    fn test_amazonq_only_workspace() {
        let mut fs = TestFileSystem::new();
        fs.add_path("/current/.amazonq");

        let kiro_base = Path::new("/current/.kiro");
        let amazonq_base = Path::new("/current/.amazonq");

        let path = resolve_migrated_path_with_fs(&fs, kiro_base, amazonq_base, false, "cli-agents", "agents");
        assert_eq!(path, Path::new("/current/.amazonq/cli-agents"));
    }

    #[test]
    fn test_both_exist_workspace() {
        let mut fs = TestFileSystem::new();
        fs.add_path("/current/.kiro");
        fs.add_path("/current/.amazonq");

        let kiro_base = Path::new("/current/.kiro");
        let amazonq_base = Path::new("/current/.amazonq");

        let path = resolve_migrated_path_with_fs(&fs, kiro_base, amazonq_base, false, "cli-agents", "agents");
        // Should prefer .kiro when both exist
        assert_eq!(path, Path::new("/current/.kiro/agents"));
    }

    #[test]
    fn test_neither_exist_workspace() {
        let fs = TestFileSystem::new();

        let kiro_base = Path::new("/current/.kiro");
        let amazonq_base = Path::new("/current/.amazonq");

        let path = resolve_migrated_path_with_fs(&fs, kiro_base, amazonq_base, false, "cli-agents", "agents");
        // Should default to .kiro when neither exists
        assert_eq!(path, Path::new("/current/.kiro/agents"));
    }

    #[test]
    fn test_kiro_only_global() {
        let mut fs = TestFileSystem::new();
        fs.add_path("/home/user/.kiro");

        let kiro_base = Path::new("/home/user/.kiro");
        let amazonq_base = Path::new("/home/user/.aws/amazonq");

        let path = resolve_migrated_path_with_fs(&fs, kiro_base, amazonq_base, true, "cli-agents", "agents");
        assert_eq!(path, Path::new("/home/user/.kiro/agents"));
    }

    #[test]
    fn test_amazonq_only_global() {
        let mut fs = TestFileSystem::new();
        fs.add_path("/home/user/.aws/amazonq");

        let kiro_base = Path::new("/home/user/.kiro");
        let amazonq_base = Path::new("/home/user/.aws/amazonq");

        let path = resolve_migrated_path_with_fs(&fs, kiro_base, amazonq_base, true, "cli-agents", "agents");
        assert_eq!(path, Path::new("/home/user/.aws/amazonq/cli-agents"));
    }

    #[test]
    fn test_both_exist_global() {
        let mut fs = TestFileSystem::new();
        fs.add_path("/home/user/.kiro");
        fs.add_path("/home/user/.aws/amazonq");

        let kiro_base = Path::new("/home/user/.kiro");
        let amazonq_base = Path::new("/home/user/.aws/amazonq");

        let path = resolve_migrated_path_with_fs(&fs, kiro_base, amazonq_base, true, "cli-agents", "agents");
        // Should prefer .kiro when both exist
        assert_eq!(path, Path::new("/home/user/.kiro/agents"));
    }

    #[test]
    fn test_neither_exist_global() {
        let fs = TestFileSystem::new();

        let kiro_base = Path::new("/home/user/.kiro");
        let amazonq_base = Path::new("/home/user/.aws/amazonq");

        let path = resolve_migrated_path_with_fs(&fs, kiro_base, amazonq_base, true, "cli-agents", "agents");
        // Should default to .kiro when neither exists
        assert_eq!(path, Path::new("/home/user/.kiro/agents"));
    }

    #[test]
    fn test_different_subpaths() {
        let mut fs = TestFileSystem::new();
        fs.add_path("/current/.amazonq");

        let kiro_base = Path::new("/current/.kiro");
        let amazonq_base = Path::new("/current/.amazonq");

        let agents_path = resolve_migrated_path_with_fs(&fs, kiro_base, amazonq_base, false, "cli-agents", "agents");
        let prompts_path = resolve_migrated_path_with_fs(&fs, kiro_base, amazonq_base, false, "prompts", "prompts");
        let mcp_path =
            resolve_migrated_path_with_fs(&fs, kiro_base, amazonq_base, false, "mcp.json", "settings/mcp.json");

        assert_eq!(agents_path, Path::new("/current/.amazonq/cli-agents"));
        assert_eq!(prompts_path, Path::new("/current/.amazonq/prompts"));
        assert_eq!(mcp_path, Path::new("/current/.amazonq/mcp.json"));
    }

    #[test]
    fn test_global_context_migration() {
        let mut fs = TestFileSystem::new();
        fs.add_path("/home/user/.kiro");

        let kiro_base = Path::new("/home/user/.kiro");
        let amazonq_base = Path::new("/home/user/.aws/amazonq");

        let path = resolve_migrated_path_with_fs(
            &fs,
            kiro_base,
            amazonq_base,
            true,
            "global_context.json",
            "global_context.json",
        );
        assert_eq!(path, Path::new("/home/user/.kiro/global_context.json"));
    }

    #[test]
    fn test_knowledge_bases_migration() {
        let mut fs = TestFileSystem::new();
        fs.add_path("/home/user/.aws/amazonq");

        let kiro_base = Path::new("/home/user/.kiro");
        let amazonq_base = Path::new("/home/user/.aws/amazonq");

        let path =
            resolve_migrated_path_with_fs(&fs, kiro_base, amazonq_base, true, "knowledge_bases", "knowledge_bases");
        assert_eq!(path, Path::new("/home/user/.aws/amazonq/knowledge_bases"));
    }

    #[test]
    fn test_rules_dir_migration() {
        let mut fs = TestFileSystem::new();
        fs.add_path("/current/.kiro");

        let kiro_base = Path::new("/current/.kiro");
        let amazonq_base = Path::new("/current/.amazonq");

        let path = resolve_migrated_path_with_fs(&fs, kiro_base, amazonq_base, false, "rules", "rules");
        assert_eq!(path, Path::new("/current/.kiro/rules"));
    }

    #[test]
    fn test_data_dir_usage_for_knowledge_bases() {
        let mut fs = TestFileSystem::new();
        fs.add_path("/data/kiro-cli");

        let kiro_base = Path::new("/data/kiro-cli");
        let amazonq_base = Path::new("/home/user/.aws/amazonq");

        let path =
            resolve_migrated_path_with_fs(&fs, kiro_base, amazonq_base, true, "knowledge_bases", "knowledge_bases");
        assert_eq!(path, Path::new("/data/kiro-cli/knowledge_bases"));
    }

    #[test]
    fn test_data_dir_usage_for_cli_checkouts() {
        let mut fs = TestFileSystem::new();
        fs.add_path("/data/kiro-cli");

        let kiro_base = Path::new("/data/kiro-cli");
        let amazonq_base = Path::new("/home/user/.aws/amazonq");

        let path = resolve_migrated_path_with_fs(&fs, kiro_base, amazonq_base, true, "cli-checkouts", "cli-checkouts");
        assert_eq!(path, Path::new("/data/kiro-cli/cli-checkouts"));
    }
}
