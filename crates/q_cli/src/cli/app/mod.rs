use std::process::ExitCode;
use std::time::Duration;

use anstream::println;
use clap::{
    Args,
    Subcommand,
    arg,
};
use crossterm::style::Stylize;
use eyre::{
    Result,
    bail,
};
use fig_install::InstallComponents;
use fig_ipc::local::update_command;
use fig_settings::{
    settings,
    state,
};
use fig_util::{
    CLI_BINARY_NAME,
    PRODUCT_NAME,
    manifest,
};
use tracing::{
    info,
    trace,
};

use crate::util::desktop::{
    LaunchArgs,
    desktop_app_running,
    launch_fig_desktop,
};

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UninstallArgs {
    /// Remove executable and user data
    #[arg(long)]
    pub app_bundle: bool,
    /// Remove input method
    #[arg(long)]
    pub input_method: bool,
    /// Remove dotfile shell integration
    #[arg(long)]
    pub dotfiles: bool,
    /// Remove SSH integration
    #[arg(long)]
    pub ssh: bool,
    /// Do not open the uninstallation page
    #[arg(long)]
    pub no_open: bool,
    /// Only open the uninstallation page
    #[arg(long)]
    pub only_open: bool,
}

#[derive(Debug, PartialEq, Eq, Subcommand)]
pub enum AppSubcommand {
    /// Run the tutorial again
    Onboarding,
    /// Check if the desktop app is running
    Running,
    /// Launch the desktop app
    Launch,
    /// Restart the desktop app
    Restart,
    /// Quit the desktop app
    Quit,
    /// Uninstall the desktop app
    Uninstall(UninstallArgs),
    /// Prompts shown on terminal startup
    Prompts,
}

impl From<&UninstallArgs> for InstallComponents {
    fn from(args: &UninstallArgs) -> Self {
        if args.input_method || args.dotfiles || args.ssh || args.app_bundle {
            let mut flags = InstallComponents::empty();
            flags.set(InstallComponents::INPUT_METHOD, args.input_method);
            flags.set(InstallComponents::SHELL_INTEGRATIONS, args.dotfiles);
            flags.set(InstallComponents::SSH, args.ssh);
            flags.set(InstallComponents::DESKTOP_APP, args.app_bundle);
            flags
        } else {
            InstallComponents::all()
        }
    }
}

pub async fn restart_fig() -> Result<()> {
    if fig_util::system_info::in_cloudshell() {
        bail!("Restarting {PRODUCT_NAME} is not supported in CloudShell");
    }

    if fig_util::system_info::is_remote() {
        bail!("Please restart {PRODUCT_NAME} from your host machine");
    }

    if !desktop_app_running() {
        launch_fig_desktop(LaunchArgs {
            wait_for_socket: true,
            open_dashboard: false,
            immediate_update: true,
            verbose: true,
        })?;

        Ok(())
    } else {
        println!("Restarting {PRODUCT_NAME}");
        crate::util::quit_fig(false).await?;
        tokio::time::sleep(Duration::from_millis(1000)).await;
        launch_fig_desktop(LaunchArgs {
            wait_for_socket: true,
            open_dashboard: false,
            immediate_update: true,
            verbose: false,
        })?;

        Ok(())
    }
}

impl AppSubcommand {
    pub async fn execute(&self) -> Result<ExitCode> {
        if !cfg!(target_os = "macos") {
            bail!("app subcommands are only supported on macOS");
        }

        match self {
            AppSubcommand::Onboarding => {
                launch_fig_desktop(LaunchArgs {
                    wait_for_socket: true,
                    open_dashboard: false,
                    immediate_update: true,
                    verbose: true,
                })?;

                if state::set_value("user.onboarding", true).is_ok()
                    && state::set_value("doctor.prompt-restart-terminal", false).is_ok()
                {
                    println!(
                        "
   ███████╗██╗ ██████╗
   ██╔════╝██║██╔════╝
   █████╗  ██║██║  ███╗
   ██╔══╝  ██║██║   ██║
   ██║     ██║╚██████╔╝
   ╚═╝     ╚═╝ ╚═════╝  ....is now installed!

   Start typing to use {}

   * Change settings? Run {}
   * {PRODUCT_NAME} not working? Run {}
                                ",
                        format!("{PRODUCT_NAME} Autocomplete").bold(),
                        CLI_BINARY_NAME.bold().magenta(),
                        format!("{CLI_BINARY_NAME} doctor").bold().magenta(),
                    );
                }
            },
            AppSubcommand::Prompts => {
                if fig_util::manifest::is_minimal() {
                } else if desktop_app_running() {
                    let new_version = state::get_string("NEW_VERSION_AVAILABLE").ok().flatten();
                    if let Some(version) = new_version {
                        info!("New version {} is available", version);
                        let autoupdates = !settings::get_bool_or("app.disableAutoupdates", false);

                        if autoupdates {
                            trace!("starting autoupdate");

                            println!("Updating {} to latest version...", PRODUCT_NAME.magenta());
                            let already_seen_hint = state::get_bool_or("DISPLAYED_AUTOUPDATE_SETTINGS_HINT", false);

                            if !already_seen_hint {
                                println!(
                                    "(To turn off automatic updates, run {})",
                                    "fig settings app.disableAutoupdates true".magenta()
                                );
                                state::set_value("DISPLAYED_AUTOUPDATE_SETTINGS_HINT", true)?;
                            }

                            // trigger forced update. This will QUIT the macOS app, it must be relaunched...
                            trace!("sending update commands to macOS app");
                            update_command(true).await?;

                            // Sleep for a bit
                            tokio::time::sleep(std::time::Duration::from_millis(3000)).await;

                            trace!("launching updated version");
                            launch_fig_desktop(LaunchArgs {
                                wait_for_socket: true,
                                open_dashboard: false,
                                immediate_update: true,
                                verbose: false,
                            })
                            .ok();
                        } else {
                            trace!("autoupdates are disabled.");

                            println!("A new version of {PRODUCT_NAME} is available. (Autoupdates are disabled)");
                            println!("To update, run: {}", "fig update".magenta());
                        }
                    }
                } else {
                    let no_autolaunch = settings::get_bool_or("app.disableAutolaunch", false) || manifest::is_minimal();
                    let user_quit_app = state::get_bool_or("APP_TERMINATED_BY_USER", false);
                    if !no_autolaunch && !user_quit_app && !fig_util::system_info::in_ssh() {
                        let already_seen_hint: bool =
                            fig_settings::state::get_bool_or("DISPLAYED_AUTOLAUNCH_SETTINGS_HINT", false);
                        println!("Launching {}...", PRODUCT_NAME.magenta());
                        if !already_seen_hint {
                            println!(
                                "(To turn off autolaunch, run {})",
                                "fig settings app.disableAutolaunch true".magenta()
                            );
                            fig_settings::state::set_value("DISPLAYED_AUTOLAUNCH_SETTINGS_HINT", true)?;
                        }

                        launch_fig_desktop(LaunchArgs {
                            wait_for_socket: false,
                            open_dashboard: false,
                            immediate_update: true,
                            verbose: false,
                        })?;
                    }
                }
            },
            #[cfg(target_os = "macos")]
            AppSubcommand::Uninstall(args) => {
                use fig_install::UNINSTALL_URL;

                if !args.no_open && !crate::util::is_brew_reinstall().await {
                    if let Err(err) = fig_util::open_url_async(UNINSTALL_URL).await {
                        tracing::error!(%err, %UNINSTALL_URL, "Failed to open uninstall url");
                    }
                }

                if !args.only_open {
                    fig_install::uninstall(args.into(), fig_os_shim::Context::new()).await?;
                }
            },
            #[cfg(not(target_os = "macos"))]
            AppSubcommand::Uninstall(_) => {},
            AppSubcommand::Restart => restart_fig().await?,
            AppSubcommand::Quit => {
                crate::util::quit_fig(true).await?;
            },
            AppSubcommand::Launch => {
                if desktop_app_running() {
                    println!("{PRODUCT_NAME} app is already running!");
                    return Ok(ExitCode::FAILURE);
                }

                launch_fig_desktop(LaunchArgs {
                    wait_for_socket: true,
                    open_dashboard: false,
                    immediate_update: true,
                    verbose: true,
                })?;
            },
            AppSubcommand::Running => {
                println!("{}", if desktop_app_running() { "1" } else { "0" });
            },
        }
        Ok(ExitCode::SUCCESS)
    }
}
