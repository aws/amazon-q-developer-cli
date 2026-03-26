//! Self-update functionality for the Kiro CLI.
//!
//! This module provides the `kiro update` command which checks for and installs
//! updates from an S3-hosted version manifest.

mod download;
mod error;
mod installer;
mod manifest;
mod platform;
mod version;

use std::cmp::Ordering;
use std::process::ExitCode;

use clap::Args;
pub use download::InstallerDownloader;
pub use error::UpdateError;
use eyre::Result;
pub use installer::InstallerRunner;
pub use manifest::ManifestFetcher;
#[cfg(test)]
pub use manifest::{
    ArtifactEntry,
    VersionManifest,
};
pub use platform::Platform;
use tracing::{
    debug,
    info,
};
pub use version::VersionComparator;

use crate::os::Os;

/// Default manifest URL for production releases.
/// Can be overridden at runtime via Q_DESKTOP_RELEASE_URL env var or install.releaseUrl setting.
// Default manifest URL points to the production CDN release endpoint.
const DEFAULT_MANIFEST_URL: &str = crate::util::consts::env_var::DEFAULT_UPDATE_MANIFEST_URL;

/// CLI arguments for the update command
#[derive(Debug, Clone, Default, PartialEq, Eq, Args)]
pub struct UpdateArgs {
    /// Only check for updates without installing
    #[arg(long)]
    pub check: bool,

    /// Force installation even if already on latest version
    #[arg(long)]
    pub force: bool,
}

impl UpdateArgs {
    /// Execute the update command.
    ///
    /// This orchestrates the entire update process:
    /// 1. Detect the current platform
    /// 2. Fetch the version manifest
    /// 3. Compare versions
    /// 4. Download and install if needed (unless --check is specified)
    pub async fn execute(self, _os: &mut Os) -> Result<ExitCode> {
        // Get current version from Cargo.toml
        let current_version = env!("CARGO_PKG_VERSION");
        info!("Current version: {}", current_version);

        // Detect platform
        let platform = Platform::detect().map_err(|e| eyre::eyre!("{}", e))?;
        info!("Detected platform: {:?}", platform);

        // Get manifest URL from environment or use default
        let manifest_url = get_manifest_url();
        debug!("Using manifest URL: {}", manifest_url);

        // Fetch manifest
        println!("Checking for updates...");
        let fetcher = ManifestFetcher::new().map_err(|e| eyre::eyre!("{}", e))?;
        let manifest = fetcher.fetch(&manifest_url).await.map_err(|e| eyre::eyre!("{}", e))?;
        info!("Latest version: {}", manifest.version);

        // Compare versions
        let comparison =
            VersionComparator::compare(current_version, &manifest.version).map_err(|e| eyre::eyre!("{}", e))?;

        match comparison {
            Ordering::Equal => {
                println!("You are on the current version ({})", current_version);
                if !self.force {
                    return Ok(ExitCode::SUCCESS);
                }
                println!("--force specified, proceeding with installation anyway");
            },
            Ordering::Greater => {
                println!(
                    "You are on a newer version ({}) than the latest release ({})",
                    current_version, manifest.version
                );
                if !self.force {
                    return Ok(ExitCode::SUCCESS);
                }
                println!("--force specified, proceeding with installation anyway");
            },
            Ordering::Less => {
                println!("Update available: {} → {}", current_version, manifest.version);
            },
        }

        // If --check flag is set, don't install
        if self.check {
            return Ok(ExitCode::SUCCESS);
        }

        // Find artifact for current platform
        let os_name = platform.os_name();
        let arch = Platform::architecture();
        let artifact = manifest
            .find_artifact(os_name, arch)
            .ok_or_else(|| eyre::eyre!("No artifact found for {}/{}", os_name, arch))?;

        // Download installer — use the manifest base URL (ends at latest/) with just the filename.
        // The artifact's `download` field contains a versioned path (e.g., "1.28.2/file.msi")
        // but artifacts are also available at the `latest/` path, matching how the bash install
        // script downloads Mac/Linux artifacts.
        println!("Downloading installer...");
        let manifest_url = get_manifest_url();
        let base_url = manifest_url
            .rsplit_once('/')
            .map_or(manifest_url.as_str(), |(base, _)| base);
        let filename = artifact
            .download
            .rsplit_once('/')
            .map_or(artifact.download.as_str(), |(_, name)| name);
        let download_url = format!("{}/{}", base_url, filename);

        let downloader = InstallerDownloader::new().map_err(|e| eyre::eyre!("{}", e))?;
        let temp_installer = downloader
            .download(
                &download_url,
                &artifact.sha256,
                Some(|downloaded, total| {
                    if total > 0 {
                        let percent = (downloaded as f64 / total as f64 * 100.0) as u32;
                        print!("\rDownloading: {}%", percent);
                        let _ = std::io::Write::flush(&mut std::io::stdout());
                    }
                }),
            )
            .await
            .map_err(|e| eyre::eyre!("{}", e))?;
        println!(); // New line after progress

        // Run installer
        // On Windows with MSI, use the install-on-exit script to avoid the locked-exe problem.
        // The script waits for this process to exit, then runs msiexec silently.
        #[cfg(target_os = "windows")]
        if artifact.kind == "msi" {
            let args: Vec<String> = std::env::args().skip(1).collect();
            let msi_path = temp_installer.into_path();
            match InstallerRunner::launch_msi_trampoline(&msi_path, &args) {
                Ok(true) => {
                    println!("Update downloaded. Handing off to installer...");
                    return Ok(ExitCode::SUCCESS);
                },
                Ok(false) | Err(_) => {
                    // Trampoline didn't launch — fall back to direct msiexec.
                    // This works (MSI can replace a running exe) but the user
                    // needs to restart to use the new version.
                    println!("Installing update...");
                    match InstallerRunner::run_silent(&msi_path, &artifact.kind).await {
                        Ok(()) => {
                            let _ = std::fs::remove_file(&msi_path);
                            println!(
                                "Successfully updated to version {}. Please restart kiro-cli.",
                                manifest.version
                            );
                        },
                        Err(e) => {
                            eprintln!("Installation failed: {}", e);
                            eprintln!(
                                "Please close kiro-cli and run the installer manually: {}",
                                msi_path.display()
                            );
                        },
                    }
                    return Ok(ExitCode::SUCCESS);
                },
            }
        }

        println!("Installing update...");
        InstallerRunner::run_silent(temp_installer.path(), &artifact.kind)
            .await
            .map_err(|e| eyre::eyre!("{}", e))?;

        // Success message
        println!("Successfully updated to version {}", manifest.version);

        Ok(ExitCode::SUCCESS)
    }
}

/// Get the release URL, resolved in priority order:
/// 1. Runtime env var: `KIRO_DESKTOP_RELEASE_URL`
/// 2. User setting: `install.releaseUrl`
/// 3. Hardcoded default from `crate::util::consts::DEFAULT_UPDATE_MANIFEST_URL`
pub fn get_manifest_url() -> String {
    use crate::util::consts::env_var::KIRO_DESKTOP_RELEASE_URL;

    // 1. Runtime env var override
    if let Ok(url) = std::env::var(KIRO_DESKTOP_RELEASE_URL)
        && !url.is_empty()
    {
        return url;
    }

    // 2. User setting (install.releaseUrl)
    // Note: This requires a database handle. For now, fall through to default.
    // TODO: Pass Os/database reference to read install.releaseUrl setting here.

    // 3. Hardcoded default
    DEFAULT_MANIFEST_URL.to_string()
}

/// Parse and validate a URL scheme.
///
/// Returns the scheme ("https" or "s3") if valid, or an error if the URL is invalid.
#[cfg(test)]
pub fn parse_url_scheme(url: &str) -> Result<&str, UpdateError> {
    if url.starts_with("https://") {
        Ok("https")
    } else if url.starts_with("s3://") {
        Ok("s3")
    } else {
        Err(UpdateError::DownloadFailed(format!(
            "Unsupported URL scheme. Expected https:// or s3://, got: {}",
            url.split("://").next().unwrap_or("unknown")
        )))
    }
}

// Background update check and install-on-exit are Windows-only for now.
// Mac/Linux auto-update is handled by the autocomplete desktop app.
#[cfg(target_os = "windows")]
/// Timeout for the auto-update manifest check (seconds).
const AUTO_UPDATE_TIMEOUT_SECS: u64 = 10;

#[cfg(target_os = "windows")]
/// Result of a background update check — holds a staged installer path if an
/// update was downloaded and is ready to install on exit.
pub struct StagedUpdate {
    /// Path to the downloaded installer file.
    pub installer_path: std::path::PathBuf,
    /// Installer kind (e.g., "msi", "pkg", "tarXz").
    pub kind: String,
    /// The version that will be installed.
    pub version: String,
}

#[cfg(target_os = "windows")]
/// Handle returned by `start_background_update_check`.
pub type UpdateHandle = tokio::sync::watch::Receiver<Option<StagedUpdate>>;

/// Start a non-blocking background update check.
///
/// This spawns a background task that:
/// 1. Fetches the version manifest
/// 2. If an update is available, downloads the installer to a staging location
/// 3. Signals the result via the returned watch channel
///
/// The caller should:
/// - Continue startup immediately (this never blocks)
/// - Hold the returned `UpdateHandle`
/// - Call `install_staged_update` with the handle when the app exits
///
/// ## Auto-install behavior
///
/// The `auto_install` parameter controls whether the installer is actually run on exit:
/// - `true`: The installer will be run when `install_staged_update` is called
/// - `false`: Only logs that an update is available (download still happens for staging)
///
/// // FUTURE: To always enable auto-install, change the caller to pass `true`
/// // unconditionally instead of reading from the DisableAutoupdates setting.
#[cfg(target_os = "windows")]
pub fn start_background_update_check(auto_install: bool) -> UpdateHandle {
    let (tx, rx) = tokio::sync::watch::channel(None);

    tokio::spawn(async move {
        if let Some(staged) = background_update_check_inner().await {
            if auto_install {
                info!(
                    "Update {} → {} downloaded and staged for install on exit at {:?}",
                    env!("CARGO_PKG_VERSION"),
                    staged.version,
                    staged.installer_path
                );
            } else {
                // FUTURE: Remove this branch when auto-install is always enabled.
                // For now, just log the update availability at debug level.
                debug!(
                    "Update available: {} → {} (auto-install disabled, installer staged at {:?})",
                    env!("CARGO_PKG_VERSION"),
                    staged.version,
                    staged.installer_path
                );
            }
            let _ = tx.send(Some(staged));
        }
    });

    rx
}

/// Inner implementation of the background update check.
/// Returns `Some(StagedUpdate)` if an update was downloaded, `None` otherwise.
#[cfg(target_os = "windows")]
async fn background_update_check_inner() -> Option<StagedUpdate> {
    let current_version = env!("CARGO_PKG_VERSION");
    let manifest_url = get_manifest_url();

    // Fetch manifest with timeout
    let fetcher = match ManifestFetcher::new() {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to create manifest fetcher: {}", e);
            return None;
        },
    };

    let manifest = match tokio::time::timeout(
        std::time::Duration::from_secs(AUTO_UPDATE_TIMEOUT_SECS),
        fetcher.fetch(&manifest_url),
    )
    .await
    {
        Ok(Ok(m)) => m,
        Ok(Err(e)) => {
            debug!("Failed to fetch update manifest: {}", e);
            return None;
        },
        Err(_) => {
            debug!("Update check timed out");
            return None;
        },
    };

    // Compare versions
    let comparison = match VersionComparator::compare(current_version, &manifest.version) {
        Ok(c) => c,
        Err(e) => {
            debug!("Failed to compare versions: {}", e);
            return None;
        },
    };

    if comparison != Ordering::Less {
        debug!("Already up to date ({})", current_version);
        return None;
    }

    debug!("Update found: {} → {}", current_version, manifest.version);

    // Detect platform
    let platform = match Platform::detect() {
        Ok(p) => p,
        Err(e) => {
            debug!("Platform detection failed: {}", e);
            return None;
        },
    };

    let os_name = platform.os_name();
    let arch = Platform::architecture();
    let artifact = match manifest.find_artifact(os_name, arch) {
        Some(a) => a,
        None => {
            debug!("No artifact found for {}/{}", os_name, arch);
            return None;
        },
    };

    // Download installer silently in the background
    let downloader = match InstallerDownloader::new() {
        Ok(d) => d,
        Err(e) => {
            debug!("Failed to create downloader: {}", e);
            return None;
        },
    };

    let manifest_url = get_manifest_url();
    let base_url = manifest_url
        .rsplit_once('/')
        .map_or(manifest_url.as_str(), |(base, _)| base);
    let filename = artifact
        .download
        .rsplit_once('/')
        .map_or(artifact.download.as_str(), |(_, name)| name);
    let download_url = format!("{}/{}", base_url, filename);

    let temp_installer = match downloader
        .download::<fn(u64, u64)>(&download_url, &artifact.sha256, None)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            debug!("Background update download failed: {}", e);
            return None;
        },
    };

    // Preserve the file (don't let TempInstallerPath delete it on drop)
    let installer_path = temp_installer.into_path();

    Some(StagedUpdate {
        installer_path,
        kind: artifact.kind.clone(),
        version: manifest.version.clone(),
    })
}

/// Install a staged update on exit.
///
/// Call this when the application is shutting down. On Windows with MSI, this
/// spawns a detached batch script that waits for the current process to exit,
/// then runs the installer silently. On other platforms, it runs the installer
/// directly (since tar/pkg/deb don't need to replace the running binary).
///
/// If `auto_install` is false, this just cleans up the staged file and logs.
///
/// // FUTURE: To always enable auto-install, remove the `auto_install` parameter
/// // and always run the installer.
#[cfg(target_os = "windows")]
pub async fn install_staged_update(handle: UpdateHandle, auto_install: bool) {
    let staged = {
        let borrowed = handle.borrow();
        match &*borrowed {
            Some(s) => (s.installer_path.clone(), s.kind.clone(), s.version.clone()),
            None => return, // No update staged
        }
    };
    let (installer_path, kind, version) = staged;

    if !auto_install {
        // FUTURE: Remove this branch when auto-install is always enabled.
        debug!(
            "Staged update {} available but auto-install is disabled. Cleaning up.",
            version
        );
        let _ = std::fs::remove_file(&installer_path);
        return;
    }

    info!(
        "Installing staged update {} on exit (kind={}, path={:?})",
        version, kind, installer_path
    );

    // On Windows with MSI, use the install-on-exit script (wait for PID exit, then msiexec).
    // No relaunch — the user will get the new version next time they start.
    #[cfg(target_os = "windows")]
    if kind == "msi" {
        match InstallerRunner::launch_install_on_exit(&installer_path) {
            Ok(true) => {
                info!("Install-on-exit script launched for version {}", version);
                return;
            },
            Ok(false) | Err(_) => {
                debug!("Install-on-exit failed, falling back to direct install");
            },
        }
    }

    // Non-Windows or fallback: install directly
    if let Err(e) = InstallerRunner::run_silent(&installer_path, &kind).await {
        debug!("Staged update installation failed: {}", e);
    }
    let _ = std::fs::remove_file(&installer_path);
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Unit Tests for URL Configuration
    // =========================================================================

    #[test]
    fn test_default_manifest_url() {
        let url = get_manifest_url();
        assert!(url.starts_with("https://"));
    }

    #[test]
    fn test_parse_url_scheme_https() {
        let result = parse_url_scheme("https://example.com/manifest.json");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "https");
    }

    #[test]
    fn test_parse_url_scheme_s3() {
        let result = parse_url_scheme("s3://my-bucket/manifest.json");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "s3");
    }

    #[test]
    fn test_parse_url_scheme_invalid() {
        let result = parse_url_scheme("http://example.com/manifest.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_url_scheme_ftp() {
        let result = parse_url_scheme("ftp://example.com/file");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_url_scheme_no_scheme() {
        let result = parse_url_scheme("example.com/manifest.json");
        assert!(result.is_err());
    }

    // =========================================================================
    // Unit Tests for UpdateArgs
    // =========================================================================

    #[test]
    fn test_update_args_default() {
        let args = UpdateArgs::default();
        assert!(!args.check);
        assert!(!args.force);
    }

    #[test]
    fn test_update_args_check_flag() {
        let args = UpdateArgs {
            check: true,
            force: false,
        };
        assert!(args.check);
        assert!(!args.force);
    }

    #[test]
    fn test_update_args_force_flag() {
        let args = UpdateArgs {
            check: false,
            force: true,
        };
        assert!(!args.check);
        assert!(args.force);
    }

    #[test]
    fn test_update_args_both_flags() {
        let args = UpdateArgs {
            check: true,
            force: true,
        };
        assert!(args.check);
        assert!(args.force);
    }

    #[test]
    fn test_update_args_equality() {
        let args1 = UpdateArgs {
            check: true,
            force: false,
        };
        let args2 = UpdateArgs {
            check: true,
            force: false,
        };
        let args3 = UpdateArgs {
            check: false,
            force: false,
        };

        assert_eq!(args1, args2);
        assert_ne!(args1, args3);
    }

    #[test]
    fn test_update_args_clone() {
        let args = UpdateArgs {
            check: true,
            force: true,
        };
        let cloned = args.clone();
        assert_eq!(args, cloned);
    }

    // =========================================================================
    // Unit Tests for Version Comparison Messaging Logic
    // Requirements: 5.2, 5.3
    // =========================================================================

    #[test]
    fn test_version_comparison_update_available() {
        // When current < latest, update is available
        let comparison = VersionComparator::compare("1.0.0", "2.0.0").unwrap();
        assert_eq!(comparison, Ordering::Less);
    }

    #[test]
    fn test_version_comparison_up_to_date() {
        // When current == latest, already up to date
        let comparison = VersionComparator::compare("1.0.0", "1.0.0").unwrap();
        assert_eq!(comparison, Ordering::Equal);
    }

    #[test]
    fn test_version_comparison_newer_than_latest() {
        // When current > latest, on a newer version
        let comparison = VersionComparator::compare("2.0.0", "1.0.0").unwrap();
        assert_eq!(comparison, Ordering::Greater);
    }

    // =========================================================================
    // Unit Tests for Exit Code Handling
    // Requirements: 8.3, 8.4
    // =========================================================================

    #[test]
    fn test_exit_code_success_is_success() {
        // ExitCode::SUCCESS should represent success (exit code 0)
        let success = ExitCode::SUCCESS;
        // Verify it's the success variant by checking the debug output contains "0"
        let debug_str = format!("{:?}", success);
        assert!(debug_str.contains("0"), "SUCCESS should contain 0: {}", debug_str);
    }

    #[test]
    fn test_exit_code_failure_is_failure() {
        // ExitCode::FAILURE should represent failure (exit code 1)
        let failure = ExitCode::FAILURE;
        // Verify it's the failure variant by checking the debug output contains "1"
        let debug_str = format!("{:?}", failure);
        assert!(debug_str.contains("1"), "FAILURE should contain 1: {}", debug_str);
    }

    #[test]
    fn test_exit_codes_are_different() {
        // SUCCESS and FAILURE should be different
        let success_debug = format!("{:?}", ExitCode::SUCCESS);
        let failure_debug = format!("{:?}", ExitCode::FAILURE);
        assert_ne!(success_debug, failure_debug);
    }

    // =========================================================================
    // Unit Tests for Artifact Selection
    // =========================================================================

    #[test]
    fn test_artifact_selection_linux() {
        let manifest = create_test_manifest();
        let artifact = manifest.find_artifact("linux", "x86_64");
        assert!(artifact.is_some());
        assert!(artifact.unwrap().download.contains("linux"));
    }

    #[test]
    fn test_artifact_selection_macos() {
        let manifest = create_test_manifest();
        let artifact = manifest.find_artifact("macos", "aarch64");
        assert!(artifact.is_some());
        assert!(artifact.unwrap().download.contains("macos"));
    }

    #[test]
    fn test_artifact_selection_windows() {
        let manifest = create_test_manifest();
        let artifact = manifest.find_artifact("windows", "x86_64");
        assert!(artifact.is_some());
        assert!(artifact.unwrap().download.contains("windows"));
    }

    #[test]
    fn test_artifact_selection_not_found() {
        let manifest = create_test_manifest();
        let artifact = manifest.find_artifact("freebsd", "x86_64");
        assert!(artifact.is_none());
    }

    /// Helper function to create a test manifest
    fn create_test_manifest() -> VersionManifest {
        VersionManifest {
            version: "2.0.0".to_string(),
            packages: vec![
                ArtifactEntry {
                    kind: "tarXz".to_string(),
                    target_triple: "x86_64-unknown-linux-gnu".to_string(),
                    os: "linux".to_string(),
                    file_type: "tarXz".to_string(),
                    architecture: "x86_64".to_string(),
                    variant: "headless".to_string(),
                    download: "nightly/2.0.0/kiro-linux-x86_64.tar.xz".to_string(),
                    sha256: "a".repeat(64),
                    size: 100000,
                    channel: "nightly".to_string(),
                },
                ArtifactEntry {
                    kind: "pkg".to_string(),
                    target_triple: "aarch64-apple-darwin".to_string(),
                    os: "macos".to_string(),
                    file_type: "pkg".to_string(),
                    architecture: "aarch64".to_string(),
                    variant: "full".to_string(),
                    download: "nightly/2.0.0/kiro-macos-aarch64.pkg".to_string(),
                    sha256: "b".repeat(64),
                    size: 200000,
                    channel: "nightly".to_string(),
                },
                ArtifactEntry {
                    kind: "msi".to_string(),
                    target_triple: "x86_64-pc-windows-msvc".to_string(),
                    os: "windows".to_string(),
                    file_type: "msi".to_string(),
                    architecture: "x86_64".to_string(),
                    variant: "full".to_string(),
                    download: "nightly/2.0.0/kiro-windows-x86_64.msi".to_string(),
                    sha256: "c".repeat(64),
                    size: 150000,
                    channel: "nightly".to_string(),
                },
            ],
        }
    }

    // =========================================================================
    // Unit Tests for CLI Argument Parsing (via clap)
    // Requirements: 1.1, 1.3, 1.4
    // =========================================================================

    #[test]
    fn test_clap_parsing_no_flags() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            update: UpdateArgs,
        }

        let cli = TestCli::parse_from(["test"]);
        assert!(!cli.update.check);
        assert!(!cli.update.force);
    }

    #[test]
    fn test_clap_parsing_check_flag() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            update: UpdateArgs,
        }

        let cli = TestCli::parse_from(["test", "--check"]);
        assert!(cli.update.check);
        assert!(!cli.update.force);
    }

    #[test]
    fn test_clap_parsing_force_flag() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            update: UpdateArgs,
        }

        let cli = TestCli::parse_from(["test", "--force"]);
        assert!(!cli.update.check);
        assert!(cli.update.force);
    }

    #[test]
    fn test_clap_parsing_both_flags() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            update: UpdateArgs,
        }

        let cli = TestCli::parse_from(["test", "--check", "--force"]);
        assert!(cli.update.check);
        assert!(cli.update.force);
    }
}
