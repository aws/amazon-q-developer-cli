//! kiro-taskei-mcp — MCP stdio server backed by Amazon Taskei.
//!
//! Phase 1b: this binary now signs and sends a real MCP `initialize` POST
//! to the configured Taskei endpoint and prints the response status. No MCP
//! stdio server yet — that lands in Phase 1d once the rmcp transport
//! bridge is wired in. The point of 1b is to prove the credential +
//! signing path end-to-end inside the kiro-bot ECS task role before any
//! agent-loaded MCP traffic goes through it.
//!
//! Credentials note: this binary takes NO --aws-profile flag. At runtime it
//! uses the AWS default provider chain — task role in ECS, profile from
//! the ambient env locally (e.g. AWS_PROFILE=kiro-bot).

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

use kiro_taskei_mcp::sigv4_client::{SigV4Error, SigV4HttpClient};

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
    /// endpoint and to scope the SigV4 signing region.
    #[arg(long, env = "AWS_REGION", default_value = "us-east-1")]
    region: String,

    /// Override the Taskei endpoint URL. When unset, derived from --region.
    #[arg(long, env = "KIRO_TASKEI_ENDPOINT")]
    endpoint: Option<String>,

    /// `read` exposes only read tools; `write` adds create/update task.
    #[arg(long, default_value = "read")]
    scope: Scope,

    /// IAM role ARN to assume via STS for read calls. When unset, signs
    /// with the ambient identity from the default provider chain.
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

fn default_endpoint(region: &str) -> String {
    // Pattern observed in Phase 0 smoke: regional sub-domains under
    // service.mcp.taskei.amazon.dev. IAD/PDX/DUB are the only Taskei MCP
    // regions today; for any other region, --endpoint is required.
    match region {
        "us-east-1" => "https://iad.prod.service.mcp.taskei.amazon.dev/mcp".into(),
        "us-west-2" => "https://pdx.prod.service.mcp.taskei.amazon.dev/mcp".into(),
        "eu-west-1" => "https://dub.prod.service.mcp.taskei.amazon.dev/mcp".into(),
        other => format!("https://{other}.prod.service.mcp.taskei.amazon.dev/mcp"),
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    let args = Args::parse();
    let endpoint = args
        .endpoint
        .clone()
        .unwrap_or_else(|| default_endpoint(&args.region));

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        region = %args.region,
        endpoint = %endpoint,
        scope = ?args.scope,
        read_role_arn = ?args.read_role_arn,
        write_role_arn = ?args.write_role_arn,
        allow_rooms = ?args.allow_rooms,
        "kiro-taskei-mcp v{} starting (Phase 1b — signed initialize probe)",
        env!("CARGO_PKG_VERSION"),
    );

    match run(&args.region, &endpoint).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Paging-severity audit line on the dedicated target so log-search
            // alarms can fire without false positives from regular tracing
            // noise.
            tracing::error!(
                target: "taskei_audit",
                error = %e,
                "kiro-taskei-mcp exiting non-zero",
            );
            ExitCode::FAILURE
        }
    }
}

async fn run(region: &str, endpoint: &str) -> Result<()> {
    let client = SigV4HttpClient::from_default_chain(region.to_string()).await?;

    // MCP initialize. Phase 1d will replace this with rmcp's transport
    // driving real tool calls; today this is just an end-to-end signing
    // probe so deploy verification doesn't depend on a real tool.
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "kiro-taskei-mcp",
                "version": env!("CARGO_PKG_VERSION"),
            }
        }
    });

    match client.post_json(endpoint, body).await {
        Ok(resp) => {
            let status = resp.status();
            // Read the body so the audit line includes a real outcome before
            // we drop the response. Bounded — initialize replies are tiny.
            let text = resp.text().await.unwrap_or_default();
            tracing::info!(
                status = %status,
                body_len = text.len(),
                "initialize probe complete",
            );
            if !status.is_success() {
                anyhow::bail!("initialize returned non-success: {status} body={text}");
            }
            Ok(())
        }
        Err(SigV4Error::CredentialFailure(msg)) => {
            // Already audit-logged inside the client; surface as a regular
            // error for the process exit path.
            anyhow::bail!("fail-closed on credentials: {msg}");
        }
        Err(e) => Err(e.into()),
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::fmt;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
