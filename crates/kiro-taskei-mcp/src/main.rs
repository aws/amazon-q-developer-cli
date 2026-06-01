//! kiro-taskei-mcp — MCP stdio server that will expose Taskei-backed tools to
//! the kiro-help bot. Phase 1a: this is intentionally a STUB. It parses the
//! CLI surface that later phases will fill in, initializes tracing, prints a
//! startup line, and exits 0. No HTTP, no SigV4, no STS, no MCP server yet —
//! those land in Phase 1b (SigV4 client), 1c (STS role assumption), and 1d
//! (rmcp ServerHandler with read tools).
//!
//! Credentials note: this binary takes NO --aws-profile flag. At runtime it
//! will use the AWS default provider chain — task role in ECS, profile from
//! the ambient env locally (e.g. AWS_PROFILE=kiro-bot).

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// Read-only: only Taskei read tools (get/list rooms, get/list tasks) are
    /// exposed. This is the default and the surface kiro-help launches at
    /// startup.
    Read,
    /// Write-capable: read tools plus create/update task. Bot launches a
    /// distinct write instance behind the reaction-approval gate.
    Write,
}

impl std::str::FromStr for Scope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "read" => Ok(Scope::Read),
            "write" => Ok(Scope::Write),
            other => Err(format!("expected `read` or `write`, got `{other}`")),
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "kiro-taskei-mcp", version, about)]
struct Args {
    /// AWS region the Taskei endpoint lives in. Used to derive the default
    /// endpoint and to scope the SigV4 signing region in Phase 1b.
    #[arg(long, env = "AWS_REGION", default_value = "us-east-1")]
    region: String,

    /// Override the Taskei endpoint URL. When unset, Phase 1b will derive it
    /// from --region.
    #[arg(long, env = "KIRO_TASKEI_ENDPOINT")]
    endpoint: Option<String>,

    /// `read` exposes only read tools; `write` adds create/update task.
    #[arg(long, default_value = "read")]
    scope: Scope,

    /// IAM role ARN to assume via STS for read calls. When unset, Phase 1c
    /// signs with the ambient identity from the default provider chain.
    #[arg(long, env = "KIRO_TASKEI_READ_ROLE_ARN")]
    read_role_arn: Option<String>,

    /// IAM role ARN to assume via STS for write calls. Only used when
    /// --scope=write. Kept distinct from --read-role-arn so the read path
    /// cannot escalate.
    #[arg(long, env = "KIRO_TASKEI_WRITE_ROLE_ARN")]
    write_role_arn: Option<String>,

    /// Comma-separated allowlist of Taskei room IDs the server is permitted
    /// to touch. Empty / unset means "no fence at this layer" — Phase 1d
    /// will treat unset as deny-by-default for write scope.
    #[arg(long, env = "KIRO_TASKEI_ALLOW_ROOMS", value_delimiter = ',')]
    allow_rooms: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        region = %args.region,
        endpoint = ?args.endpoint,
        scope = ?args.scope,
        read_role_arn = ?args.read_role_arn,
        write_role_arn = ?args.write_role_arn,
        allow_rooms = ?args.allow_rooms,
        "kiro-taskei-mcp v{} starting (stub — Phase 1a)",
        env!("CARGO_PKG_VERSION"),
    );

    Ok(())
}
