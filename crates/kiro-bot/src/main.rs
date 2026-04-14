//! CLI entrypoint for kiro-bot.

use anyhow::Result;
use clap::{
    Parser,
    Subcommand,
};

mod cli;

#[derive(Parser)]
#[command(name = "kiro-bot", about = "Manage ACP-backed Slack bot instances")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Install a bot instance from a config directory
    Install { path: String },

    /// Uninstall a bot instance
    #[command(group = clap::ArgGroup::new("target").required(true))]
    Uninstall {
        #[arg(group = "target")]
        name: Option<String>,
        #[arg(long, group = "target")]
        all: bool,
    },

    /// Start a bot instance (background daemon; use --foreground to attach)
    #[command(group = clap::ArgGroup::new("target").required(true))]
    Start {
        #[arg(group = "target")]
        name: Option<String>,
        /// Run attached to the terminal (for debugging)
        #[arg(long, requires = "name", conflicts_with = "all")]
        foreground: bool,
        /// Start all installed instances
        #[arg(long, group = "target")]
        all: bool,
    },

    /// Stop a running bot instance
    #[command(group = clap::ArgGroup::new("target").required(true))]
    Stop {
        #[arg(group = "target")]
        name: Option<String>,
        #[arg(long, group = "target")]
        all: bool,
    },

    /// List all instances and their status
    Status,

    /// Run a cron/headless instance once and exit
    Run { name: String },

    /// Interactive CLI chat without Slack
    Chat { name: String },

    /// Start the web monitoring dashboard
    Monitor {
        /// Port to listen on
        #[arg(long, default_value = "9090")]
        port: u16,
    },
}

fn resolve_names(name: Option<String>, all: bool) -> Result<Vec<String>> {
    if all {
        kiro_bot::config::all_instance_names()
    } else {
        Ok(vec![name.unwrap()])
    }
}

fn is_cron_instance(name: &str) -> Result<bool> {
    let dir = kiro_bot::config::config_dir(name)?;
    let cfg = kiro_bot::config::load_config(&dir)?;
    Ok(matches!(cfg.frontend, kiro_bot::config::FrontendConfig::Cron { .. }))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cli = Cli::parse();
    match cli.command {
        Commands::Install { path } => cli::install::cmd_install(&path),
        Commands::Uninstall { name, all } => {
            for n in resolve_names(name, all)? {
                cli::install::cmd_uninstall(&n)?;
            }
            Ok(())
        },
        Commands::Start { name, foreground, all } => {
            if foreground {
                let n = name.unwrap();
                if is_cron_instance(&n)? {
                    cli::run::cmd_cron_daemon(&n).await
                } else {
                    cli::run::cmd_run(&n).await
                }
            } else {
                for n in resolve_names(name, all)? {
                    cli::service::cmd_start(&n)?;
                }
                Ok(())
            }
        },
        Commands::Stop { name, all } => {
            for n in resolve_names(name, all)? {
                cli::service::cmd_stop(&n)?;
            }
            Ok(())
        },
        Commands::Status => cli::service::cmd_status(),
        Commands::Run { name } => cli::run::cmd_cron(&name).await,
        Commands::Chat { name } => cli::run::cmd_chat(&name).await,
        Commands::Monitor { port: _ } => cli::service::cmd_monitor(),
    }
}
