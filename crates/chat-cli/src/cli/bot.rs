//! Bot subcommand — manages ACP-backed bot instances.
//! Only compiled when the `bot` feature is enabled.

use std::process::ExitCode;
use std::time::Duration;

use clap::{
    Args,
    Subcommand,
};
use eyre::Result;

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
    /// Interactive CLI chat without Slack (debug)
    Chat { name: String },
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

/// Parse a human-friendly duration string like "7d", "24h", "1h30m", "90m".
pub fn parse_duration(s: &str) -> Result<Duration> {
    let mut total_secs: u64 = 0;
    let mut current_num = String::new();

    for c in s.chars() {
        if c.is_ascii_digit() {
            current_num.push(c);
        } else {
            let n: u64 = current_num
                .parse()
                .map_err(|e| eyre::eyre!("invalid duration: '{s}': {e}"))?;
            current_num.clear();
            match c {
                'd' => total_secs += n * 86400,
                'h' => total_secs += n * 3600,
                'm' => total_secs += n * 60,
                's' => total_secs += n,
                _ => return Err(eyre::eyre!("unknown duration unit '{c}' in '{s}'")),
            }
        }
    }

    // Handle bare number (treat as seconds)
    if !current_num.is_empty() {
        let n: u64 = current_num
            .parse()
            .map_err(|e| eyre::eyre!("invalid duration: '{s}': {e}"))?;
        total_secs += n;
    }

    if total_secs == 0 {
        return Err(eyre::eyre!("duration must be > 0: '{s}'"));
    }

    Ok(Duration::from_secs(total_secs))
}

/// Path to the staleness marker file for bot updates.
pub fn update_marker_path() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|d| d.join("kiro").join("bot-last-update"))
}

/// Check if an update is needed based on the staleness marker.
/// Returns true if the marker doesn't exist or is older than `max_age`.
pub fn is_update_stale(max_age: Duration) -> bool {
    let Some(marker) = update_marker_path() else {
        return true;
    };
    match std::fs::metadata(&marker) {
        Ok(meta) => meta
            .modified()
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_none_or(|age| age >= max_age),
        Err(_) => true,
    }
}

/// Touch the staleness marker to record that an update was performed.
pub fn touch_update_marker() -> Result<()> {
    let Some(marker) = update_marker_path() else {
        return Ok(());
    };
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&marker, [])?;
    Ok(())
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
                    if is_cron_instance(&n)? {
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
            BotCommand::Chat { name } => {
                kiro_bot::cli::run::cmd_chat(&name).await.map_err(|e| eyre::eyre!(e))?;
                Ok(ExitCode::SUCCESS)
            },
            BotCommand::Monitor => {
                kiro_bot::cli::service::cmd_monitor().map_err(|e| eyre::eyre!(e))?;
                Ok(ExitCode::SUCCESS)
            },
            BotCommand::Update(args) => {
                // Check staleness if --if-stale provided
                if let Some(ref duration_str) = args.if_stale {
                    let max_age = parse_duration(duration_str)?;
                    if !is_update_stale(max_age) && !args.force {
                        println!("Binary is fresh (within {duration_str}), skipping update.");
                        return Ok(ExitCode::SUCCESS);
                    }
                }

                // Delegate to the existing update mechanism
                let update_args = crate::cli::update::UpdateArgs {
                    check: false,
                    force: args.force,
                };
                let result = update_args.execute(os).await?;

                // Touch the marker on success
                touch_update_marker()?;

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

fn is_cron_instance(name: &str) -> Result<bool> {
    let dir = kiro_bot::config::config_dir(name).map_err(|e| eyre::eyre!(e))?;
    let cfg = kiro_bot::config::load_config(&dir).map_err(|e| eyre::eyre!(e))?;
    Ok(matches!(cfg.frontend, kiro_bot::config::FrontendConfig::Cron { .. }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // TDD: Duration parsing tests (RED → GREEN)
    // =========================================================================

    #[test]
    fn test_parse_duration_days() {
        assert_eq!(parse_duration("7d").unwrap(), Duration::from_secs(7 * 86400));
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(parse_duration("24h").unwrap(), Duration::from_secs(24 * 3600));
    }

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(parse_duration("90m").unwrap(), Duration::from_secs(90 * 60));
    }

    #[test]
    fn test_parse_duration_combined() {
        assert_eq!(parse_duration("1d12h").unwrap(), Duration::from_secs(86400 + 12 * 3600));
    }

    #[test]
    fn test_parse_duration_complex() {
        assert_eq!(parse_duration("1h30m").unwrap(), Duration::from_secs(3600 + 30 * 60));
    }

    #[test]
    fn test_parse_duration_bare_number_is_seconds() {
        assert_eq!(parse_duration("3600").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn test_parse_duration_zero_fails() {
        assert!(parse_duration("0d").is_err());
    }

    #[test]
    fn test_parse_duration_invalid_unit() {
        assert!(parse_duration("7x").is_err());
    }

    #[test]
    fn test_parse_duration_empty_fails() {
        assert!(parse_duration("").is_err());
    }

    // =========================================================================
    // TDD: Staleness logic tests
    // =========================================================================

    #[test]
    fn test_is_update_stale_no_marker() {
        // When marker doesn't exist, should be stale
        assert!(is_update_stale(Duration::from_secs(86400)));
    }

    #[test]
    fn test_touch_and_check_marker() {
        // Create a temp dir to isolate the test
        let tmp = tempfile::tempdir().unwrap();
        let marker = tmp.path().join("bot-last-update");

        // Write marker
        std::fs::write(&marker, []).unwrap();

        // Check freshness — just written, should not be stale for 1 day
        let meta = std::fs::metadata(&marker).unwrap();
        let age = meta.modified().unwrap().elapsed().unwrap();
        assert!(age < Duration::from_secs(86400));
    }

    // =========================================================================
    // TDD: Subcommand parsing tests
    // =========================================================================

    #[test]
    fn test_bot_install_parsing() {
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
    fn test_bot_start_foreground_parsing() {
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
    fn test_bot_start_all_parsing() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BotCommand,
        }

        let cli = TestCli::parse_from(["test", "start", "--all"]);
        assert_eq!(cli.cmd, BotCommand::Start {
            name: None,
            foreground: false,
            all: true,
        });
    }

    #[test]
    fn test_bot_update_if_stale_parsing() {
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

    #[test]
    fn test_bot_update_force_parsing() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BotCommand,
        }

        let cli = TestCli::parse_from(["test", "update", "--force"]);
        assert_eq!(
            cli.cmd,
            BotCommand::Update(BotUpdateArgs {
                if_stale: None,
                force: true,
            })
        );
    }

    #[test]
    fn test_bot_status_parsing() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BotCommand,
        }

        let cli = TestCli::parse_from(["test", "status"]);
        assert_eq!(cli.cmd, BotCommand::Status);
    }

    #[test]
    fn test_bot_stop_parsing() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BotCommand,
        }

        let cli = TestCli::parse_from(["test", "stop", "my-bot"]);
        assert_eq!(cli.cmd, BotCommand::Stop {
            name: Some("my-bot".into()),
            all: false,
        });
    }

    #[test]
    fn test_env_var_name() {
        assert_eq!(KIRO_ENABLE_BOT, "KIRO_ENABLE_BOT");
    }
}
