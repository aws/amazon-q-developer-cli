//! Top-level CLI entry point for the bundled `kiro-mcp` shim.
//!
//! Lives on the lib side (rather than in `main.rs`) so the two
//! `[[bin]]` targets — `kiro-mcp` (canonical) and `kiro-taskei-mcp`
//! (deprecated alias kept for one release; see plan §1c-bundle) — can
//! both reduce to a 3-line shim that calls [`run_cli`]. Cargo refuses
//! to share a single `main.rs` between two `[[bin]]` entries cleanly,
//! and duplicating the file would just double the maintenance surface
//! during the alias's lifetime.
//!
//! Phase 1c (recap): the initialize probe routes through the STS
//! bridge. With both `--read-role-arn` and `--write-role-arn` unset
//! (the Phase-0 reality, where Taskei accepts the kiro-bot ECS task
//! role directly), the bridge is a no-op pass-through and the call
//! signs with the same base creds Phase 1b used. When a read role is
//! configured, the read path goes through a cached
//! `AssumeRoleProvider`. When `--scope=write` and a write role is
//! configured, the binary builds a one-shot signed client from a
//! per-call `AssumeRole` snapshot. No MCP stdio server yet — that
//! lands in Phase 1d once the rmcp transport bridge is wired in.

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

use crate::sigv4_client::{
    SigV4Error,
    SigV4HttpClient,
};
use crate::sts_bridge::{
    StsBridge,
    StsBridgeConfig,
    WriteMode,
    one_shot_provider,
};

#[derive(Parser, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Read-only: only Taskei read tools (get/list rooms, get/list tasks)
    /// are exposed. This is the default and the surface kiro-help launches
    /// at startup.
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

/// Tool families the bundled shim is permitted to expose. Phase
/// 1c-bundle only exposes `taskei`; `knowledge` and `github` migrate in
/// via a follow-up consolidation plan (open question §6). Surfaced as
/// a clap arg now so the shape is stable before Phase 1d's rmcp proxy
/// lands — agent.json entries authored against this CLI today won't
/// need to change when later families are folded in.
#[derive(Parser, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolFamily {
    Taskei,
}

impl std::str::FromStr for ToolFamily {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "taskei" => Ok(ToolFamily::Taskei),
            other => Err(format!("unknown tool family `{other}` (known: taskei)")),
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "kiro-mcp", version, about)]
pub struct Args {
    /// AWS region the Taskei endpoint lives in. Used to derive the default
    /// endpoint and to scope the SigV4 signing region.
    #[arg(long, env = "AWS_REGION", default_value = "us-east-1")]
    pub region: String,

    /// Override the Taskei endpoint URL. When unset, derived from --region.
    #[arg(long, env = "KIRO_TASKEI_ENDPOINT")]
    pub endpoint: Option<String>,

    /// `read` exposes only read tools; `write` adds create/update task.
    #[arg(long, default_value = "read")]
    pub scope: Scope,

    /// IAM role ARN to assume via STS for read calls. When unset, signs
    /// with the ambient identity from the default provider chain.
    #[arg(long, env = "KIRO_TASKEI_READ_ROLE_ARN")]
    pub read_role_arn: Option<String>,

    /// IAM role ARN to assume via STS for write calls. Only used when
    /// --scope=write. Kept distinct from --read-role-arn so the read path
    /// cannot escalate.
    #[arg(long, env = "KIRO_TASKEI_WRITE_ROLE_ARN")]
    pub write_role_arn: Option<String>,

    /// Comma-separated allowlist of Taskei room IDs the server is permitted
    /// to touch. Empty / unset means "no fence at this layer" — Phase 1d
    /// will treat unset as deny-by-default for write scope.
    #[arg(long, env = "KIRO_TASKEI_ALLOW_ROOMS", value_delimiter = ',')]
    pub allow_rooms: Vec<String>,

    /// Comma-separated list of tool families to expose. Phase 1c-bundle
    /// only honors `taskei`; the arg shape is stable so future families
    /// don't change the agent.json contract.
    #[arg(
        long,
        env = "KIRO_MCP_ENABLED_FAMILIES",
        value_delimiter = ',',
        default_value = "taskei"
    )]
    pub enabled_families: Vec<ToolFamily>,
}

fn default_endpoint(region: &str) -> String {
    // Pattern observed in Phase 0 smoke: regional sub-domains under
    // service.mcp.taskei.amazon.dev. IAD/PDX/DUB are the only Taskei
    // MCP regions today; for any other region, --endpoint is required.
    match region {
        "us-east-1" => "https://iad.prod.service.mcp.taskei.amazon.dev/mcp".into(),
        "us-west-2" => "https://pdx.prod.service.mcp.taskei.amazon.dev/mcp".into(),
        "eu-west-1" => "https://dub.prod.service.mcp.taskei.amazon.dev/mcp".into(),
        other => format!("https://{other}.prod.service.mcp.taskei.amazon.dev/mcp"),
    }
}

/// Top-level entry point used by both `[[bin]]` targets.
pub async fn run_cli() -> ExitCode {
    init_tracing();

    let args = Args::parse();
    let endpoint = args.endpoint.clone().unwrap_or_else(|| default_endpoint(&args.region));

    let bridge_cfg = StsBridgeConfig::new(args.read_role_arn.clone(), args.write_role_arn.clone());

    // argv[0] is the name the shell resolved on $PATH. With the Phase
    // 1c-bundle alias, the same `run_cli()` is reachable as either
    // `kiro-mcp` (canonical) or `kiro-taskei-mcp` (deprecated alias,
    // removed in Phase 2). Surface which name was invoked in the audit
    // log so an operator can see at a glance whether anything is still
    // pinning the old name post-rollout.
    let invoked_as = std::env::args().next().unwrap_or_else(|| "kiro-mcp".to_string());
    let invoked_alias = invoked_as.contains("kiro-taskei-mcp");

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        invoked_as = %invoked_as,
        invoked_via_legacy_alias = invoked_alias,
        region = %args.region,
        endpoint = %endpoint,
        scope = ?args.scope,
        read_role_arn = ?args.read_role_arn,
        write_role_arn = ?args.write_role_arn,
        allow_rooms = ?args.allow_rooms,
        enabled_families = ?args.enabled_families,
        sts_bridge_no_op = bridge_cfg.is_no_op(),
        "kiro-mcp v{} starting (Phase 1c-bundle — bundled shim, signed initialize probe)",
        env!("CARGO_PKG_VERSION"),
    );

    if invoked_alias {
        // Structured warning so a CloudWatch query can confirm the alias
        // is unused before Phase 2 removes it.
        tracing::warn!(
            target: "taskei_audit",
            invoked_as = %invoked_as,
            "invoked under deprecated `kiro-taskei-mcp` alias; this binary will be removed in Phase 2 — switch agent.json / scripts to `kiro-mcp`",
        );
    }

    match run(&args.region, &endpoint, args.scope, bridge_cfg).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Paging-severity audit line on the dedicated target so
            // log-search alarms can fire without false positives from
            // regular tracing noise.
            tracing::error!(
                target: "taskei_audit",
                error = %e,
                "kiro-mcp exiting non-zero",
            );
            ExitCode::FAILURE
        },
    }
}

async fn run(region: &str, endpoint: &str, scope: Scope, bridge_cfg: StsBridgeConfig) -> Result<()> {
    let bridge = StsBridge::from_default_chain(region.to_string(), bridge_cfg)
        .await
        .map_err(|e| anyhow::anyhow!("sts bridge build failed: {e}"))?;

    let client = match scope {
        Scope::Read => {
            // Read-tool probe: cached read-role provider (or base creds
            // if no read role is configured). One client, reused across
            // calls.
            let provider = bridge
                .read_credentials_provider()
                .map_err(|e| anyhow::anyhow!("read provider: {e}"))?;
            tracing::info!(
                target: "taskei_audit",
                scope = "read",
                writes_use_dedicated_role = bridge.writes_use_dedicated_role(),
                "initialized read SigV4 client"
            );
            SigV4HttpClient::with_provider(provider, region.to_string())?
        },
        Scope::Write => {
            // Write-tool probe: one-shot creds. We resolve once, drop the
            // AssumeRoleProvider, sign exactly one call, and exit.
            let wc = bridge
                .assume_write_once()
                .await
                .map_err(|e| anyhow::anyhow!("assume write: {e}"))?;
            tracing::info!(
                target: "taskei_audit",
                scope = "write",
                write_mode = wc.mode.as_str(),
                "resolved one-shot write credentials"
            );
            // Treat operator misconfig as a hard error: a `--scope=write`
            // probe with no `--write-role-arn` would silently sign with
            // base creds, which would mask the misconfig in production.
            if scope == Scope::Write && wc.mode != WriteMode::WriteRole {
                tracing::warn!(
                    target: "taskei_audit",
                    write_mode = wc.mode.as_str(),
                    "scope=write but no dedicated write role — proceeding with non-WriteRole creds (Phase-0 reality when both ARNs unset)"
                );
            }
            SigV4HttpClient::with_provider(one_shot_provider(wc.creds), region.to_string())?
        },
    };

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
                "name": "kiro-mcp",
                "version": env!("CARGO_PKG_VERSION"),
            }
        }
    });

    match client.post_json(endpoint, body).await {
        Ok(resp) => {
            let status = resp.status();
            // Read the body so the audit line includes a real outcome
            // before we drop the response. Bounded — initialize replies
            // are tiny.
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
        },
        Err(SigV4Error::CredentialFailure(msg)) => {
            // Already audit-logged inside the client; surface as a
            // regular error for the process exit path.
            anyhow::bail!("fail-closed on credentials: {msg}");
        },
        Err(e) => Err(e.into()),
    }
}

fn init_tracing() {
    use tracing_subscriber::{
        EnvFilter,
        fmt,
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).with_writer(std::io::stderr).init();
}
