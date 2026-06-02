//! Canonical `kiro-mcp` binary entrypoint. The CLI body lives in
//! [`kiro_mcp::cli::run_cli`] so the deprecated `kiro-taskei-mcp` alias
//! shim under `src/bin/kiro-taskei-mcp.rs` reduces to the same call.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    kiro_mcp::cli::run_cli().await
}
