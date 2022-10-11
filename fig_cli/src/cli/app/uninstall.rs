#![allow(dead_code, unused_imports)]

use std::path::{
    Path,
    PathBuf,
};

use clap::Args;
use fig_telemetry::{
    TrackEvent,
    TrackEventType,
    TrackSource,
};
use fig_util::directories;
use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tracing::warn;

use crate::cli::installation::{
    uninstall_cli,
    InstallComponents,
};
use crate::daemon::IS_RUNNING_DAEMON;

async fn remove_in_dir_with_prefix_unless(dir: &Path, prefix: &str, unless: impl Fn(&str) -> bool) {
    if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(prefix) && !unless(name) {
                    tokio::fs::remove_file(entry.path()).await.ok();
                    tokio::fs::remove_dir_all(entry.path()).await.ok();
                }
            }
        }
    }
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UninstallArgs {
    /// Remove configuration and data files
    #[arg(long)]
    pub user_data: bool,
    /// Remove executable and
    #[arg(long)]
    pub app_bundle: bool,
    /// Remove input method
    #[arg(long)]
    pub input_method: bool,
    /// Remove terminal integrations (i.e. VSCode, iTerm2, etc.)
    #[arg(long)]
    pub terminal_integrations: bool,
    /// Remove Fig daemon
    #[arg(long)]
    pub daemon: bool,
    /// Remove dotfile shell integration
    #[arg(long)]
    pub dotfiles: bool,
    /// Remove SSH integration
    #[arg(long)]
    pub ssh: bool,
    /// Do not open the uninstallation page
    #[arg(long)]
    pub no_open: bool,
}

#[cfg(target_os = "macos")]
pub async fn uninstall_mac_app(uninstall_args: &UninstallArgs) {
    // Send uninstall telemetry event
    let tel_join = tokio::task::spawn(async move {
        fig_telemetry::emit_track(TrackEvent::new(
            TrackEventType::UninstalledApp,
            if *IS_RUNNING_DAEMON.lock() {
                TrackSource::Daemon
            } else {
                TrackSource::Cli
            },
            env!("CARGO_PKG_VERSION").into(),
            [("source", "fig app uninstall")],
        ))
        .await
        .ok();
    });

    if !uninstall_args.no_open {
        // Open the uninstallation page
        let email = fig_request::auth::get_email().unwrap_or_default();
        let version = fig_request::defaults::get_default("versionAtPreviousLaunch").unwrap_or_default();
        fig_util::open_url(format!("https://fig.io/uninstall?email={email}&version={version}",)).ok();
    }

    if uninstall_args.app_bundle {
        uninstall_app_bundle().await;
    }

    if uninstall_args.user_data {
        uninstall_user_data().await;
    }

    if uninstall_args.input_method {
        uninstall_input_method().await;
    }

    if uninstall_args.terminal_integrations {
        uninstall_terminal_integrations().await;
    }

    if uninstall_args.dotfiles {
        uninstall_dotfiles().await;
    }

    if uninstall_args.ssh {
        uninstall_ssh().await;
    }

    // Daemon must come last
    if uninstall_args.daemon {
        uninstall_daemon().await
    }

    tel_join.await.ok();
}

async fn uninstall_app_bundle() {
    let app_path = PathBuf::from("Applications").join("Fig.app");
    if app_path.exists() {
        tokio::fs::remove_dir_all(&app_path)
            .await
            .map_err(|err| warn!("Failed to remove Fig.app: {err}"))
            .ok();
    }

    // Remove launch agents
    if let Ok(home) = directories::home_dir() {
        let launch_agents = home.join("Library").join("LaunchAgents");
        remove_in_dir_with_prefix_unless(&launch_agents, "io.fig.", |p| p.contains("daemon")).await;
    } else {
        warn!("Could not find home directory");
    }

    if let Err(err) = uninstall_cli(InstallComponents::BINARY).await {
        warn!("Could not uninstall CLI: {err}");
    }
}

async fn uninstall_user_data() {
    // Delete Fig defaults on macOS
    #[cfg(target_os = "macos")]
    {
        tokio::process::Command::new("defaults")
            .args(["delete", "com.mschrage.fig.shared"])
            .output()
            .await
            .map_err(|err| warn!("Failed to delete defaults: {err}"))
            .ok();
    }

    // Delete data dir
    if let Ok(fig_data_dir) = directories::fig_data_dir() {
        tokio::fs::remove_dir_all(&fig_data_dir)
            .await
            .map_err(|err| warn!("Could not remove {}: {err}", fig_data_dir.display()))
            .ok();
    }

    // Delete the ~/.fig folder
    if let Ok(fig_dir) = directories::fig_dir() {
        tokio::fs::remove_dir_all(fig_dir)
            .await
            .map_err(|err| warn!("Could not remove ~/.fig folder: {err}"))
            .ok();
    } else {
        warn!("Could not find .fig folder");
    }
}

async fn uninstall_input_method() {
    if let Ok(home) = directories::home_dir() {
        // Remove the app
        let fig_input_method_app = home.join("Library").join("Input Methods").join("FigInputMethod.app");

        if fig_input_method_app.exists() {
            tokio::fs::remove_dir_all(fig_input_method_app)
                .await
                .map_err(|err| warn!("Could not remove FigInputMethod.app: {err}"))
                .ok();
        }
    } else {
        warn!("Could not find home directory");
    }
}

async fn uninstall_terminal_integrations() {
    // Delete integrations
    if let Ok(home) = directories::home_dir() {
        // Delete iTerm integration
        for path in &[
            "Library/Application Support/iTerm2/Scripts/AutoLaunch/fig-iterm-integration.py",
            ".config/iterm2/AppSupport/Scripts/AutoLaunch/fig-iterm-integration.py",
            "Library/Application Support/iTerm2/Scripts/AutoLaunch/fig-iterm-integration.scpt",
        ] {
            tokio::fs::remove_file(home.join(path))
                .await
                .map_err(|err| warn!("Could not remove iTerm integration {path}: {err}"))
                .ok();
        }

        // Delete VSCode integration
        for (folder, prefix) in &[
            (".vscode/extensions", "withfig.fig-"),
            (".vscode-insiders/extensions", "withfig.fig-"),
            (".vscode-oss/extensions", "withfig.fig-"),
        ] {
            let folder = home.join(folder);
            remove_in_dir_with_prefix_unless(&folder, prefix, |_| false).await;
        }

        // Remove Hyper integration
        let hyper_path = home.join(".hyper.js");
        if hyper_path.exists() {
            // Read the config file
            match tokio::fs::File::open(&hyper_path).await {
                Ok(mut file) => {
                    let mut contents = String::new();
                    match file.read_to_string(&mut contents).await {
                        Ok(_) => {
                            contents = contents.replace("\"fig-hyper-integration\",", "");
                            contents = contents.replace("\"fig-hyper-integration\"", "");

                            // Write the config file
                            match tokio::fs::File::create(&hyper_path).await {
                                Ok(mut file) => {
                                    file.write_all(contents.as_bytes())
                                        .await
                                        .map_err(|err| warn!("Could not write to Hyper config: {err}"))
                                        .ok();
                                },
                                Err(err) => {
                                    warn!("Could not create Hyper config: {err}")
                                },
                            }
                        },
                        Err(err) => {
                            warn!("Could not read Hyper config: {err}");
                        },
                    }
                },
                Err(err) => {
                    warn!("Could not open Hyper config: {err}");
                },
            }
        }

        // Remove Kitty integration
        let kitty_path = home.join(".config").join("kitty").join("kitty.conf");
        if kitty_path.exists() {
            // Read the config file
            match tokio::fs::File::open(&kitty_path).await {
                Ok(mut file) => {
                    let mut contents = String::new();
                    match file.read_to_string(&mut contents).await {
                        Ok(_) => {
                            contents = contents.replace("watcher ${HOME}/.fig/tools/kitty-integration.py", "");
                            // Write the config file
                            match tokio::fs::File::create(&kitty_path).await {
                                Ok(mut file) => {
                                    file.write_all(contents.as_bytes())
                                        .await
                                        .map_err(|err| warn!("Could not write to Kitty config: {err}"))
                                        .ok();
                                },
                                Err(err) => {
                                    warn!("Could not create Kitty config: {err}")
                                },
                            }
                        },
                        Err(err) => {
                            warn!("Could not read Kitty config: {err}");
                        },
                    }
                },
                Err(err) => {
                    warn!("Could not open Kitty config: {err}");
                },
            }
        }
        // TODO: Add Jetbrains integration
    }
}

async fn uninstall_daemon() {
    uninstall_cli(InstallComponents::DAEMON)
        .await
        .map_err(|err| warn!("Could not uninstall daemon: {err}"))
        .ok();
}

async fn uninstall_dotfiles() {
    uninstall_cli(InstallComponents::DOTFILES)
        .await
        .map_err(|err| warn!("Could not uninstall dotfiles: {err}"))
        .ok();
}

async fn uninstall_ssh() {
    uninstall_cli(InstallComponents::SSH)
        .await
        .map_err(|err| warn!("Could not uninstall SSH: {err}"))
        .ok();
}
