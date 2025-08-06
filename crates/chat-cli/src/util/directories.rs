use std::path::{
    PathBuf,
    StripPrefixError,
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
}

type Result<T, E = DirectoryError> = std::result::Result<T, E>;

const WORKSPACE_AGENT_DIR_RELATIVE: &str = ".amazonq/cli-agents";
const GLOBAL_AGENT_DIR_RELATIVE_TO_HOME: &str = ".aws/amazonq/cli-agents";

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
pub fn chat_legacy_mcp_config(os: &Os) -> Result<PathBuf> {
    Ok(home_dir(os)?.join(".aws").join("amazonq").join("mcp.json"))
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
}
