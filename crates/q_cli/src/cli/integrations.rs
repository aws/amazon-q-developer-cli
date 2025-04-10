use std::process::ExitCode;

use anstream::println;
use clap::Subcommand;
use crossterm::style::Stylize;
use eyre::Result;
use fig_integrations::Integration as _;
use fig_integrations::shell::ShellExt;
use fig_integrations::ssh::SshIntegration;
use fig_os_shim::Env;
use fig_util::Shell;
use serde_json::json;
use tracing::debug;

use super::OutputFormat;

#[derive(Debug, PartialEq, Eq, Subcommand)]
pub enum IntegrationsSubcommands {
    Install {
        /// Integration to install
        #[command(subcommand)]
        integration: Integration,
        /// Suppress status messages
        #[arg(long, short)]
        silent: bool,
    },
    Uninstall {
        /// Integration to uninstall
        #[command(subcommand)]
        integration: Integration,
        /// Suppress status messages
        #[arg(long, short)]
        silent: bool,
    },
    Reinstall {
        /// Integration to reinstall
        #[command(subcommand)]
        integration: Integration,
        /// Suppress status messages
        #[arg(long, short)]
        silent: bool,
    },
    Status {
        /// Integration to check status of
        #[command(subcommand)]
        integration: Integration,
        #[arg(long, short, value_enum, default_value_t)]
        format: OutputFormat,
    },
}

#[derive(Debug, Subcommand, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Integration {
    Dotfiles {
        /// Only install the integrations for a single shell
        #[arg(value_enum)]
        shell: Option<Shell>,
    },
    Ssh,
    InputMethod,
    #[command(name = "vscode")]
    VSCode,
    #[command(alias = "jetbrains-plugin")]
    IntellijPlugin,
    AutostartEntry,
    GnomeShellExtension,
    #[doc(hidden)]
    All,
}

impl IntegrationsSubcommands {
    pub async fn execute(self) -> Result<ExitCode> {
        match self {
            IntegrationsSubcommands::Install { integration, silent } => {
                if let Integration::All = integration {
                    install(Integration::Dotfiles { shell: None }, silent).await?;
                    install(Integration::Ssh, silent).await?;
                    #[cfg(target_os = "macos")]
                    install(Integration::InputMethod, silent).await?;
                } else {
                    install(integration, silent).await?;
                }
                Ok(ExitCode::SUCCESS)
            },
            IntegrationsSubcommands::Uninstall { integration, silent } => {
                if let Integration::All = integration {
                    uninstall(Integration::Dotfiles { shell: None }, silent).await?;
                    uninstall(Integration::Ssh, silent).await?;
                    #[cfg(target_os = "macos")]
                    uninstall(Integration::InputMethod, silent).await?;
                    #[cfg(target_os = "linux")]
                    uninstall(Integration::AutostartEntry, silent).await?;
                    #[cfg(target_os = "linux")]
                    uninstall(Integration::GnomeShellExtension, silent).await?;
                } else {
                    uninstall(integration, silent).await?;
                }
                Ok(ExitCode::SUCCESS)
            },
            IntegrationsSubcommands::Status { integration, format } => status(integration, format).await,
            IntegrationsSubcommands::Reinstall { integration, silent } => {
                if let Integration::All = integration {
                    uninstall(Integration::Dotfiles { shell: None }, silent).await?;
                    uninstall(Integration::Ssh, silent).await?;
                    #[cfg(target_os = "macos")]
                    uninstall(Integration::InputMethod, silent).await?;
                    install(Integration::Dotfiles { shell: None }, silent).await?;
                    install(Integration::Ssh, silent).await?;
                    #[cfg(target_os = "macos")]
                    install(Integration::InputMethod, silent).await?;
                } else {
                    uninstall(integration, silent).await?;
                    install(integration, silent).await?;
                }
                Ok(ExitCode::SUCCESS)
            },
        }
    }
}

#[allow(unused_mut)]
async fn install(integration: Integration, silent: bool) -> Result<()> {
    let mut installed = false;
    let mut errored = false;
    let mut status: Option<&str> = None;

    let result = match integration {
        Integration::All => Ok(()),
        Integration::Dotfiles { shell } => {
            let shells = match shell {
                Some(shell) => vec![shell],
                None => vec![Shell::Bash, Shell::Zsh, Shell::Fish],
            };

            let mut errs: Vec<String> = vec![];
            for shell in &shells {
                match shell.get_shell_integrations(&Env::new()) {
                    Ok(integrations) => {
                        for integration in integrations {
                            match integration.is_installed().await {
                                Ok(_) => {
                                    debug!("Skipping {}", integration.describe());
                                },
                                Err(_) => {
                                    installed = true;
                                    if let Err(e) = integration.install().await {
                                        errs.push(format!(
                                            "{}: {}",
                                            integration.describe().bold(),
                                            e.verbose_message()
                                        ));
                                    }
                                },
                            }
                        }
                    },
                    Err(e) => {
                        errs.push(format!("{shell}: {e}"));
                    },
                }
            }

            if errs.is_empty() {
                Ok(())
            } else {
                Err(eyre::eyre!("\n\n{}", errs.join("\n\n")))
            }
        },
        Integration::Ssh => {
            let ssh_integration = SshIntegration::new()?;
            if ssh_integration.is_installed().await.is_err() {
                installed = true;
                ssh_integration.install().await.map_err(eyre::Report::from)
            } else {
                Ok(())
            }
        },
        Integration::InputMethod => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    fig_settings::state::set_value("input-method.enabled", true).ok();
                    fig_integrations::input_method::InputMethod::default().install().await?;
                    installed = true;
                    status = Some("You must restart your terminal to finish installing the input method.");
                    Ok(())
                } else {
                    errored = true;
                    Err(eyre::eyre!("Input method integration is only supported on macOS"))
                }
            }
        },
        Integration::VSCode => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    let variants = fig_integrations::vscode::variants_installed();
                    installed = !variants.is_empty();
                    for variant in variants {
                        fig_integrations::vscode::VSCodeIntegration { variant }.install().await?;
                    }
                    Ok(())
                } else {
                    errored = true;
                    Err(eyre::eyre!("VSCode integration is only supported on macOS"))
                }
            }
        },
        Integration::IntellijPlugin => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    let variants = fig_integrations::intellij::variants_installed().await?;
                    installed = !variants.is_empty();
                    for variant in variants {
                        variant.install().await?;
                    }
                    Ok(())
                } else {
                    errored = true;
                    Err(eyre::eyre!("IntelliJ integration is only supported on macOS"))
                }
            }
        },
        Integration::AutostartEntry => {
            errored = true;
            Err(eyre::eyre!(
                "Installing the autostart entry from the CLI is not supported"
            ))
        },
        Integration::GnomeShellExtension => {
            errored = true;
            Err(eyre::eyre!(
                "Installing the GNOME Shell extension from the CLI is not supported"
            ))
        },
    };

    if installed && result.is_ok() && !silent {
        println!("Installed!");

        if let Some(status) = status {
            println!("{status}");
        }
    }

    if !errored && !installed && !silent {
        println!("Already installed");
    }

    result
}

async fn uninstall(integration: Integration, silent: bool) -> Result<()> {
    let mut uninstalled = false;

    let result = match integration {
        Integration::All => Ok(()),
        Integration::Dotfiles { shell } => {
            let shells = match shell {
                Some(shell) => vec![shell],
                None => vec![Shell::Bash, Shell::Zsh, Shell::Fish],
            };

            let mut errs: Vec<String> = vec![];
            for shell in &shells {
                match shell.get_shell_integrations(&Env::new()) {
                    Ok(integrations) => {
                        for integration in integrations {
                            match integration.is_installed().await {
                                Ok(_) => {
                                    uninstalled = true;
                                    if let Err(e) = integration.uninstall().await {
                                        errs.push(format!(
                                            "{}: {}",
                                            integration.describe().bold(),
                                            e.verbose_message()
                                        ));
                                    }
                                },
                                Err(_) => {
                                    debug!("Skipping {}", integration.describe());
                                },
                            }
                        }
                    },
                    Err(e) => {
                        errs.push(format!("{shell}: {e}"));
                    },
                }
            }

            if errs.is_empty() {
                Ok(())
            } else {
                Err(eyre::eyre!("\n\n{}", errs.join("\n\n")))
            }
        },
        Integration::Ssh => {
            let ssh_integration = SshIntegration::new()?;
            if ssh_integration.is_installed().await.is_ok() {
                uninstalled = true;
                ssh_integration.uninstall().await.map_err(eyre::Report::from)
            } else {
                Ok(())
            }
        },
        Integration::InputMethod => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    fig_integrations::input_method::InputMethod::default().uninstall().await?;
                    uninstalled = true;
                    Ok(())
                } else {
                    Err(eyre::eyre!("Input method integration is only supported on macOS"))
                }
            }
        },
        Integration::VSCode => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    for variant in fig_integrations::vscode::variants_installed() {
                        let integration = fig_integrations::vscode::VSCodeIntegration { variant };
                        if integration.is_installed().await.is_ok() {
                            integration.uninstall().await?;
                            uninstalled = true;
                        }
                    }
                    println!("Warning: VSCode integrations are automatically reinstalled on launch");
                    Ok(())
                } else {
                    Err(eyre::eyre!("VSCode integration is only supported on macOS"))
                }
            }
        },
        Integration::IntellijPlugin => {
            cfg_if::cfg_if! {
                if #[cfg(any(target_os = "macos", target_os = "linux"))] {
                    for variant in fig_integrations::intellij::variants_installed().await? {
                        if variant.is_installed().await.is_ok() {
                            variant.uninstall().await?;
                            uninstalled = true;
                        }
                    }
                    println!("Warning: IntelliJ integrations are automatically reinstalled on launch");
                    Ok(())
                } else {
                    Err(eyre::eyre!("IntelliJ integration is only supported on macOS and Linux"))
                }
            }
        },
        Integration::AutostartEntry => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "linux")] {
                    use fig_integrations::desktop_entry::AutostartIntegration;
                    use fig_os_shim::Context;
                    AutostartIntegration::uninstall(&Context::new()).await?;
                    uninstalled = true;
                    Ok(())
                } else {
                    Err(eyre::eyre!("The autostart integration is only supported on Linux"))
                }
            }
        },
        Integration::GnomeShellExtension => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "linux")] {
                    use std::sync::Arc;
                    use dbus::gnome_shell::ShellExtensions;
                    use fig_integrations::gnome_extension::GnomeExtensionIntegration;
                    use fig_os_shim::Context;
                    let ctx = Context::new();
                    let shell_extensions = ShellExtensions::new(Arc::downgrade(&ctx));
                    uninstalled = GnomeExtensionIntegration::new(&ctx, &shell_extensions, None::<&str>, None).uninstall_manually().await?;
                    Ok(())
                } else {
                    Err(eyre::eyre!("The GNOME Shell extension is only supported on Linux"))
                }
            }
        },
    };

    if uninstalled && result.is_ok() && !silent {
        println!("Uninstalled!");
    }

    if !uninstalled && !silent {
        println!("Not installed");
    }

    result
}

async fn status(integration: Integration, format: OutputFormat) -> Result<ExitCode> {
    match integration {
        Integration::All => Err(eyre::eyre!(
            "Checking the status for all integrations is currently not supported"
        )),
        Integration::Ssh => {
            let ssh_integration = SshIntegration::new()?;
            let installed = ssh_integration.is_installed().await.is_ok();
            format.print(
                || if installed { "Installed" } else { "Not installed" },
                || {
                    json!({
                        "installed": installed,
                    })
                },
            );
            Ok(ExitCode::SUCCESS)
        },
        Integration::Dotfiles { .. } => {
            let mut all_integrations = vec![];
            let mut errors = vec![];

            for shell in &[Shell::Bash, Shell::Zsh, Shell::Fish] {
                match shell.get_shell_integrations(&Env::new()) {
                    Ok(integrations) => {
                        for integration in integrations {
                            all_integrations.push((
                                integration.is_installed().await.is_ok(),
                                integration.describe(),
                                integration.get_shell(),
                                integration.file_name().to_owned(),
                            ));
                        }
                    },
                    Err(e) => {
                        errors.push((shell.to_string(), e.verbose_message()));
                    },
                }
            }

            format.print(
                || {
                    let mut s = String::new();
                    for (installed, describe, _, _) in &all_integrations {
                        s.push_str(&if *installed {
                            "✔ ".green().to_string()
                        } else {
                            "✘ ".red().to_string()
                        });
                        s.push_str(describe);
                        s.push('\n');
                    }

                    for (shell, error) in &errors {
                        s.push_str(&format!("{shell}: {error}\n"));
                    }

                    s
                },
                || {
                    let integrations = all_integrations
                        .iter()
                        .map(|(installed, describe, shell, file_name)| {
                            json!({
                                "installed": installed,
                                "description": describe,
                                "shell": shell,
                                "file_name": file_name,
                            })
                        })
                        .collect::<Vec<_>>();

                    let errors = errors
                        .iter()
                        .map(|(shell, error)| {
                            json!({
                                "shell": shell,
                                "error": error,
                            })
                        })
                        .collect::<Vec<_>>();

                    json!({
                        "integrations": integrations,
                        "errors": errors,
                    })
                },
            );

            Ok(ExitCode::SUCCESS)
        },
        Integration::InputMethod => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    let input_method = fig_integrations::input_method::InputMethod::default();
                    let installed = input_method.is_installed().await.is_ok();
                    format.print(
                        || if installed { "Installed" } else { "Not installed" },
                        || json!({
                            "installed": installed,
                        })
                    );
                    Ok(ExitCode::SUCCESS)
                } else {
                    Err(eyre::eyre!("Input method integration is only supported on macOS"))
                }
            }
        },
        Integration::VSCode => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    let variants = fig_integrations::vscode::variants_installed();
                    for variant in variants {
                        let integration = fig_integrations::vscode::VSCodeIntegration { variant };
                        match integration.is_installed().await {
                            Ok(_) => {
                                println!("{}: Installed", integration.variant.application_name);
                            }
                            Err(_) => {
                                println!("{}: Not installed", integration.variant.application_name);
                            }
                        }
                    }
                    Ok(ExitCode::SUCCESS)
                } else {
                    Err(eyre::eyre!("VSCode integration is only supported on macOS"))
                }
            }
        },
        Integration::IntellijPlugin => {
            cfg_if::cfg_if! {
                if #[cfg(any(target_os = "macos", target_os = "linux"))] {
                    let variants = fig_integrations::intellij::variants_installed().await?;
                    for variant in variants {
                        match variant.is_installed().await {
                            Ok(_) => {
                                println!("{}: Installed", variant.variant.application_name());
                            }
                            Err(_) => {
                                println!("{}: Not installed", variant.variant.application_name());
                            }
                        }
                    }
                    Ok(ExitCode::SUCCESS)
                } else {
                    Err(eyre::eyre!("IntelliJ integration is only supported on macOS and Linux"))
                }
            }
        },
        Integration::AutostartEntry => Err(eyre::eyre!(
            "Checking the status of the autostart entry from the CLI is not supported"
        )),
        Integration::GnomeShellExtension => Err(eyre::eyre!(
            "Checking the status of the GNOME Shell extension from the CLI is not supported"
        )),
    }
}
