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
//! Phase 1d: the binary now stands up an rmcp stdio server that
//! proxies `tools/list` and `tools/call` to the IAD prod Taskei
//! gateway with SigV4 signing. Before serving, it runs the
//! schema-pin assertion (plan §305-313) — a checked-in fixture
//! pinning each tool's `readOnlyHint`, `destructiveHint`, and a
//! canonical-JSON SHA-256 of its `inputSchema`. Mismatch causes a
//! paging-severity log + non-zero exit so a drifted gateway never
//! reaches user traffic.
//!
//! Phase 1c (recap): the STS bridge is in the path. With both
//! `--read-role-arn` and `--write-role-arn` unset (the Phase-0
//! reality, where Taskei accepts the kiro-bot ECS task role
//! directly), the bridge is a no-op pass-through and signs with the
//! same base creds Phase 1b used. With a read role configured,
//! reads sign with cached `AssumeRoleProvider` snapshots; with a
//! write role configured, writes hop STS per call.

use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use rmcp::ServiceExt;
use rmcp::transport::stdio;

use crate::families::taskei::{
    TaskeiServer,
    schema_pin,
    tools as taskei_tools,
};
use crate::sts_bridge::{
    StsBridge,
    StsBridgeConfig,
};

/// Operational scope flag — break-glass per plan §609-615. Phase 1d's
/// runtime path doesn't *use* this for tool routing (the rmcp
/// server's `call_tool` dispatches by `is_read_tool` / `is_write_tool`
/// from the curated overlay table), but the flag is retained so an
/// operator can launch a process-wide read-only sidecar in
/// emergency-debug shapes that want OS-enforced isolation back.
#[derive(Parser, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Default. Both read and write tools listed; write tools route
    /// through the per-call STS write path. The kiro-bot reaction-gate
    /// upstream is what actually fences writes — see plan §270 / §296
    /// `readOnlyHint` policy.
    Both,
    /// Refuse to dispatch any write tool, regardless of its annotation.
    /// Process-level break-glass: useful for "I want to be SURE this
    /// process can't write."
    ReadOnly,
}

impl std::str::FromStr for Scope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "both" | "all" => Ok(Scope::Both),
            // Accept the legacy `read` / `read-only` spellings —
            // Phase 1c shipped `--scope=read` and `--scope=write`.
            // Phase 1d's effective values are `both` / `read-only`,
            // and we collapse the old `read` to `read-only` and
            // accept (but warn) on `write` as `both`.
            "read" | "read-only" => Ok(Scope::ReadOnly),
            "write" => Ok(Scope::Both),
            other => Err(format!("expected `both` or `read-only`, got `{other}`")),
        }
    }
}

/// Tool families the bundled shim is permitted to expose. Phase
/// 1c-bundle only honors `taskei`; `knowledge` and `github` migrate in
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
    #[arg(long, env = "TASKEI_ENDPOINT")]
    pub endpoint: Option<String>,

    /// `both` exposes the full curated catalog (default); `read-only`
    /// is a break-glass that refuses to dispatch any write tool
    /// regardless of its annotation. Plan §609-615.
    #[arg(long, default_value = "both")]
    pub scope: Scope,

    /// IAM role ARN to assume via STS for read calls. When unset, signs
    /// with the ambient identity from the default provider chain.
    #[arg(long, env = "TASKEI_READ_ROLE_ARN")]
    pub read_role_arn: Option<String>,

    /// IAM role ARN to assume via STS for write calls. Kept distinct
    /// from --read-role-arn so the read path cannot escalate.
    #[arg(long, env = "TASKEI_WRITE_ROLE_ARN")]
    pub write_role_arn: Option<String>,

    /// Comma-separated allowlist of Taskei room IDs the server is
    /// permitted to touch. Empty / unset means "no fence at this layer"
    /// — the gateway's room-membership ACL is still authoritative;
    /// this is just an additional client-side guard against
    /// misconfigured agents. Plan §299-300.
    #[arg(long, env = "TASKEI_ALLOW_ROOMS", value_delimiter = ',')]
    pub allow_rooms: Vec<String>,

    /// Comma-separated list of tool families to expose. Phase
    /// 1c-bundle only honors `taskei`; the arg shape is stable so
    /// future families don't change the agent.json contract.
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

fn legacy_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn legacy_env_csv(name: &str) -> Vec<String> {
    legacy_env(name)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Top-level entry point used by both `[[bin]]` targets.
pub async fn run_cli() -> ExitCode {
    init_tracing();

    let args = Args::parse();
    let endpoint = args
        .endpoint
        .clone()
        .or_else(|| legacy_env("KIRO_TASKEI_ENDPOINT"))
        .unwrap_or_else(|| default_endpoint(&args.region));
    let read_role_arn = args
        .read_role_arn
        .clone()
        .or_else(|| legacy_env("KIRO_TASKEI_READ_ROLE_ARN"));
    let write_role_arn = args
        .write_role_arn
        .clone()
        .or_else(|| legacy_env("KIRO_TASKEI_WRITE_ROLE_ARN"));
    let allow_rooms = if args.allow_rooms.is_empty() {
        legacy_env_csv("KIRO_TASKEI_ALLOW_ROOMS")
    } else {
        args.allow_rooms.clone()
    };

    let bridge_cfg = StsBridgeConfig::new(read_role_arn.clone(), write_role_arn.clone());

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
        read_role_arn = ?read_role_arn,
        write_role_arn = ?write_role_arn,
        allow_rooms = ?allow_rooms,
        enabled_families = ?args.enabled_families,
        sts_bridge_no_op = bridge_cfg.is_no_op(),
        "kiro-mcp v{} starting (Phase 1d — rmcp stdio proxy + schema-pin)",
        env!("CARGO_PKG_VERSION"),
    );

    if invoked_alias {
        // Structured warning so a CloudWatch query can confirm the
        // alias is unused before Phase 2 removes it.
        tracing::warn!(
            target: "taskei_audit",
            invoked_as = %invoked_as,
            "invoked under deprecated `kiro-taskei-mcp` alias; this binary will be removed in Phase 2 — switch agent.json / scripts to `kiro-mcp`",
        );
    }

    match run(&args, &endpoint, bridge_cfg, allow_rooms).await {
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

async fn run(args: &Args, endpoint: &str, bridge_cfg: StsBridgeConfig, allow_rooms: Vec<String>) -> Result<()> {
    let bridge = Arc::new(
        StsBridge::from_default_chain(args.region.clone(), bridge_cfg)
            .await
            .map_err(|e| anyhow::anyhow!("sts bridge build failed: {e}"))?,
    );

    // Schema-pin assertion (plan §305-313, §389-394). Runs against the
    // live gateway BEFORE we accept any stdio traffic. Mismatch exits
    // non-zero with one taskei_audit error per discrepancy. Skipped
    // when the operator sets the documented break-glass env var.
    let view = bridge.read_only();
    let live_tools = schema_pin::assert_pinned_schema(&view, endpoint, &args.region)
        .await
        .map_err(|e| anyhow::anyhow!("schema-pin: {e}"))?;

    // Phase 1d's break-glass `--scope=read-only` flag is plumbed down
    // to TaskeiServer via the curated tool catalog: when read-only,
    // the catalog is filtered to read-classified tools only, so
    // upstream callers never see a write tool name in `tools/list`
    // and can't dispatch one. The TaskeiServer also rejects unknown
    // names locally, so the gate is effective at both layers.
    let tools = taskei_tools::decorate_with_overlay(live_tools);
    let server = TaskeiServer::with_catalog(bridge.clone(), endpoint, args.region.clone(), allow_rooms, tools);
    let server = match args.scope {
        Scope::Both => server,
        Scope::ReadOnly => server.filter_to_read_only(),
    };

    tracing::info!(
        target: "taskei_audit",
        scope = ?args.scope,
        "kiro-mcp ready — accepting stdio MCP traffic"
    );

    let service = server
        .serve(stdio())
        .await
        .map_err(|e| anyhow::anyhow!("stdio serve: {e}"))?;
    service
        .waiting()
        .await
        .map_err(|e| anyhow::anyhow!("stdio waiting: {e}"))?;
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::{
        EnvFilter,
        fmt,
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // CRITICAL: stdout is the MCP transport. All log output MUST go
    // to stderr; a stray log byte on stdout corrupts the JSON-RPC
    // stream and the upstream client hangs.
    fmt().with_env_filter(filter).with_writer(std::io::stderr).init();
}
