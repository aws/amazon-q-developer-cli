//! Dev/operator probe — captures the live Taskei MCP gateway's
//! `tools/list` response into the schema-pin fixture at
//! `src/families/taskei/fixtures/taskei-tools.json`.
//!
//! Usage (locally, with `kiro-bot` profile creds):
//!
//! ```text
//! AWS_PROFILE=kiro-bot AWS_REGION=us-east-1 \
//!     cargo run -p kiro-mcp --bin dump-taskei-tools
//! ```
//!
//! Filled in by Phase 1d.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    kiro_mcp::dumper::run().await
}
