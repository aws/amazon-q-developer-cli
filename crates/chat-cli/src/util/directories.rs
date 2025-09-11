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

use crate::cli::DEFAULT_AGENT_NAME;
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

type Result<T, E = DirectoryError> = std::result::Result<T, E>;

const WORKSPACE_AGENT_DIR_RELATIVE: &str = ".amazonq/cli-agents";
const GLOBAL_AGENT_DIR_RELATIVE_TO_HOME: &str = ".aws/amazonq/cli-agents";
const CLI_BASH_HISTORY_PATH: &str = ".aws/amazonq/.cli_bash_history";

/// The directory of the users home
///
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

/// The q data directory
///
/// - Linux: `$XDG_DATA_HOME/amazon-q` or `$HOME/.local/share/amazon-q`
/// - MacOS: `$HOME/Library/Application Support/amazon-q`
pub fn fig_data_dir() -> Result<PathBuf> {
    Ok(dirs::data_local_dir()
        .ok_or(DirectoryError::NoHomeDirectory)?
        .join("amazon-q"))
}

/// Get the macos tempdir from the `confstr` function
///
/// See: <https://man7.org/linux/man-pages/man3/confstr.3.html>
#[cfg(target_os = "macos")]
fn macos_tempdir() -> Result<PathBuf> {
    let len = unsafe { libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, std::ptr::null::<i8>().cast_mut(), 0) };
    let mut buf: Vec<u8> = vec![0; len];
    unsafe { libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, buf.as_mut_ptr().cast(), buf.len()) };
    let c_string = std::ffi::CString::from_vec_with_nul(buf)?;
    let str = c_string.into_string()?;
    Ok(PathBuf::from(str))
}

/// Runtime dir is used for runtime data that should not be persisted for a long time, e.g. socket
/// files and logs
///
/// The XDG_RUNTIME_DIR is set by systemd <https://www.freedesktop.org/software/systemd/man/latest/file-hierarchy.html#/run/user/>,
/// if this is not set such as on macOS it will fallback to TMPDIR which is secure on macOS
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

/// The directory to all the fig logs
/// - Linux: `/tmp/fig/$USER/logs`
/// - MacOS: `$TMPDIR/logs`
/// - Windows: `%TEMP%\fig\logs`
pub fn logs_dir() -> Result<PathBuf> {
    cfg_if::cfg_if! {
        if #[cfg(unix)] {
            Ok(runtime_dir()?.join("qlog"))
        } else if #[cfg(windows)] {
            Ok(std::env::temp_dir().join("amazon-q").join("logs"))
        }
    }
}

/// Example agent config path
pub fn example_agent_config(os: &Os) -> Result<PathBuf> {
    let global_path = chat_global_agent_path(os)?;
    Ok(global_path.join("agent_config.json.example"))
}

/// Legacy global MCP server config path
pub fn chat_legacy_global_mcp_config(os: &Os) -> Result<PathBuf> {
    Ok(home_dir(os)?.join(".aws").join("amazonq").join("mcp.json"))
}

pub fn chat_cli_bash_history_path(os: &Os) -> Result<PathBuf> {
    Ok(home_dir(os)?.join(CLI_BASH_HISTORY_PATH))
}

/// Legacy workspace MCP server config path
pub fn chat_legacy_workspace_mcp_config(os: &Os) -> Result<PathBuf> {
    let cwd = os.env.current_dir()?;
    Ok(cwd.join(".amazonq").join("mcp.json"))
}

/// The directory to the directory containing global agents
pub fn chat_global_agent_path(os: &Os) -> Result<PathBuf> {
    Ok(home_dir(os)?.join(GLOBAL_AGENT_DIR_RELATIVE_TO_HOME))
}

/// The directory to the directory containing config for the `/context` feature in `q chat`.
pub fn chat_local_agent_dir(os: &Os) -> Result<PathBuf> {
    let cwd = os.env.current_dir()?;
    Ok(cwd.join(WORKSPACE_AGENT_DIR_RELATIVE))
}

/// Canonicalizes path given by expanding the path given
pub fn canonicalizes_path(os: &Os, path_as_str: &str) -> Result<String> {
    let context = |input: &str| Ok(os.env.get(input).ok());
    let home_dir = || os.env.home().map(|p| p.to_string_lossy().to_string());

    Ok(shellexpand::full_with_context(path_as_str, home_dir, context)?.to_string())
}

/// Given a globset builder and a path, build globs for both the file and directory patterns
/// This is needed because by default glob does not match children of a dir so we need both
/// patterns to exist in a globset.
pub fn add_gitignore_globs(builder: &mut GlobSetBuilder, path: &str) -> Result<()> {
    let glob_for_file = Glob::new(path)?;

    // remove existing slash in path so we don't end up with double slash
    // Glob doesn't normalize the path so it doesn't work with double slash
    let dir_pattern: String = format!("{}/**", path.trim_end_matches('/'));
    let glob_for_dir = Glob::new(&dir_pattern)?;

    builder.add(glob_for_file);
    builder.add(glob_for_dir);

    Ok(())
}

/// Derives the absolute path to an agent config directory given a "workspace directory".
/// A workspace directory is a directory where q chat is to be launched
///
/// For example, if the given path is /path/one, then the derived config path would be
/// `/path/one/.amazonq/agents`
pub fn agent_config_dir(workspace_dir: PathBuf) -> Result<PathBuf> {
    Ok(workspace_dir.join(WORKSPACE_AGENT_DIR_RELATIVE))
}

/// The directory to the directory containing config for the `/context` feature in `q chat`.
pub fn chat_global_context_path(os: &Os) -> Result<PathBuf> {
    Ok(home_dir(os)?.join(".aws").join("amazonq").join("global_context.json"))
}

/// The directory to the directory containing config for the `/context` feature in `q chat`.
#[allow(dead_code)]
pub fn chat_profiles_dir(os: &Os) -> Result<PathBuf> {
    Ok(home_dir(os)?.join(".aws").join("amazonq").join("profiles"))
}

/// The directory for knowledge base storage
pub fn knowledge_bases_dir(os: &Os) -> Result<PathBuf> {
    Ok(home_dir(os)?.join(".aws").join("amazonq").join("knowledge_bases"))
}

/// The directory for agent-specific knowledge base storage
pub fn agent_knowledge_dir(os: &Os, agent: Option<&crate::cli::Agent>) -> Result<PathBuf> {
    let unique_id = if let Some(agent) = agent {
        generate_agent_unique_id(agent)
    } else {
        // Default agent case
        DEFAULT_AGENT_NAME.to_string()
    };
    Ok(knowledge_bases_dir(os)?.join(unique_id))
}

/// The directory for MCP authentication cache
///
/// This is the same directory used by IDE for SSO cache storage.
/// - All platforms: `$HOME/.aws/sso/cache`
pub fn get_mcp_auth_dir(os: &Os) -> Result<PathBuf> {
    Ok(home_dir(os)?.join(".aws").join("sso").join("cache"))
}

/// Generate a unique identifier for an agent based on its path and name
fn generate_agent_unique_id(agent: &crate::cli::Agent) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{
        Hash,
        Hasher,
    };

    if let Some(path) = &agent.path {
        // Create a hash from the agent's path for uniqueness
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        let path_hash = hasher.finish();

        // Combine hash with agent name for readability
        format!("{}_{:x}", agent.name, path_hash)
    } else {
        // For agents without a path (like default), use just the name
        agent.name.clone()
    }
}

/// The path to the fig settings file
pub fn settings_path() -> Result<PathBuf> {
    Ok(fig_data_dir()?.join("settings.json"))
}

/// The path to the local sqlite database
pub fn database_path() -> Result<PathBuf> {
    Ok(fig_data_dir()?.join("data.sqlite3"))
}

#[cfg(test)]
mod linux_tests {
    use super::*;

    #[test]
    fn all_paths() {
        assert!(logs_dir().is_ok());
        assert!(settings_path().is_ok());
    }

    #[test]
    fn test_add_gitignore_globs() {
        let direct_file = "/home/user/a.txt";
        let nested_file = "/home/user/folder/a.txt";
        let other_file = "/home/admin/a.txt";

        // Case 1: Path with trailing slash
        let mut builder1 = GlobSetBuilder::new();
        add_gitignore_globs(&mut builder1, "/home/user/").unwrap();
        let globset1 = builder1.build().unwrap();

        assert!(globset1.is_match(direct_file));
        assert!(globset1.is_match(nested_file));
        assert!(!globset1.is_match(other_file));

        // Case 2: Path without trailing slash - should behave same as case 1
        let mut builder2 = GlobSetBuilder::new();
        add_gitignore_globs(&mut builder2, "/home/user").unwrap();
        let globset2 = builder2.build().unwrap();

        assert!(globset2.is_match(direct_file));
        assert!(globset2.is_match(nested_file));
        assert!(!globset1.is_match(other_file));

        // Case 3: File path - should only match exact file
        let mut builder3 = GlobSetBuilder::new();
        add_gitignore_globs(&mut builder3, "/home/user/a.txt").unwrap();
        let globset3 = builder3.build().unwrap();

        assert!(globset3.is_match(direct_file));
        assert!(!globset3.is_match(nested_file));
        assert!(!globset1.is_match(other_file));
    }
}

// TODO(grant): Add back path tests on linux
#[cfg(all(test, not(target_os = "linux")))]
mod tests {
    use insta;

    use super::*;

    macro_rules! assert_directory {
        ($value:expr, @$snapshot:literal) => {
            insta::assert_snapshot!(
                sanitized_directory_path($value),
                @$snapshot,
            )
        };
    }

    macro_rules! macos {
        ($value:expr, @$snapshot:literal) => {
            #[cfg(target_os = "macos")]
            assert_directory!($value, @$snapshot)
        };
    }

    macro_rules! linux {
        ($value:expr, @$snapshot:literal) => {
            #[cfg(target_os = "linux")]
            assert_directory!($value, @$snapshot)
        };
    }

    macro_rules! windows {
        ($value:expr, @$snapshot:literal) => {
            #[cfg(target_os = "windows")]
            assert_directory!($value, @$snapshot)
        };
    }

    fn sanitized_directory_path(path: Result<PathBuf>) -> String {
        let mut path = path.unwrap().into_os_string().into_string().unwrap();

        if let Ok(home) = std::env::var("HOME") {
            let home = home.strip_suffix('/').unwrap_or(&home);
            path = path.replace(home, "$HOME");
        }

        let user = whoami::username();
        path = path.replace(&user, "$USER");

        if let Ok(tmpdir) = std::env::var("TMPDIR") {
            let tmpdir = tmpdir.strip_suffix('/').unwrap_or(&tmpdir);
            path = path.replace(tmpdir, "$TMPDIR");
        }

        #[cfg(target_os = "macos")]
        {
            if let Ok(tmpdir) = macos_tempdir() {
                let tmpdir = tmpdir.to_str().unwrap();
                let tmpdir = tmpdir.strip_suffix('/').unwrap_or(tmpdir);
                path = path.replace(tmpdir, "$TMPDIR");
            };
        }

        if let Ok(xdg_runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            let xdg_runtime_dir = xdg_runtime_dir.strip_suffix('/').unwrap_or(&xdg_runtime_dir);
            path = path.replace(xdg_runtime_dir, "$XDG_RUNTIME_DIR");
        }

        #[cfg(target_os = "linux")]
        {
            path = path.replace("/tmp", "$TMPDIR");
        }

        path
    }

    #[test]
    fn snapshot_fig_data_dir() {
        linux!(fig_data_dir(), @"$HOME/.local/share/amazon-q");
        macos!(fig_data_dir(), @"$HOME/Library/Application Support/amazon-q");
        windows!(fig_data_dir(), @r"C:\Users\$USER\AppData\Local\amazon-q");
    }

    #[test]
    fn snapshot_settings_path() {
        linux!(settings_path(), @"$HOME/.local/share/amazon-q/settings.json");
        macos!(settings_path(), @"$HOME/Library/Application Support/amazon-q/settings.json");
        windows!(settings_path(), @r"C:\Users\$USER\AppData\Local\amazon-q\settings.json");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_tempdir_test() {
        let tmpdir = macos_tempdir().unwrap();
        println!("{:?}", tmpdir);
    }

    #[tokio::test]
    async fn test_canonicalizes_path() {
        use std::fs;

        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create a test file and directory
        let test_file = temp_path.join("test_file.txt");
        let test_dir = temp_path.join("test_dir");
        fs::write(&test_file, "test content").unwrap();
        fs::create_dir(&test_dir).unwrap();

        let test_os = Os::new().await.unwrap();
        unsafe {
            test_os.env.set_var("HOME", "/home/testuser");
            test_os.env.set_var("TEST_VAR", "test_value");
        }

        // Test home directory expansion
        let result = canonicalizes_path(&test_os, "~/test").unwrap();
        assert_eq!(result, "/home/testuser/test");

        // Test environment variable expansion
        let result = canonicalizes_path(&test_os, "$TEST_VAR/path").unwrap();
        assert_eq!(result, "test_value/path");

        // Test combined expansion
        let result = canonicalizes_path(&test_os, "~/$TEST_VAR").unwrap();
        assert_eq!(result, "/home/testuser/test_value");

        // Test absolute path (no expansion needed)
        let result = canonicalizes_path(&test_os, "/absolute/path").unwrap();
        assert_eq!(result, "/absolute/path");

        // Test relative path (no expansion needed)
        let result = canonicalizes_path(&test_os, "relative/path").unwrap();
        assert_eq!(result, "relative/path");

        // Test glob prefixed paths
        let result = canonicalizes_path(&test_os, "**/path").unwrap();
        assert_eq!(result, "**/path");
        let result = canonicalizes_path(&test_os, "**/middle/**/path").unwrap();
        assert_eq!(result, "**/middle/**/path");
    }
}
