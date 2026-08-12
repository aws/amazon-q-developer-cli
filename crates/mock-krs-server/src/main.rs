//! Standalone entrypoint, so a harness that is not Rust (the TypeScript E2E
//! suite) can spawn the fake KRS as a child process and drive it over the
//! control API.
//!
//! The "listening on" line is printed to stdout as the readiness signal, the
//! same contract `cloud/mock-bff.mjs` uses, so the harness can wait for the
//! line instead of polling. `--port-file` is the alternative for a harness that
//! would rather read the ephemeral port from disk than parse stdout.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use mock_krs_server::{
    Config,
    MockKrsServer,
};

#[derive(Debug, Parser)]
#[command(
    name = "mock-krs-server",
    about = "Fake Kiro Runtime Service for KAS integration tests"
)]
struct Args {
    /// Port to bind. 0 (the default) picks a free one.
    #[arg(long, default_value_t = 0)]
    port: u16,

    /// Bearer token to require. Any non-empty token is accepted when unset.
    #[arg(long, env = "MOCK_KRS_API_KEY")]
    api_key: Option<String>,

    /// Write the bound port here once listening.
    #[arg(long)]
    port_file: Option<PathBuf>,

    /// Accept requests that carry no user message. Off by default so wire
    /// breakage fails at the first call rather than as a stalled turn.
    #[arg(long)]
    allow_unvalidated_requests: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber_init();
    let args = Args::parse();

    let server = MockKrsServer::start(Config {
        bind: SocketAddr::from(([127, 0, 0, 1], args.port)),
        api_key: args.api_key,
        validate_requests: !args.allow_unvalidated_requests,
    })
    .await?;

    if let Some(path) = &args.port_file {
        std::fs::write(path, server.addr().port().to_string())?;
    }

    println!("[mock-krs] listening on {}", server.endpoint());
    println!("[mock-krs] set KIRO_KAS_ENDPOINT={}", server.endpoint());
    println!("[mock-krs] control API at {}/__control", server.endpoint());

    tokio::signal::ctrl_c().await?;
    server.shutdown().await;
    Ok(())
}

/// Logging is opt-in via `RUST_LOG`; the server's own stdout contract is the
/// three lines above, and unconditional tracing output would muddy it.
fn tracing_subscriber_init() {
    if std::env::var_os("RUST_LOG").is_some() {
        tracing::subscriber::set_global_default(tracing_subscriber::fmt().with_writer(std::io::stderr).finish()).ok();
    }
}
