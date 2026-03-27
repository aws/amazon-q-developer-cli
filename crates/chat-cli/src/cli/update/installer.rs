//! Platform-specific installer execution.
//!
//! This module provides the `InstallerRunner` for executing downloaded installers
//! silently. It dispatches to the appropriate installation method based on the
//! artifact's `kind` field from the manifest.
//!
//! ## Windows Update Pattern
//!
//! On Windows, MSI installation needs the running executable to be unlocked.
//! To handle this, we use an "install-on-exit" approach:
//!
//! 1. Write a .cmd batch script to a temp file
//! 2. The script waits for the current process to exit
//! 3. Runs `msiexec` to install the MSI silently
//! 4. Cleans up the script and MSI files

use std::path::Path;
#[cfg(target_os = "windows")]
use std::process::Stdio;

#[cfg(target_os = "windows")]
use tokio::process::Command;
use tracing::{
    info,
    warn,
};

use super::error::UpdateError;

/// Executes installers silently based on the artifact kind.
pub struct InstallerRunner;

impl InstallerRunner {
    /// Execute the installer silently based on the artifact kind.
    ///
    /// Dispatches to the appropriate installation method:
    /// - `msi` → `msiexec /i <path> /quiet /norestart`
    /// - `pkg` → `sudo installer -pkg <path> -target /`
    /// - `deb` → `sudo dpkg -i <path>`
    /// - `tarGz` → `tar -xzf <path> -C <target>`
    /// - `tarXz` → `tar -xJf <path> -C <target>`
    pub async fn run_silent(installer_path: &Path, kind: &str) -> Result<(), UpdateError> {
        info!("Running installer kind={} path={:?}", kind, installer_path);

        match kind {
            "msi" => Self::run_msi(installer_path).await,
            "pkg" => Self::run_pkg(installer_path).await,
            "deb" => Self::run_deb(installer_path).await,
            "tarGz" => Self::run_tar(installer_path, "-xzf").await,
            "tarXz" => Self::run_tar(installer_path, "-xJf").await,
            other => Err(UpdateError::InstallationFailed {
                code: -1,
                message: format!("Unsupported installer kind: {}", other),
            }),
        }
    }

    /// Generate an install-only batch script (no relaunch).
    ///
    /// Used for background updates that install on exit — the user will get the
    /// new version next time they start the app.
    #[cfg(target_os = "windows")]
    pub fn generate_install_only_script(msi_path: &str, parent_pid: u32) -> String {
        format!(
            r#"@echo off
REM Kiro CLI background update installer — installs after the app exits.
REM Wait for the old process (PID {pid}) to exit so the exe is unlocked.
:WAIT_LOOP
tasklist /FI "PID eq {pid}" 2>NUL | find /I "{pid}" >NUL
if %ERRORLEVEL%==0 (
    ping -n 2 127.0.0.1 >NUL
    goto WAIT_LOOP
)
REM Small extra delay to ensure file handles are released
ping -n 2 127.0.0.1 >NUL

REM Run the MSI installer silently.
start /wait msiexec /i "{msi_path}" /quiet /norestart

REM Clean up the MSI.
del /f /q "{msi_path}" 2>NUL

REM Clean up this script.
(goto) 2>NUL & del /f /q "%~f0"
"#,
            pid = parent_pid,
            msi_path = msi_path,
        )
    }

    /// Launch an install-only trampoline that runs the MSI after this process exits.
    /// No relaunch — the user gets the new version next time they start the app.
    #[cfg(target_os = "windows")]
    pub fn launch_install_on_exit(installer_path: &Path) -> Result<bool, UpdateError> {
        let msi_path = Self::path_str(installer_path)?;
        let pid = std::process::id();

        let script = Self::generate_install_only_script(msi_path, pid);

        let script_path = std::env::temp_dir().join(format!("kiro-update-install-{}.cmd", uuid::Uuid::new_v4()));
        std::fs::write(&script_path, &script)?;
        info!("Wrote install-on-exit script to {:?}", script_path);

        // Spawn detached — we don't need to inherit console since there's no relaunch.
        let child = std::process::Command::new("cmd.exe")
            .args(["/C", &script_path.to_string_lossy()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        match child {
            Ok(_) => {
                info!("Install-on-exit script launched, will run after PID {} exits", pid);
                Ok(true)
            },
            Err(e) => {
                let _ = std::fs::remove_file(&script_path);
                Err(UpdateError::InstallationFailed {
                    code: -1,
                    message: format!("Failed to launch install-on-exit script: {}", e),
                })
            },
        }
    }

    /// Execute Windows MSI installer silently (direct, non-trampoline).
    #[cfg(target_os = "windows")]
    async fn run_msi(installer_path: &Path) -> Result<(), UpdateError> {
        let path_str = Self::path_str(installer_path)?;
        info!("Executing: msiexec /i {} /quiet /norestart", path_str);

        let output = Command::new("msiexec")
            .args(["/i", path_str, "/quiet", "/norestart"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        Self::check_exit_status(output, "msiexec")
    }

    #[cfg(not(target_os = "windows"))]
    async fn run_msi(installer_path: &Path) -> Result<(), UpdateError> {
        let path_str = Self::path_str(installer_path)?;
        warn!(
            "MSI installation not supported on this platform. Would run: msiexec /i {} /quiet /norestart",
            path_str
        );
        Err(UpdateError::InstallationFailed {
            code: -1,
            message: "MSI installation is only supported on Windows".to_string(),
        })
    }

    /// Execute macOS PKG installer silently.
    /// TODO: Implement when we release a self-updating binary on macOS.
    #[cfg(target_os = "macos")]
    async fn run_pkg(_installer_path: &Path) -> Result<(), UpdateError> {
        Err(UpdateError::InstallationFailed {
            code: -1,
            message: "macOS PKG auto-install is not yet implemented. macOS updates are managed by the desktop app."
                .to_string(),
        })
    }

    #[cfg(not(target_os = "macos"))]
    async fn run_pkg(_installer_path: &Path) -> Result<(), UpdateError> {
        Err(UpdateError::InstallationFailed {
            code: -1,
            message: "PKG installation is only supported on macOS".to_string(),
        })
    }

    /// Execute Debian package installation.
    /// TODO: Implement when we release a self-updating binary on Linux.
    #[cfg(target_os = "linux")]
    async fn run_deb(_installer_path: &Path) -> Result<(), UpdateError> {
        Err(UpdateError::InstallationFailed {
            code: -1,
            message: "Linux DEB auto-install is not yet implemented. Linux updates are managed by the desktop app."
                .to_string(),
        })
    }

    #[cfg(not(target_os = "linux"))]
    async fn run_deb(_installer_path: &Path) -> Result<(), UpdateError> {
        Err(UpdateError::InstallationFailed {
            code: -1,
            message: "DEB installation is only supported on Linux".to_string(),
        })
    }

    /// Execute tar extraction (supports both gzip and xz).
    /// TODO: Implement when we release a self-updating binary on Linux/macOS.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    async fn run_tar(_installer_path: &Path, _tar_flag: &str) -> Result<(), UpdateError> {
        Err(UpdateError::InstallationFailed {
            code: -1,
            message: "Tar-based auto-install is not yet implemented. Updates on this platform are managed by the desktop app.".to_string(),
        })
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    async fn run_tar(_installer_path: &Path, _tar_flag: &str) -> Result<(), UpdateError> {
        Err(UpdateError::InstallationFailed {
            code: -1,
            message: "Tar extraction is only supported on Linux/macOS".to_string(),
        })
    }

    fn path_str(path: &Path) -> Result<&str, UpdateError> {
        path.to_str().ok_or_else(|| UpdateError::InstallationFailed {
            code: -1,
            message: "Invalid installer path encoding".to_string(),
        })
    }

    #[allow(dead_code)] // Used by run_msi on Windows, needed by tests on all platforms
    fn check_exit_status(output: std::process::Output, command_name: &str) -> Result<(), UpdateError> {
        if output.status.success() {
            info!("{} completed successfully", command_name);
            Ok(())
        } else {
            let code = output.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let message = if !stderr.is_empty() {
                stderr.to_string()
            } else if !stdout.is_empty() {
                stdout.to_string()
            } else {
                format!("{} failed with no output", command_name)
            };
            warn!("{} failed with exit code {}: {}", command_name, code, message);
            Err(UpdateError::InstallationFailed {
                code,
                message: message.trim().to_string(),
            })
        }
    }
}

/// Build the command arguments for each installer type (exposed for testing).
#[allow(dead_code)]
pub mod command_builder {
    use std::path::Path;

    pub fn msi_args(installer_path: &Path) -> Vec<String> {
        vec![
            "/i".into(),
            installer_path.to_string_lossy().into(),
            "/quiet".into(),
            "/norestart".into(),
        ]
    }

    pub fn pkg_args(installer_path: &Path) -> Vec<String> {
        vec![
            "installer".into(),
            "-pkg".into(),
            installer_path.to_string_lossy().into(),
            "-target".into(),
            "/".into(),
        ]
    }

    pub fn deb_args(installer_path: &Path) -> Vec<String> {
        vec!["dpkg".into(), "-i".into(), installer_path.to_string_lossy().into()]
    }

    pub fn tar_args(installer_path: &Path, target_dir: &Path, flag: &str) -> Vec<String> {
        vec![
            flag.into(),
            installer_path.to_string_lossy().into(),
            "-C".into(),
            target_dir.to_string_lossy().into(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_msi_command_construction() {
        let args = command_builder::msi_args(&PathBuf::from("/path/to/installer.msi"));
        assert_eq!(args, vec!["/i", "/path/to/installer.msi", "/quiet", "/norestart"]);
    }

    #[test]
    fn test_pkg_command_construction() {
        let args = command_builder::pkg_args(&PathBuf::from("/path/to/installer.pkg"));
        assert_eq!(args, vec![
            "installer",
            "-pkg",
            "/path/to/installer.pkg",
            "-target",
            "/"
        ]);
    }

    #[test]
    fn test_deb_command_construction() {
        let args = command_builder::deb_args(&PathBuf::from("/path/to/installer.deb"));
        assert_eq!(args, vec!["dpkg", "-i", "/path/to/installer.deb"]);
    }

    #[test]
    fn test_tar_gz_command_construction() {
        let args = command_builder::tar_args(
            &PathBuf::from("/tmp/kiro.tar.gz"),
            &PathBuf::from("/usr/local/bin"),
            "-xzf",
        );
        assert_eq!(args, vec!["-xzf", "/tmp/kiro.tar.gz", "-C", "/usr/local/bin"]);
    }

    #[test]
    fn test_tar_xz_command_construction() {
        let args = command_builder::tar_args(
            &PathBuf::from("/tmp/kiro.tar.xz"),
            &PathBuf::from("/usr/local/bin"),
            "-xJf",
        );
        assert_eq!(args, vec!["-xJf", "/tmp/kiro.tar.xz", "-C", "/usr/local/bin"]);
    }

    #[tokio::test]
    async fn test_unsupported_kind() {
        let result = InstallerRunner::run_silent(&PathBuf::from("/tmp/test"), "rpm").await;
        assert!(matches!(result, Err(UpdateError::InstallationFailed { .. })));
    }

    #[test]
    fn test_check_exit_status_success() {
        #[cfg(unix)]
        {
            let output = std::process::Command::new("true").output().unwrap();
            assert!(InstallerRunner::check_exit_status(output, "true").is_ok());
        }
    }

    #[test]
    fn test_check_exit_status_failure() {
        #[cfg(unix)]
        {
            let output = std::process::Command::new("sh")
                .args(["-c", "echo err >&2; exit 1"])
                .output()
                .unwrap();
            let result = InstallerRunner::check_exit_status(output, "test");
            assert!(matches!(result, Err(UpdateError::InstallationFailed { code: 1, .. })));
        }
    }
}
