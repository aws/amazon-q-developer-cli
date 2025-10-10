pub mod chat;
mod run;

use std::process::ExitCode;

use chat::ChatArgs;
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
use tracing::Level;
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
        let _guard = self.setup_logging().context("failed to initialize logging")?;

        let subcommand = self.subcommand.unwrap_or_default();

        subcommand.execute().await
    }

    fn setup_logging(&self) -> Result<WorkerGuard> {
        let log_level = match self.verbose > 0 {
            true => Some(
                match self.verbose {
                    1 => Level::WARN,
                    2 => Level::INFO,
                    3 => Level::DEBUG,
                    _ => Level::TRACE,
                }
                .to_string(),
            ),
            false => None,
        };

        let env_filter = EnvFilter::try_from_default_env().unwrap_or_default();
        let (non_blocking, _file_guard) = NonBlocking::new(RollingFileAppender::new(Rotation::NEVER, ".", "chat.log"));
        let file_layer = tracing_subscriber::fmt::layer().with_writer(non_blocking);
        // .with_ansi(false);

        Registry::default().with(env_filter).with(file_layer).init();

        Ok(_file_guard)
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum RootSubcommand {
    /// TUI Chat Interface
    Chat(ChatArgs),
    /// Run a single prompt
    Run(RunArgs),
}

impl RootSubcommand {
    pub async fn execute(self) -> Result<ExitCode> {
        match self {
            RootSubcommand::Chat(chat_args) => chat_args.execute().await,
            RootSubcommand::Run(run_args) => run_args.execute().await,
        }
    }
}

impl Default for RootSubcommand {
    fn default() -> Self {
        Self::Chat(Default::default())
    }
}
