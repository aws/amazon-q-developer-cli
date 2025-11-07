mod run;

use std::process::ExitCode;

use clap::{
    ArgAction,
    Parser,
    Subcommand,
};
use eyre::{
    Context,
    Result,
};
use run::RunArgs;
use tracing_appender::non_blocking::{
    NonBlocking,
    WorkerGuard,
};
use tracing_appender::rolling::{
    RollingFileAppender,
    Rotation,
};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{
    EnvFilter,
    Registry,
};

use crate::acp::{
    acp_agent,
    acp_client,
};

#[derive(Debug, Clone, Parser)]
pub struct CliArgs {
    #[command(subcommand)]
    pub subcommand: Option<RootSubcommand>,
    /// Increase logging verbosity
    #[arg(long, short = 'v', action = ArgAction::Count, global = true)]
    pub verbose: u8,
}

impl CliArgs {
    pub async fn execute(self) -> Result<ExitCode> {
        let _guard = Self::setup_logging().context("failed to initialize logging")?;

        let subcommand = self.subcommand.unwrap_or_default();

        subcommand.execute().await
    }

    fn setup_logging() -> Result<WorkerGuard> {
        let env_filter = EnvFilter::try_from_default_env().unwrap_or_default();

        // No logging configured, return dummy guard
        let max_level = env_filter.max_level_hint();
        if max_level.is_none() || max_level == Some(tracing::level_filters::LevelFilter::OFF) {
            let (_, guard) = NonBlocking::new(std::io::sink());
            return Ok(guard);
        }

        let (non_blocking, _file_guard) = NonBlocking::new(RollingFileAppender::new(Rotation::NEVER, ".", "chat.log"));
        let file_layer = tracing_subscriber::fmt::layer().with_writer(non_blocking);
        // .with_ansi(false);

        Registry::default().with(env_filter).with(file_layer).init();

        Ok(_file_guard)
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum RootSubcommand {
    /// Run a single prompt
    Run(RunArgs),
    /// Test ACP client
    AcpClient {
        /// Path to the ACP agent executable
        agent_path: String,
    },
    /// ACP server
    Acp,
}

impl RootSubcommand {
    pub async fn execute(self) -> Result<ExitCode> {
        match self {
            RootSubcommand::Run(run_args) => run_args.execute().await,
            RootSubcommand::AcpClient { agent_path } => acp_client::execute(agent_path).await,
            RootSubcommand::Acp => acp_agent::execute().await,
        }
    }
}

impl Default for RootSubcommand {
    fn default() -> Self {
        Self::Run(Default::default())
    }
}
