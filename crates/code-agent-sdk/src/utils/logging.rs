use std::fs::OpenOptions;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{
    EnvFilter,
    fmt,
};

/// Initialize logging to file with trace level for LSP operations
pub fn init_file_logging() -> anyhow::Result<()> {
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("code_intelligence.log")?;

    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(log_file)
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true),
        )
        .with(EnvFilter::from_default_env().add_directive("code_agent_sdk=trace".parse()?))
        .try_init()
        .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {e}"))?;

    Ok(())
}
