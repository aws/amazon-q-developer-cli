//! `kiro-cli bot` — thin clap shim that dispatches into the `kiro-bot` crate.
//!
//! All bot-specific logic (duration parsing, staleness marker, instance type
//! detection, install/run/service flows) lives in `kiro-bot::cli`. This file
//! intentionally only owns:
//!   1. the clap subcommand surface,
//!   2. the `KIRO_ENABLE_BOT` env-var feature gate,
//!   3. the dispatch match into `kiro-bot`,
//!   4. the one bridge into chat-cli's own `update::UpdateArgs` (since the binary updating itself
//!      is a chat-cli concern, not a kiro-bot one).

use std::process::ExitCode;

use clap::{
    Args,
    Subcommand,
};
use eyre::Result;
use kiro_bot::cli::update as bot_update;

use crate::os::Os;

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct BotArgs {
    #[command(subcommand)]
    pub command: BotCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum BotCommand {
    /// Install a bot instance from a config directory
    Install { path: String },
    /// Uninstall a bot instance
    Uninstall {
        name: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Start a bot instance (background daemon; use --foreground to attach)
    Start {
        name: Option<String>,
        /// Run attached to the terminal
        #[arg(long)]
        foreground: bool,
        /// Start all installed instances
        #[arg(long)]
        all: bool,
    },
    /// Stop a running bot instance
    Stop {
        name: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// List all instances and their status
    Status,
    /// Run a cron/headless instance once and exit
    Run { name: String },
    /// Show the monitoring dashboard
    Monitor,
    /// Check for and install binary updates
    Update(BotUpdateArgs),
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct BotUpdateArgs {
    /// Only update if last update was older than this duration (e.g. "7d", "24h", "1h30m")
    #[arg(long)]
    pub if_stale: Option<String>,
    /// Force update even if current version matches
    #[arg(long)]
    pub force: bool,
}

const KIRO_ENABLE_BOT: &str = "KIRO_ENABLE_BOT";

impl BotArgs {
    pub async fn execute(self, os: &mut Os) -> Result<ExitCode> {
        let enabled = std::env::var_os(KIRO_ENABLE_BOT).is_some_and(|v| !v.is_empty() && v != "0");
        if !enabled {
            return Ok(ExitCode::FAILURE);
        }

        match self.command {
            BotCommand::Install { path } => {
                kiro_bot::cli::install::cmd_install(&path).map_err(|e| eyre::eyre!(e))?;
                Ok(ExitCode::SUCCESS)
            },
            BotCommand::Uninstall { name, all } => {
                for n in resolve_names(name, all)? {
                    kiro_bot::cli::install::cmd_uninstall(&n).map_err(|e| eyre::eyre!(e))?;
                }
                Ok(ExitCode::SUCCESS)
            },
            BotCommand::Start { name, foreground, all } => {
                if foreground {
                    let n = name.ok_or_else(|| eyre::eyre!("--foreground requires a name"))?;
                    if bot_update::is_cron_instance(&n).map_err(|e| eyre::eyre!(e))? {
                        kiro_bot::cli::run::cmd_cron_daemon(&n)
                            .await
                            .map_err(|e| eyre::eyre!(e))?;
                    } else {
                        kiro_bot::cli::run::cmd_run(&n).await.map_err(|e| eyre::eyre!(e))?;
                    }
                } else {
                    for n in resolve_names(name, all)? {
                        kiro_bot::cli::service::cmd_start(&n).map_err(|e| eyre::eyre!(e))?;
                    }
                }
                Ok(ExitCode::SUCCESS)
            },
            BotCommand::Stop { name, all } => {
                for n in resolve_names(name, all)? {
                    kiro_bot::cli::service::cmd_stop(&n).map_err(|e| eyre::eyre!(e))?;
                }
                Ok(ExitCode::SUCCESS)
            },
            BotCommand::Status => {
                kiro_bot::cli::service::cmd_status().map_err(|e| eyre::eyre!(e))?;
                Ok(ExitCode::SUCCESS)
            },
            BotCommand::Run { name } => {
                kiro_bot::cli::run::cmd_cron(&name).await.map_err(|e| eyre::eyre!(e))?;
                Ok(ExitCode::SUCCESS)
            },
            BotCommand::Monitor => {
                kiro_bot::cli::service::cmd_monitor().map_err(|e| eyre::eyre!(e))?;
                Ok(ExitCode::SUCCESS)
            },
            BotCommand::Update(args) => {
                if let Some(ref duration_str) = args.if_stale {
                    let max_age = bot_update::parse_duration(duration_str).map_err(|e| eyre::eyre!(e))?;
                    if !bot_update::is_update_stale(max_age) && !args.force {
                        println!("Binary is fresh (within {duration_str}), skipping update.");
                        return Ok(ExitCode::SUCCESS);
                    }
                }

                let update_args = crate::cli::update::UpdateArgs {
                    check: false,
                    force: args.force,
                };
                let result = update_args.execute(os).await?;
                bot_update::touch_update_marker().map_err(|e| eyre::eyre!(e))?;
                Ok(result)
            },
        }
    }
}

fn resolve_names(name: Option<String>, all: bool) -> Result<Vec<String>> {
    if all {
        kiro_bot::config::all_instance_names().map_err(|e| eyre::eyre!(e))
    } else {
        Ok(vec![name.ok_or_else(|| eyre::eyre!("name or --all required"))?])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_var_name() {
        assert_eq!(KIRO_ENABLE_BOT, "KIRO_ENABLE_BOT");
    }

    #[test]
    fn bot_install_parsing() {
        use clap::Parser;
        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BotCommand,
        }
        let cli = TestCli::parse_from(["test", "install", "/path/to/config"]);
        assert_eq!(cli.cmd, BotCommand::Install {
            path: "/path/to/config".into()
        });
    }

    #[test]
    fn bot_start_foreground_parsing() {
        use clap::Parser;
        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BotCommand,
        }
        let cli = TestCli::parse_from(["test", "start", "my-bot", "--foreground"]);
        assert_eq!(cli.cmd, BotCommand::Start {
            name: Some("my-bot".into()),
            foreground: true,
            all: false,
        });
    }

    #[test]
    fn bot_update_if_stale_parsing() {
        use clap::Parser;
        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BotCommand,
        }
        let cli = TestCli::parse_from(["test", "update", "--if-stale", "7d"]);
        assert_eq!(
            cli.cmd,
            BotCommand::Update(BotUpdateArgs {
                if_stale: Some("7d".into()),
                force: false,
            })
        );
    }
}
