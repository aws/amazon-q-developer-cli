use std::sync::OnceLock;

/// Returns the platform-appropriate shell and its command-execution flag.
///
/// - **Unix**: Reads `$SHELL` (e.g. `("/usr/bin/zsh", "-c")`), falling back to `("bash", "-c")`
/// - **Windows**: Walks the parent process chain to detect if running inside PowerShell, returning
///   `("pwsh", "-Command")` or `("powershell", "-Command")` accordingly. Falls back to `("cmd",
///   "/C")`. Result is cached after first call.
pub fn shell_command() -> (&'static str, &'static str) {
    static CACHED: OnceLock<(&str, &str)> = OnceLock::new();
    *CACHED.get_or_init(detect_shell)
}

fn detect_shell() -> (&'static str, &'static str) {
    #[cfg(windows)]
    {
        detect_windows_shell()
    }

    #[cfg(not(windows))]
    {
        // Respect the user's preferred shell (e.g. zsh, fish) when available.
        static SHELL: OnceLock<String> = OnceLock::new();
        let shell = SHELL.get_or_init(|| std::env::var("SHELL").unwrap_or_else(|_| "bash".into()));
        (shell.as_str(), "-c")
    }
}

/// Wraps a command string with `ulimit -n 10240` on macOS to prevent
/// Python's `sh` library from overflowing when Bun sets RLIMIT_NOFILE
/// to 2^63-1. No-op on non-macOS or if the limit is already sane.
pub fn wrap_cmd_with_fd_limit(command: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("ulimit -n 10240 2>/dev/null; {command}")
    } else {
        command.to_string()
    }
}

/// Detects the parent shell on Windows.
///
/// Checks the `PSModulePath` environment variable — present when running inside
/// PowerShell (both `powershell.exe` and `pwsh.exe`). Falls back to `cmd /C`.
#[cfg(windows)]
fn detect_windows_shell() -> (&'static str, &'static str) {
    // PSModulePath is always set inside PowerShell sessions
    if std::env::var("PSModulePath").is_ok() {
        // Prefer pwsh (PowerShell 7+) if available, fall back to powershell (5.1)
        if which_exists("pwsh") {
            return ("pwsh", "-Command");
        }
        return ("powershell", "-Command");
    }
    ("cmd", "/C")
}

#[cfg(windows)]
fn which_exists(name: &str) -> bool {
    std::process::Command::new("where.exe")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_command_returns_valid_pair() {
        let (shell, flag) = shell_command();
        if cfg!(windows) {
            assert!(
                (shell == "cmd" && flag == "/C")
                    || (shell == "powershell" && flag == "-Command")
                    || (shell == "pwsh" && flag == "-Command")
            );
        } else {
            // Should respect $SHELL, falling back to bash
            let expected = std::env::var("SHELL").unwrap_or_else(|_| "bash".into());
            assert_eq!(shell, expected);
            assert_eq!(flag, "-c");
        }
    }

    #[test]
    fn test_shell_command_cached() {
        let first = shell_command();
        let second = shell_command();
        assert_eq!(first, second);
    }

    #[test]
    fn test_detect_shell_runs() {
        let (s, f) = detect_shell();
        assert!(!s.is_empty());
        assert!(!f.is_empty());
    }

    #[test]
    fn test_wrap_cmd_with_fd_limit_macos() {
        let wrapped = wrap_cmd_with_fd_limit("ls -la");
        if cfg!(target_os = "macos") {
            assert!(wrapped.starts_with("ulimit -n 10240"));
            assert!(wrapped.contains("ls -la"));
        } else {
            assert_eq!(wrapped, "ls -la");
        }
    }

    #[test]
    fn test_wrap_cmd_empty() {
        let wrapped = wrap_cmd_with_fd_limit("");
        if cfg!(target_os = "macos") {
            assert!(wrapped.contains("ulimit"));
        } else {
            assert_eq!(wrapped, "");
        }
    }

    #[test]
    fn test_wrap_cmd_complex() {
        let wrapped = wrap_cmd_with_fd_limit("echo 'hello' && ls");
        if cfg!(target_os = "macos") {
            assert!(wrapped.contains("echo 'hello' && ls"));
        }
    }
}
