//! Deprecated alias binary — `kiro-taskei-mcp` resolves to the same
//! `run_cli()` body as `kiro-mcp`. Removed in Phase 2 once
//! `kiro-help.json` and any local scripts have been switched to the
//! canonical name. See [`kiro_mcp::cli`] for the actual entry point
//! and the alias-deprecation warning emitted on startup when this
//! binary is invoked.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    kiro_mcp::cli::run_cli().await
}
