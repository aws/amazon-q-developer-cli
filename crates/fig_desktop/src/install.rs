use std::sync::Arc;

#[cfg(not(target_os = "linux"))]
use fig_install::check_for_updates;
use fig_integrations::Integration;
use fig_integrations::ssh::SshIntegration;
use fig_os_shim::Context;
#[cfg(target_os = "macos")]
use fig_util::directories::fig_data_dir;
#[cfg(target_os = "macos")]
use macos_utils::bundle::get_bundle_path_for_executable;
use semver::Version;
use tracing::{
    error,
    info,
};

#[allow(unused_imports)]
use crate::utils::is_cargo_debug_build;

const PREVIOUS_VERSION_KEY: &str = "desktop.versionAtPreviousLaunch";

#[cfg(target_os = "macos")]
const MIGRATED_KEY: &str = "desktop.migratedFromFig";

#[cfg(target_os = "macos")]
pub async fn migrate_data_dir() {
    // Migrate the user data dir
    if let (Ok(old), Ok(new)) = (fig_util::directories::old_fig_data_dir(), fig_data_dir()) {
        if !old.is_symlink() && old.is_dir() && !new.is_dir() {
            match tokio::fs::rename(&old, &new).await {
                Ok(()) => {
                    if let Err(err) = symlink(&new, &old).await {
                        error!(%err, "Failed to symlink old user data dir");
                    }
                },
                Err(err) => {
                    error!(%err, "Failed to migrate user data dir");
                },
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn run_input_method_migration() {
    use fig_integrations::input_method::InputMethod;
    use tokio::time::{
        Duration,
        sleep,
    };
    use tracing::warn;

    let input_method = InputMethod::default();
    match input_method.target_bundle_path() {
        Ok(target_bundle_path) if target_bundle_path.exists() => {
            tokio::spawn(async move {
                input_method.terminate().ok();
                if let Err(err) = input_method.migrate().await {
                    warn!(%err, "Failed to migrate input method");
                }

                sleep(Duration::from_secs(1)).await;
                input_method.launch();
            });
        },
        Ok(_) => warn!("Input method bundle path does not exist"),
        Err(err) => warn!(%err, "Failed to get input method bundle path"),
    }
}

/// Run items at launch
#[allow(unused_variables)]
pub async fn run_install(ctx: Arc<Context>, ignore_immediate_update: bool) {
    #[cfg(target_os = "macos")]
    {
        initialize_fig_dir(&fig_os_shim::Env::new()).await.ok();

        if fig_util::directories::home_dir()
            .map(|home| home.join("Library/Application Support/fig/credentials.json"))
            .is_ok_and(|path| path.exists())
            && !fig_settings::state::get_bool_or(MIGRATED_KEY, false)
        {
            let set = fig_settings::state::set_value(MIGRATED_KEY, true);
            if set.is_ok() {
                fig_telemetry::send_fig_user_migrated().await;
            }
        }
    }

    #[cfg(target_os = "macos")]
    // Add any items that are only once per version
    if should_run_install_script() {
        run_input_method_migration();
    }

    #[cfg(target_os = "linux")]
    run_linux_install(
        Arc::clone(&ctx),
        Arc::new(fig_settings::Settings::new()),
        Arc::new(fig_settings::State::new()),
    )
    .await;

    if let Err(err) = set_previous_version(current_version()) {
        error!(%err, "Failed to set previous version");
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Update if there's a newer version
        if !ignore_immediate_update && !is_cargo_debug_build() {
            use std::time::Duration;

            use tokio::time::timeout;
            // Check for updates but timeout after 3 seconds to avoid making the user wait too long
            // todo: don't download the index file twice
            match timeout(Duration::from_secs(3), check_for_updates(true)).await {
                Ok(Ok(Some(_))) => {
                    crate::update::check_for_update(true, true).await;
                },
                Ok(Ok(None)) => error!("No update found"),
                Ok(Err(err)) => error!(%err, "Failed to check for updates"),
                Err(err) => error!(%err, "Update check timed out"),
            }
        }

        tokio::spawn(async {
            let seconds = fig_settings::settings::get_int_or("app.autoupdate.check-period", 60 * 60 * 3);
            if seconds < 0 {
                return;
            }
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(seconds as u64));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            interval.tick().await;
            loop {
                interval.tick().await;
                // TODO: we need to determine if the dashboard is open here and pass that as the second bool
                crate::update::check_for_update(false, false).await;
            }
        });

        // remove the updater if it exists
        #[cfg(target_os = "windows")]
        std::fs::remove_file(fig_util::directories::fig_dir().unwrap().join("fig_installer.exe")).ok();
    }

    // install vscode integration
    #[cfg(target_os = "macos")]
    for variant in fig_integrations::vscode::variants_installed() {
        let integration = fig_integrations::vscode::VSCodeIntegration { variant };
        if integration.is_installed().await.is_err() {
            info!(
                "Attempting to install vscode integration for variant {}",
                integration.variant.application_name
            );
            if let Err(err) = integration.install().await {
                error!(%err, "Failed installing vscode integration for variant {}", integration.variant.application_name);
            }
        }
    }

    // install intellij integration
    #[cfg(target_os = "macos")]
    match fig_integrations::intellij::variants_installed().await {
        Ok(variants) => {
            for integration in variants {
                if integration.is_installed().await.is_err() {
                    info!(
                        "Attempting to install intellij integration for variant {}",
                        integration.variant.application_name()
                    );
                    if let Err(err) = integration.install().await {
                        error!(%err, "Failed installing intellij integration for variant {}", integration.variant.application_name());
                    }
                }
            }
        },
        Err(err) => error!(%err, "Failed getting installed intellij variants"),
    }

    // update ssh integration
    if let Ok(ssh_integration) = SshIntegration::new() {
        if let Err(err) = ssh_integration.reinstall().await {
            error!(%err, "Failed updating ssh integration");
        }
    }
}

/// Symlink, and overwrite if it already exists and is invalid or not a symlink
#[cfg(target_os = "macos")]
async fn symlink(src: impl AsRef<std::path::Path>, dst: impl AsRef<std::path::Path>) -> Result<(), std::io::Error> {
    use std::io::ErrorKind;

    let src = src.as_ref();
    let dst = dst.as_ref();

    // Check if the link already exists
    match tokio::fs::symlink_metadata(dst).await {
        Ok(metadata) => {
            // If it's a symlink, check if it points to the right place
            if metadata.file_type().is_symlink() {
                if let Ok(read_link) = tokio::fs::read_link(dst).await {
                    if read_link == src {
                        return Ok(());
                    }
                }
            }

            // If it's not a symlink or it points to the wrong place, delete it
            tokio::fs::remove_file(dst).await?;
        },
        Err(err) if err.kind() == ErrorKind::NotFound => {},
        Err(err) => return Err(err),
    }

    // Create the symlink
    tokio::fs::symlink(src, dst).await
}

#[cfg(target_os = "macos")]
pub async fn initialize_fig_dir(env: &fig_os_shim::Env) -> anyhow::Result<()> {
    use std::fs;

    use fig_integrations::shell::ShellExt;
    use fig_util::consts::{
        APP_BUNDLE_ID,
        APP_PROCESS_NAME,
        CLI_BINARY_NAME,
        PTY_BINARY_NAME,
    };
    use fig_util::directories::home_dir;
    use fig_util::launchd_plist::{
        LaunchdPlist,
        create_launch_agent,
    };
    use fig_util::{
        CHAT_BINARY_NAME,
        OLD_CLI_BINARY_NAMES,
        OLD_PTY_BINARY_NAMES,
        Shell,
    };
    use macos_utils::bundle::get_bundle_path;
    use tracing::warn;

    let local_bin = fig_util::directories::home_local_bin()?;
    if let Err(err) = fs::create_dir_all(&local_bin) {
        error!(%err, "Failed to create {local_bin:?}");
    }

    // Install figterm to ~/.local/bin
    match get_bundle_path_for_executable(PTY_BINARY_NAME) {
        Some(pty_path) => {
            let link = local_bin.join(PTY_BINARY_NAME);
            if let Err(err) = symlink(&pty_path, link).await {
                error!(%err, "Failed to symlink for {PTY_BINARY_NAME}: {pty_path:?}");
            }

            for old_pty_binary_name in OLD_PTY_BINARY_NAMES {
                let old_pty_binary_path = local_bin.join(old_pty_binary_name);
                if old_pty_binary_path.exists() {
                    if let Err(err) = tokio::fs::remove_file(&old_pty_binary_path).await {
                        warn!(%err, "Failed to remove {old_pty_binary_name}: {old_pty_binary_path:?}");
                    }
                }
            }

            for shell in Shell::all() {
                let pty_shell_cpy = local_bin.join(format!("{shell} ({PTY_BINARY_NAME})"));
                let pty_path = pty_path.clone();

                tokio::spawn(async move {
                    // Check version if copy already exists, this is because everytime a copy is made the first start is
                    // kinda slow and we want to avoid that
                    if pty_shell_cpy.exists() {
                        let output = tokio::process::Command::new(&pty_shell_cpy)
                            .arg("--version")
                            .output()
                            .await
                            .ok();

                        let version = output
                            .as_ref()
                            .and_then(|output| std::str::from_utf8(&output.stdout).ok())
                            .map(|s| {
                                match s.strip_prefix(PTY_BINARY_NAME) {
                                    Some(s) => s,
                                    None => s,
                                }
                                .trim()
                            });

                        if version == Some(env!("CARGO_PKG_VERSION")) {
                            return;
                        }
                    }

                    if let Err(err) = tokio::fs::remove_file(&pty_shell_cpy).await {
                        error!(%err, "Failed to remove {PTY_BINARY_NAME} shell {shell:?} copy");
                    }
                    if let Err(err) = tokio::fs::copy(&pty_path, &pty_shell_cpy).await {
                        error!(%err, "Failed to copy {PTY_BINARY_NAME} to {}", pty_shell_cpy.display());
                    }
                });

                for old_pty_binary_name in OLD_PTY_BINARY_NAMES {
                    // Remove legacy pty shell copies
                    let old_pty_binary_path = local_bin.join(format!("{shell} ({old_pty_binary_name})"));
                    if old_pty_binary_path.exists() {
                        if let Err(err) = tokio::fs::remove_file(&old_pty_binary_path).await {
                            warn!(%err, "Failed to remove legacy pty: {old_pty_binary_path:?}");
                        }
                    }
                }
            }
        },
        None => error!("Failed to find {PTY_BINARY_NAME} in bundle"),
    }

    // install the cli to ~/.local/bin
    match get_bundle_path_for_executable(CLI_BINARY_NAME) {
        Some(q_cli_path) => {
            let dest = local_bin.join(CLI_BINARY_NAME);
            if let Err(err) = symlink(&q_cli_path, dest).await {
                error!(%err, "Failed to symlink {CLI_BINARY_NAME}");
            }

            for old_cli_binary_name in OLD_CLI_BINARY_NAMES {
                let old_cli_binary_path = local_bin.join(old_cli_binary_name);
                if old_cli_binary_path.is_symlink() {
                    if let Err(err) = symlink(&q_cli_path, &old_cli_binary_path).await {
                        warn!(%err, "Failed to symlink legacy CLI: {old_cli_binary_path:?}");
                    }
                }
            }
        },
        None => error!("Failed to find {CLI_BINARY_NAME} in bundle"),
    }

    // install chat to ~/.local/bin
    match get_bundle_path_for_executable(CHAT_BINARY_NAME) {
        Some(qchat_path) => {
            let dest = local_bin.join(CHAT_BINARY_NAME);
            if let Err(err) = symlink(&qchat_path, dest).await {
                error!(%err, "Failed to symlink {CHAT_BINARY_NAME}");
            }
        },
        None => error!("Failed to find {CHAT_BINARY_NAME} in bundle"),
    }

    if let Some(bundle_path) = get_bundle_path() {
        let exe = bundle_path.join("Contents").join("MacOS").join(APP_PROCESS_NAME);
        let startup_launch_agent = LaunchdPlist::new("com.amazon.codewhisperer.launcher")
            .program_arguments([&exe.to_string_lossy(), "--is-startup", "--no-dashboard"])
            .associated_bundle_identifiers([APP_BUNDLE_ID])
            .run_at_load(true);

        create_launch_agent(&startup_launch_agent)?;

        let path = startup_launch_agent.get_file_path()?;
        std::process::Command::new("launchctl")
            .arg("load")
            .arg(&path)
            .status()
            .ok();
    }

    if let Ok(home) = home_dir() {
        let iterm_integration_path = home
            .join("Library")
            .join("Application Support")
            .join("iTerm2")
            .join("Scripts")
            .join("AutoLaunch")
            .join("fig-iterm-integration.scpt");

        if iterm_integration_path.exists() {
            std::fs::remove_file(&iterm_integration_path).ok();
        }
    }

    // Init the shell directory
    std::fs::create_dir(fig_data_dir()?.join("shell")).ok();
    for shell in fig_util::Shell::all().iter() {
        for script_integration in shell.get_script_integrations().unwrap_or_default() {
            if let Err(err) = script_integration.install().await {
                error!(%err, "Failed installing shell integration {}", script_integration.describe());
            }
        }

        for shell_integration in shell.get_shell_integrations(env).unwrap_or_default() {
            if let Err(err) = shell_integration.migrate().await {
                error!(%err, "Failed installing shell integration {}", shell_integration.describe());
            }
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
async fn run_linux_install(ctx: Arc<Context>, settings: Arc<fig_settings::Settings>, state: Arc<fig_settings::State>) {
    use dbus::gnome_shell::ShellExtensions;
    use fig_settings::State;
    use fig_util::system_info::linux::get_display_server;

    // install binaries under home local bin
    if ctx.env().in_appimage() {
        let ctx_clone = Arc::clone(&ctx);
        tokio::spawn(async move {
            install_appimage_binaries(&ctx_clone)
                .await
                .map_err(|err| error!(?err, "Unable to install binaries under the local bin directory"))
                .ok();
        });
    }

    // Important we log an error if we cannot detect the display server in use.
    // If this isn't wayland or x11, the user will probably just see a blank screen.
    match get_display_server(&ctx) {
        Ok(_) => (),
        Err(fig_util::Error::UnknownDisplayServer(server)) => {
            error!(
                "Unknown value set for XDG_SESSION_TYPE: {}. This must be set to x11 or wayland.",
                server
            );
        },
        Err(err) => {
            error!(
                "Unknown error occurred when detecting the display server: {:?}. Is XDG_SESSION_TYPE set to x11 or wayland?",
                err
            );
        },
    }

    // GNOME Shell Extension
    {
        let ctx_clone = Arc::clone(&ctx);
        tokio::spawn(async move {
            let ctx = ctx_clone;
            let shell_extensions = ShellExtensions::new(Arc::downgrade(&ctx));
            let state = State::new();
            install_gnome_shell_extension(&ctx, &shell_extensions, &state)
                .await
                .map_err(|err| error!(?err, "Unable to install the GNOME Shell extension"))
                .ok();
        });
    }

    // Desktop entry
    {
        let ctx_clone = Arc::clone(&ctx);
        let settings_clone = Arc::clone(&settings);
        let state_clone = Arc::clone(&state);
        tokio::spawn(async move {
            install_desktop_entry(&ctx_clone, &state_clone)
                .await
                .map_err(|err| error!(?err, "Unable to install desktop entry"))
                .ok();
            install_autostart_entry(&ctx_clone, &settings_clone, &state_clone)
                .await
                .map_err(|err| error!(?err, "Unable to install autostart entry"))
                .ok();
        });
    }

    // TODO: is this correct?
    // launch_ibus().await;
}

/// Installs the correct version of the Amazon Q for CLI GNOME Shell extension, if required.
#[cfg(target_os = "linux")]
async fn install_gnome_shell_extension<Ctx, ExtensionsCtx>(
    ctx: &Ctx,
    shell_extensions: &dbus::gnome_shell::ShellExtensions<ExtensionsCtx>,
    state: &fig_settings::State,
) -> anyhow::Result<()>
where
    Ctx: fig_os_shim::ContextProvider,
    ExtensionsCtx: fig_os_shim::ContextProvider,
{
    use dbus::gnome_shell::{
        ExtensionInstallationStatus,
        get_extension_status,
    };
    use fig_os_shim::FsProvider;
    use fig_util::directories::{
        bundled_gnome_extension_version_path,
        bundled_gnome_extension_zip_path,
    };
    use fig_util::system_info::linux::{
        DisplayServer,
        get_display_server,
    };
    use tracing::debug;

    let display_server = get_display_server(ctx)?;
    if display_server != DisplayServer::Wayland {
        debug!(
            "Detected non-Wayland display server: `{:?}`. Not installing the extension.",
            display_server
        );
        return Ok(());
    }

    if !state.get_bool_or("desktop.gnomeExtensionInstallationPermissionGranted", false) {
        debug!("Permission is not granted to install GNOME extension, doing nothing.");
        return Ok(());
    }

    let fs = ctx.fs();
    let extension_uuid = shell_extensions.extension_uuid().await?;
    let bundled_version: u32 = fs
        .read_to_string(bundled_gnome_extension_version_path(ctx, &extension_uuid)?)
        .await?
        .parse()?;
    let bundled_path = bundled_gnome_extension_zip_path(ctx, &extension_uuid)?;

    match get_extension_status(ctx, shell_extensions, Some(bundled_version)).await? {
        ExtensionInstallationStatus::GnomeShellNotRunning => {
            info!("GNOME Shell is not running, not installing the extension.");
        },
        ExtensionInstallationStatus::NotInstalled => {
            info!("Extension {} not installed, installing now.", extension_uuid);
            shell_extensions.install_bundled_extension(bundled_path).await?;
        },
        ExtensionInstallationStatus::Errored => {
            error!(
                "Extension {} is in an errored state. It must be manually uninstalled, and the current desktop session must be restarted.",
                extension_uuid
            );
        },
        ExtensionInstallationStatus::RequiresReboot => {
            info!(
                "Extension {} already installed but not loaded. User must reboot their machine.",
                extension_uuid
            );
        },
        ExtensionInstallationStatus::UnexpectedVersion { installed_version } => {
            info!(
                "Installed extension {} has version {} but the bundled extension has version {}. Installing now.",
                extension_uuid, installed_version, bundled_version
            );
            shell_extensions.install_bundled_extension(bundled_path).await?;
        },
        ExtensionInstallationStatus::NotEnabled => {
            info!(
                "Extension {} is installed but not enabled. Enabling now.",
                extension_uuid
            );
            match shell_extensions.enable_extension().await {
                Ok(true) => {
                    info!("Extension enabled.");
                },
                Ok(false) => {
                    error!("Something went wrong trying to enable the extension.");
                },
                Err(err) => {
                    error!("Error occurred enabling the extension: {:?}", err);
                },
            }
        },
        ExtensionInstallationStatus::Enabled => {
            info!("Extension {} is already installed and enabled.", extension_uuid);
        },
    }

    Ok(())
}

/// Installs the desktop entry if required.
#[cfg(target_os = "linux")]
async fn install_desktop_entry(ctx: &Context, state: &fig_settings::State) -> anyhow::Result<()> {
    use fig_integrations::desktop_entry::DesktopEntryIntegration;
    use fig_util::directories::{
        appimage_desktop_entry_icon_path,
        appimage_desktop_entry_path,
    };

    if !state.get_bool_or("appimage.manageDesktopEntry", false) {
        return Ok(());
    }

    let exec_path = ctx.env().get("APPIMAGE")?;
    let entry_path = appimage_desktop_entry_path(ctx)?;
    let icon_path = appimage_desktop_entry_icon_path(ctx)?;
    DesktopEntryIntegration::new(ctx, Some(entry_path), Some(icon_path), Some(exec_path.into()))
        .install()
        .await?;
    Ok(())
}

/// Installs the autostart entry if required.
#[cfg(target_os = "linux")]
async fn install_autostart_entry(
    ctx: &Context,
    settings: &fig_settings::Settings,
    state: &fig_settings::State,
) -> anyhow::Result<()> {
    use fig_integrations::desktop_entry::{
        AutostartIntegration,
        should_install_autostart_entry,
    };

    if !should_install_autostart_entry(ctx, settings, state) {
        return Ok(());
    }

    AutostartIntegration::new(ctx)?.install().await?;

    Ok(())
}

/// Installs the CLI and PTY under the user's local bin directory from the AppImage, if required.
#[cfg(target_os = "linux")]
async fn install_appimage_binaries(ctx: &Context) -> anyhow::Result<()> {
    use fig_util::consts::{
        CHAT_BINARY_NAME,
        CLI_BINARY_NAME,
        PTY_BINARY_NAME,
    };
    use fig_util::directories::home_local_bin_ctx;
    use tokio::process::Command;

    if !home_local_bin_ctx(ctx)?.exists() {
        ctx.fs().create_dir_all(home_local_bin_ctx(ctx)?).await?;
    }

    // Extract and install the CLI + PTY under home local bin, if required.
    for binary_name in &[CLI_BINARY_NAME, PTY_BINARY_NAME, CHAT_BINARY_NAME] {
        let local_binary_path = home_local_bin_ctx(ctx)?.join(binary_name);
        if local_binary_path.exists() {
            let output = Command::new(&local_binary_path).arg("--version").output().await.ok();

            let installed_version = output
                .as_ref()
                .and_then(|output| std::str::from_utf8(&output.stdout).ok())
                .map(parse_version);

            match installed_version {
                Some(installed_version) => {
                    let app_version = env!("CARGO_PKG_VERSION");
                    if installed_version != app_version {
                        info!(
                            "Installed version {} for binary {} is different than application version {}",
                            installed_version,
                            local_binary_path.to_string_lossy(),
                            app_version
                        );
                        copy_binary_from_appimage_mount(ctx, binary_name, local_binary_path).await?;
                    }
                },
                None => error!(
                    "Unable to parse the version of the binary at: {}",
                    local_binary_path.to_string_lossy()
                ),
            }
        } else {
            copy_binary_from_appimage_mount(ctx, binary_name, local_binary_path).await?;
        }
    }

    Ok(())
}

/// The AppImage is executed by mounting to a temporary directory and running the desktop binary.
/// The current working directory of the desktop app essentially looks like this:
/// - <tempdir>/bin/q
/// - <tempdir>/bin/qterm
///
/// Thus, we can access and copy the bundled binaries from the AppImage to the provided
/// `destination`.
#[cfg(target_os = "linux")]
async fn copy_binary_from_appimage_mount(
    ctx: &Context,
    binary_name: &str,
    destination: impl AsRef<std::path::Path>,
) -> anyhow::Result<()> {
    use anyhow::Context;
    use tracing::debug;

    let cwd = ctx.env().current_dir()?;
    let binary_path = cwd.join(format!("bin/{binary_name}"));
    debug!(
        "Copying {} to {}",
        binary_path.to_string_lossy(),
        destination.as_ref().to_string_lossy()
    );
    ctx.fs()
        .copy(&binary_path, destination)
        .await
        .context(format!("Unable to copy {binary_name}"))?;

    Ok(())
}

/// Parses the semver portion of a string of the form: `"<binary-name> <semver>"`.
#[cfg(target_os = "linux")]
fn parse_version(output: &str) -> String {
    output
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
enum SystemdUserService {
    IBusGeneric,
    IBusGnome,
}

#[cfg(target_os = "linux")]
impl SystemdUserService {
    fn service_name(&self) -> &'static str {
        match self {
            SystemdUserService::IBusGeneric => "org.freedesktop.IBus.session.generic.service",
            SystemdUserService::IBusGnome => "org.freedesktop.IBus.session.GNOME.service",
        }
    }
}

#[cfg(target_os = "linux")]
impl std::fmt::Display for SystemdUserService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.service_name())
    }
}

#[cfg(target_os = "linux")]
async fn launch_systemd_user_service(service: SystemdUserService) -> anyhow::Result<()> {
    use tokio::process::Command;
    let output = Command::new("systemctl")
        .args(["--user", "restart", service.service_name()])
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&output.stderr))
    }
    Ok(())
}

#[cfg(target_os = "linux")]
#[allow(dead_code)]
async fn launch_ibus(ctx: &Context) {
    use std::ffi::OsString;

    use sysinfo::{
        ProcessRefreshKind,
        RefreshKind,
        System,
    };
    use tokio::process::Command;

    let system = tokio::task::block_in_place(|| {
        System::new_with_specifics(RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing()))
    });
    let ibus_daemon = OsString::from("ibus-daemon");
    if system.processes_by_name(&ibus_daemon).next().is_none() {
        info!("Launching ibus via systemd");

        match Command::new("systemctl")
            .args(["--user", "is-active", "gnome-session-initialized.target"])
            .output()
            .await
        {
            Ok(gnome_session_output) => match std::str::from_utf8(&gnome_session_output.stdout).map(|s| s.trim()) {
                Ok("active") => match launch_systemd_user_service(SystemdUserService::IBusGnome).await {
                    Ok(_) => info!("Launched '{}", SystemdUserService::IBusGnome),
                    Err(err) => error!(%err, "Failed to launch '{}'", SystemdUserService::IBusGnome),
                },
                Ok("inactive") => match launch_systemd_user_service(SystemdUserService::IBusGeneric).await {
                    Ok(_) => info!("Launched '{}'", SystemdUserService::IBusGeneric),
                    Err(err) => error!(%err, "Failed to launch '{}'", SystemdUserService::IBusGeneric),
                },
                result => error!(
                    ?result,
                    "Failed to determine if gnome-session-initialized.target is running"
                ),
            },
            Err(err) => error!(%err, "Failed to run 'systemctl --user is-active gnome-session-initialized.target'"),
        }
    }

    // Wait up to 2 sec for ibus activation
    for _ in 0..10 {
        if dbus::ibus::connect_to_ibus_daemon(ctx).await.is_ok() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    error!("Timed out after 2 sec waiting for ibus activation");
}

#[cfg(target_os = "macos")]
fn should_run_install_script() -> bool {
    let current_version = current_version();
    let previous_version = match previous_version() {
        Some(previous_version) => previous_version,
        None => return true,
    };

    !is_cargo_debug_build() && current_version > previous_version
}

/// The current version of the desktop app
fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap()
}

/// The previous version of the desktop app stored in local state
#[cfg(target_os = "macos")]
fn previous_version() -> Option<Version> {
    fig_settings::state::get_string(PREVIOUS_VERSION_KEY)
        .ok()
        .flatten()
        .and_then(|ref v| Version::parse(v).ok())
}

fn set_previous_version(version: Version) -> anyhow::Result<()> {
    fig_settings::state::set_value(PREVIOUS_VERSION_KEY, version.to_string())?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_current_version() {
        current_version();
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_symlink() {
        use tempfile::tempdir;

        let tmp_dir = tempdir().unwrap();
        let tmp_dir = tmp_dir.path();

        // folders
        let src_dir_1 = tmp_dir.join("dir_1");
        let src_dir_2 = tmp_dir.join("dir_2");
        let dst_dir = tmp_dir.join("dst");

        std::fs::create_dir_all(&src_dir_1).unwrap();
        std::fs::create_dir_all(&src_dir_2).unwrap();

        // Check that a new symlink is created
        assert!(!dst_dir.exists());
        symlink(&src_dir_1, &dst_dir).await.unwrap();
        assert!(dst_dir.exists());
        assert_eq!(dst_dir.read_link().unwrap(), src_dir_1);

        // Check that the symlink is updated
        symlink(&src_dir_2, &dst_dir).await.unwrap();
        assert!(dst_dir.exists());
        assert_eq!(dst_dir.read_link().unwrap(), src_dir_2);

        // files
        let src_file_1 = src_dir_1.join("file_1");
        let src_file_2 = src_dir_2.join("file_2");
        let dst_file = dst_dir.join("file");

        std::fs::write(&src_file_1, "content 1").unwrap();
        std::fs::write(&src_file_2, "content 2").unwrap();

        // Check that a new symlink is created
        assert!(!dst_file.exists());
        symlink(&src_file_1, &dst_file).await.unwrap();
        assert!(dst_file.exists());
        assert_eq!(std::fs::read_to_string(&dst_file).unwrap(), "content 1");

        // Check that the symlink is updated
        symlink(&src_file_2, &dst_file).await.unwrap();
        assert!(dst_file.exists());
        assert_eq!(std::fs::read_to_string(&dst_file).unwrap(), "content 2");
    }

    #[cfg(target_os = "linux")]
    mod linux_appimage_tests {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;
        use std::path::Path;

        use fig_util::directories::home_local_bin_ctx;
        use fig_util::{
            CHAT_BINARY_NAME,
            CLI_BINARY_NAME,
            PTY_BINARY_NAME,
        };
        use tokio::process::Command;

        use super::*;

        /// Writes a test script for the CLI/PTY binaries to `directory` that prints
        /// `"<binaryname> version"`.
        async fn write_test_binaries(ctx: &Context, version: &str, destination: impl AsRef<Path>) {
            let fs = ctx.fs();
            if !fs.exists(&destination) {
                fs.create_dir_all(&destination).await.unwrap();
            }
            for binary_name in &[CLI_BINARY_NAME, PTY_BINARY_NAME, CHAT_BINARY_NAME] {
                let path = destination.as_ref().join(binary_name);
                fs.write(
                    &path,
                    format!(
                        r#"#!/usr/bin/env sh
echo "{binary_name} {version}"
            "#
                    ),
                )
                .await
                .unwrap();
                fs.set_permissions(&path, Permissions::from_mode(0o700)).await.unwrap();
            }
        }

        async fn assert_binaries_installed(ctx: &Context, expected_version: &str) {
            for binary_name in &[CLI_BINARY_NAME, PTY_BINARY_NAME, CHAT_BINARY_NAME] {
                let binary_path = home_local_bin_ctx(ctx).unwrap().join(binary_name);
                let stdout = Command::new(ctx.fs().chroot_path(binary_path))
                    .output()
                    .await
                    .unwrap()
                    .stdout;
                let stdout = std::str::from_utf8(&stdout).unwrap();
                assert!(ctx.fs().exists(home_local_bin_ctx(ctx).unwrap().join(binary_name)));
                assert_eq!(parse_version(stdout), expected_version);
            }
        }

        #[test]
        fn test_linux_parse_version() {
            assert_eq!(parse_version("cli 1.2.3"), "1.2.3");
        }

        static INSTALL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

        #[tokio::test]
        async fn test_linux_appimage_install_on_fresh_system() {
            let _lock = INSTALL_TEST_LOCK.lock().await;

            tracing_subscriber::fmt::try_init().ok();

            // Given
            let ctx = Context::builder().with_test_home().await.unwrap().build();
            let current_version = current_version().to_string();
            write_test_binaries(&ctx, &current_version, "/bin").await;

            // When
            install_appimage_binaries(&ctx).await.unwrap();

            // Then
            assert_binaries_installed(&ctx, &current_version).await;
        }

        #[tokio::test]
        async fn test_linux_appimage_install_when_installed_binaries_have_old_version() {
            let _lock = INSTALL_TEST_LOCK.lock().await;

            tracing_subscriber::fmt::try_init().ok();

            // Given
            let ctx = Context::builder().with_test_home().await.unwrap().build();
            let current_version = current_version().to_string();
            let old_version = "0.0.1";
            write_test_binaries(&ctx, &current_version, "/bin").await;
            write_test_binaries(&ctx, old_version, home_local_bin_ctx(&ctx).unwrap()).await;

            // When
            install_appimage_binaries(&ctx).await.unwrap();

            // Then
            assert_binaries_installed(&ctx, &current_version).await;
        }
    }

    #[cfg(target_os = "linux")]
    mod linux_gnome_shell_extension_tests {
        use dbus::gnome_shell::{
            ExtensionInstallationStatus,
            GNOME_SHELL_PROCESS_NAME,
            ShellExtensions,
            get_extension_status,
        };
        use fig_os_shim::Os;
        use fig_settings::State;
        use fig_util::directories::{
            bundled_gnome_extension_version_path,
            bundled_gnome_extension_zip_path,
        };

        use super::*;

        /// Helper function that writes test files to [bundled_gnome_extension_zip_path] and
        /// [bundled_extension_version_path].
        async fn write_extension_bundle(ctx: &Context, uuid: &str, version: u32) {
            let zip_path = bundled_gnome_extension_zip_path(ctx, uuid).unwrap();
            let version_path = bundled_gnome_extension_version_path(ctx, uuid).unwrap();
            ctx.fs().create_dir_all(zip_path.parent().unwrap()).await.unwrap();
            ctx.fs().write(&zip_path, version.to_string()).await.unwrap();
            ctx.fs().write(&version_path, version.to_string()).await.unwrap();
        }

        #[tokio::test]
        async fn test_extension_is_installed_for_new_user() {
            let ctx = Context::builder()
                .with_test_home()
                .await
                .unwrap()
                .with_env_var("APPIMAGE", "1")
                .with_env_var("XDG_SESSION_TYPE", "wayland")
                .with_os(Os::Linux)
                .with_running_processes(&[GNOME_SHELL_PROCESS_NAME])
                .build_fake();
            let shell_extensions = ShellExtensions::new_fake(Arc::downgrade(&ctx));
            let extension_version = 1;
            write_extension_bundle(
                &ctx,
                &shell_extensions.extension_uuid().await.unwrap(),
                extension_version,
            )
            .await;
            let state = State::from_slice(&[("desktop.gnomeExtensionInstallationPermissionGranted", true.into())]);

            // When
            install_gnome_shell_extension(&ctx, &shell_extensions, &state)
                .await
                .unwrap();

            // Then
            let status = get_extension_status(&ctx, &shell_extensions, Some(extension_version))
                .await
                .unwrap();
            assert!(matches!(status, ExtensionInstallationStatus::RequiresReboot));
        }

        #[tokio::test]
        async fn test_extension_not_installed_if_permission_not_granted() {
            let ctx = Context::builder()
                .with_test_home()
                .await
                .unwrap()
                .with_env_var("APPIMAGE", "1")
                .with_os(Os::Linux)
                .with_running_processes(&[GNOME_SHELL_PROCESS_NAME])
                .build_fake();
            let shell_extensions = ShellExtensions::new_fake(Arc::downgrade(&ctx));
            let extension_version = 1;
            write_extension_bundle(
                &ctx,
                &shell_extensions.extension_uuid().await.unwrap(),
                extension_version,
            )
            .await;
            let state = State::new_fake();

            // When
            install_gnome_shell_extension(&ctx, &shell_extensions, &state)
                .await
                .unwrap();

            // Then
            let status = get_extension_status(&ctx, &shell_extensions, Some(extension_version))
                .await
                .unwrap();
            assert!(matches!(status, ExtensionInstallationStatus::NotInstalled));
        }

        #[tokio::test]
        async fn test_extension_not_installed_if_not_wayland() {
            let ctx = Context::builder()
                .with_test_home()
                .await
                .unwrap()
                .with_env_var("APPIMAGE", "1")
                .with_os(Os::Linux)
                .with_running_processes(&[GNOME_SHELL_PROCESS_NAME])
                .build_fake();
            let shell_extensions = ShellExtensions::new_fake(Arc::downgrade(&ctx));
            let extension_version = 1;
            write_extension_bundle(
                &ctx,
                &shell_extensions.extension_uuid().await.unwrap(),
                extension_version,
            )
            .await;
            let state = State::from_slice(&[("desktop.gnomeExtensionInstallationPermissionGranted", true.into())]);

            // When
            install_gnome_shell_extension(&ctx, &shell_extensions, &state)
                .await
                .unwrap();

            // Then
            let status = get_extension_status(&ctx, &shell_extensions, Some(extension_version))
                .await
                .unwrap();
            assert!(matches!(status, ExtensionInstallationStatus::NotInstalled));
        }
    }

    #[cfg(target_os = "linux")]
    mod linux_desktop_entry_tests {
        use fig_integrations::desktop_entry::{
            AutostartIntegration,
            local_entry_path,
            local_icon_path,
        };
        use fig_settings::{
            Settings,
            State,
        };
        use fig_util::directories::{
            appimage_desktop_entry_icon_path,
            appimage_desktop_entry_path,
        };

        use super::*;

        #[tokio::test]
        async fn test_desktop_entry_is_installed() {
            let ctx = Context::builder()
                .with_test_home()
                .await
                .unwrap()
                .with_env_var("APPIMAGE", "/test.appimage")
                .build_fake();
            let fs = ctx.fs();
            let entry_path = appimage_desktop_entry_path(&ctx).unwrap();
            let icon_path = appimage_desktop_entry_icon_path(&ctx).unwrap();
            fs.create_dir_all(&entry_path.parent().unwrap()).await.unwrap();
            fs.write(&entry_path, "[Desktop Entry]\nExec=q-desktop").await.unwrap();
            fs.create_dir_all(icon_path.parent().unwrap()).await.unwrap();
            fs.write(&icon_path, "image").await.unwrap();
            let state = State::from_slice(&[("appimage.manageDesktopEntry", true.into())]);

            // When
            install_desktop_entry(&ctx, &state).await.unwrap();

            // Then
            assert!(fs.exists(local_entry_path(&ctx).unwrap()));
            assert!(fs.exists(local_icon_path(&ctx).unwrap()));
        }

        #[tokio::test]
        async fn test_desktop_entry_not_installed_if_not_managed() {
            let ctx = Context::builder()
                .with_test_home()
                .await
                .unwrap()
                .with_env_var("APPIMAGE", "/test.appimage")
                .build_fake();
            let fs = ctx.fs();
            let entry_path = appimage_desktop_entry_path(&ctx).unwrap();
            let icon_path = appimage_desktop_entry_icon_path(&ctx).unwrap();
            fs.create_dir_all(&entry_path.parent().unwrap()).await.unwrap();
            fs.write(&entry_path, "[Desktop Entry]\nExec=q-desktop").await.unwrap();
            fs.create_dir_all(icon_path.parent().unwrap()).await.unwrap();
            fs.write(&icon_path, "image").await.unwrap();
            let state = State::new_fake();

            // When
            install_desktop_entry(&ctx, &state).await.unwrap();

            // Then
            assert!(!fs.exists(local_entry_path(&ctx).unwrap()));
            assert!(!fs.exists(local_icon_path(&ctx).unwrap()));
        }

        #[tokio::test]
        async fn test_autostart_entry_installed_locally_for_appimage() {
            let ctx = Context::builder()
                .with_test_home()
                .await
                .unwrap()
                .with_env_var("APPIMAGE", "/test.appimage")
                .build_fake();
            let fs = ctx.fs();
            fs.create_dir_all(local_entry_path(&ctx).unwrap().parent().unwrap())
                .await
                .unwrap();
            fs.write(local_entry_path(&ctx).unwrap(), "[Desktop Entry]")
                .await
                .unwrap();

            // When
            install_autostart_entry(
                &ctx,
                &Settings::new_fake(),
                &State::from_slice(&[("appimage.manageDesktopEntry", true.into())]),
            )
            .await
            .unwrap();

            // Then
            assert!(
                AutostartIntegration::to_local(&ctx)
                    .unwrap()
                    .is_installed()
                    .await
                    .is_ok()
            );
        }
    }
}
