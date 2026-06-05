//! Dev/operator probe — captures the live Taskei MCP gateway's
//! `tools/list` response into the schema-pin fixture at
//! `src/families/taskei/fixtures/taskei-tools.json`.
//!
//! Output format matches [`crate::families::taskei::schema_pin::PinnedFixture`]
//! (name-keyed map with per-tool `readOnlyHint`, `destructiveHint`,
//! and a canonical-JSON SHA-256 of the `inputSchema`). Re-running the
//! dumper produces deterministic bytes — the fixture is checked in,
//! reviewed in PRs, and the runtime schema-pin assertion compares
//! against it via `include_str!` at startup.
//!
//! Usage:
//!
//! ```text
//! AWS_PROFILE=kiro-bot AWS_REGION=us-east-1 \
//!     cargo run -p kiro-mcp --bin dump-taskei-tools
//! ```
//!
//! For a non-default endpoint (e.g. a Taskei dev stack) override via
//! `TASKEI_ENDPOINT=...`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use crate::families::taskei::{
    mcp_proxy,
    schema_pin,
};
use crate::sts_bridge::{
    StsBridge,
    StsBridgeConfig,
};

#[derive(Parser, Debug)]
#[command(
    name = "dump-taskei-tools",
    version,
    about = "Capture the Taskei tools/list fixture for kiro-mcp's schema-pin"
)]
struct DumperArgs {
    #[arg(long, env = "AWS_REGION", default_value = "us-east-1")]
    region: String,

    #[arg(long, env = "TASKEI_ENDPOINT")]
    endpoint: Option<String>,

    #[arg(long, env = "TASKEI_READ_ROLE_ARN")]
    read_role_arn: Option<String>,

    /// Where to write the fixture. Defaults to the canonical path
    /// the runtime schema-pin reads via `include_str!`.
    #[arg(
        long,
        default_value = "crates/kiro-mcp/src/families/taskei/fixtures/taskei-tools.json"
    )]
    output: PathBuf,
}

fn default_endpoint(region: &str) -> String {
    match region {
        "us-east-1" => "https://iad.prod.service.mcp.taskei.amazon.dev/mcp".into(),
        "us-west-2" => "https://pdx.prod.service.mcp.taskei.amazon.dev/mcp".into(),
        "eu-west-1" => "https://dub.prod.service.mcp.taskei.amazon.dev/mcp".into(),
        other => format!("https://{other}.prod.service.mcp.taskei.amazon.dev/mcp"),
    }
}

fn legacy_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.trim().is_empty())
}

/// Top-level entry. Build the bridge, hit `tools/list`, normalize,
/// pretty-print, write. Errors print to stderr and the process exits
/// non-zero.
pub async fn run() -> ExitCode {
    init_tracing();
    let args = DumperArgs::parse();
    let endpoint = args
        .endpoint
        .clone()
        .or_else(|| legacy_env("KIRO_TASKEI_ENDPOINT"))
        .unwrap_or_else(|| default_endpoint(&args.region));
    let read_role_arn = args
        .read_role_arn
        .clone()
        .or_else(|| legacy_env("KIRO_TASKEI_READ_ROLE_ARN"));

    eprintln!(
        "dump-taskei-tools: region={} endpoint={} output={}",
        args.region,
        endpoint,
        args.output.display()
    );

    let bridge_cfg = StsBridgeConfig::new(read_role_arn, None);
    let bridge = match StsBridge::from_default_chain(args.region.clone(), bridge_cfg).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: sts bridge build failed: {e}");
            return ExitCode::FAILURE;
        },
    };
    let view = bridge.read_only();

    let protocol_version = match mcp_proxy::initialize_remote(&view, &endpoint, &args.region).await {
        Ok(version) => version,
        Err(e) => {
            eprintln!("error: initialize failed: {}", e.message);
            return ExitCode::FAILURE;
        },
    };

    let live_tools = match mcp_proxy::list_tools_remote(&view, &endpoint, &args.region).await {
        Ok(tools) => tools,
        Err(e) => {
            eprintln!("error: tools/list failed: {}", e.message);
            return ExitCode::FAILURE;
        },
    };

    eprintln!("captured {} tools from gateway", live_tools.len());
    let fixture = schema_pin::normalize(&live_tools, protocol_version);

    let pretty = match serde_json::to_string_pretty(&fixture) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: serialize fixture: {e}");
            return ExitCode::FAILURE;
        },
    };

    // Trailing newline — POSIX-conventional and keeps git diffs clean.
    let bytes = format!("{pretty}\n");
    if let Some(parent) = args.output.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        eprintln!("error: create_dir_all({}): {e}", parent.display());
        return ExitCode::FAILURE;
    }
    if let Err(e) = std::fs::write(&args.output, bytes.as_bytes()) {
        eprintln!("error: write {}: {e}", args.output.display());
        return ExitCode::FAILURE;
    }
    eprintln!(
        "wrote {} bytes to {} ({} tool entries)",
        bytes.len(),
        args.output.display(),
        fixture.tools.len()
    );
    ExitCode::SUCCESS
}

fn init_tracing() {
    use tracing_subscriber::{
        EnvFilter,
        fmt,
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).with_writer(std::io::stderr).init();
}
