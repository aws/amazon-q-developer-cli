use std::sync::OnceLock;

/// Returns the platform-appropriate shell and its command-execution flag.
///
/// - **Unix**: `("bash", "-c")`
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
        ("bash", "-c")
    }
}

/// Detects the parent shell on Windows by walking the process tree.
///
/// Looks for `powershell.exe` or `pwsh.exe` among ancestor processes.
/// If found, returns the matching shell with `-Command` flag.
/// Otherwise falls back to `cmd /C`.
#[cfg(windows)]
fn detect_windows_shell() -> (&'static str, &'static str) {
    use sysinfo::{
        Pid,
        ProcessRefreshKind,
        RefreshKind,
        System,
    };

    let sys = System::new_with_specifics(RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing()));

    let mut pid = Pid::from_u32(std::process::id());

    // Walk up to 10 ancestors to avoid infinite loops from pid recycling
    for _ in 0..10 {
        let Some(proc) = sys.process(pid) else {
            break;
        };
        let name = proc.name().to_string_lossy().to_ascii_lowercase();
        if name == "pwsh.exe" || name == "pwsh" {
            return ("pwsh", "-Command");
        }
        if name == "powershell.exe" || name == "powershell" {
            return ("powershell", "-Command");
        }
        let Some(parent) = proc.parent() else {
            break;
        };
        if parent == pid {
            break;
        }
        pid = parent;
    }

    ("cmd", "/C")
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
            assert_eq!(shell, "bash");
            assert_eq!(flag, "-c");
        }
    }
}
