//! Installation, uninstallation, and update of the CLI.

use std::process::ExitCode;

use anstream::{
    eprintln,
    println,
};
use crossterm::style::Stylize;
use eyre::{
    Result,
    bail,
};
use fig_install::{
    InstallComponents,
    install,
};
use fig_os_shim::Env;
use fig_util::system_info::in_cloudshell;
use fig_util::{
    CLI_BINARY_NAME,
    PRODUCT_NAME,
};
use tracing::warn;

use super::user::{
    LoginArgs,
    login_interactive,
};
use crate::util::choose;

#[cfg_attr(windows, allow(unused_variables))]
pub async fn install_cli(
    install_components: InstallComponents,
    no_confirm: bool,
    force: bool,
    global: bool,
) -> Result<ExitCode> {
    let env = Env::new();

    #[cfg(unix)]
    {
        use nix::unistd::geteuid;
        if geteuid().is_root() && !global {
            eprintln!("{}", "Installing as root is not supported.".red().bold());
            if !force {
                eprintln!(
                    "{}",
                    "If you know what you're doing, run the command again with --force.".red()
                );
                return Ok(ExitCode::FAILURE);
            }
        }
    }

    if install_components.contains(InstallComponents::SHELL_INTEGRATIONS) && !global {
        let mut manual_install = if no_confirm {
            false
        } else {
            if !dialoguer::console::user_attended() {
                eyre::bail!("You must run with --no-confirm if unattended");
            }

            match choose(
                format!(
                    "Do you want {CLI_BINARY_NAME} to modify your shell config (you will have to manually do this otherwise)?",
                ),
                &["Yes", "No"],
            )? {
                Some(1) => true,
                Some(_) => false,
                None => bail!("No option selected"),
            }
        };
        if !manual_install {
            if let Err(err) = install(InstallComponents::SHELL_INTEGRATIONS, &env).await {
                println!("{}", "Could not automatically install:".bold());
                println!("{err}");
                manual_install = true;
            }
        }
        if !no_confirm && manual_install {
            let shell_dir = fig_util::directories::fig_data_dir_utf8()?.join("shell");
            let shell_dir = shell_dir
                .strip_prefix(fig_util::directories::home_dir()?)
                .unwrap_or(&shell_dir);

            println!();
            println!("To install the integrations manually, you will have to add the following to your rc files");
            println!("This step is required for the application to function properly");
            println!();
            println!("At the top of your .bashrc or .zshrc file:");
            println!("bash:    . \"$HOME/{shell_dir}/bashrc.pre.bash\"");
            println!("zsh:     . \"$HOME/{shell_dir}/zshrc.pre.zsh\"");
            println!();
            println!("At the bottom of your .bashrc or .zshrc file:");
            println!("bash:    . \"$HOME/{shell_dir}/bashrc.post.bash\"");
            println!("zsh:     . \"$HOME/{shell_dir}/zshrc.post.zsh\"");
            println!();

            if let Err(err) = install(InstallComponents::SHELL_INTEGRATIONS, &env).await {
                println!("Could not install required files:");
                println!("{err}");
            }
        }
    }

    if install_components.contains(InstallComponents::SHELL_INTEGRATIONS) && global {
        // TODO: fix this, we do not error out at the moment as the fix is in `install.sh` which works
        // for the CloudShell team
        warn!("Global install is not supported for shell integrations");
    }

    if install_components.contains(InstallComponents::INPUT_METHOD) && !no_confirm && !global {
        cfg_if::cfg_if! {
            if #[cfg(target_os = "macos")] {
                if !dialoguer::console::user_attended() {
                    eyre::bail!("You must run with --no-confirm if unattended");
                }

                println!();
                println!("To enable support for some terminals like Kitty, Alacritty, and Wezterm,");
                println!("you must enable our Input Method integration.");
                println!();
                println!("To enable the integration, select \"yes\" below and then click Ok in the popup.");
                println!();

                match choose("Do you want to enable support for input method backed terminals?", &["Yes", "No"])? {
                    Some(0) => {
                        install(InstallComponents::INPUT_METHOD, &env).await?;
                    }
                    Some(_) => {}
                    None => bail!("No option selected"),
                }
            }
        }
    }

    if !fig_auth::is_logged_in().await && !in_cloudshell() && !global {
        if !no_confirm {
            if !dialoguer::console::user_attended() {
                eyre::bail!("You must run with --no-confirm if unattended");
            }

            login_interactive(LoginArgs {
                license: None,
                identity_provider: None,
                region: None,
            })
            .await?;
        } else {
            println!();
            println!("You must login before you can use {PRODUCT_NAME}'s features.");
            println!("To login run: {}", format!("{CLI_BINARY_NAME} login").bold());
            println!();
        }
    }

    Ok(ExitCode::SUCCESS)
}
