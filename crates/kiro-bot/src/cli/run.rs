//! Bot runtime — wires config, secrets, frontends, and ACP together.

use std::sync::Arc;

use anyhow::{
    Context,
    Result,
};
use slack_morphism::prelude::*;
use tokio::sync::{
    mpsc,
    oneshot,
};
use tracing::info;

use crate::config::{
    self,
    Config,
    FrontendConfig,
    Secrets,
    TomlReply,
    TomlTrigger,
};
use crate::engine::acp::{
    self,
    AcpConfig,
    AcpInfo,
    AcpRuntimeState,
    ApprovalPolicy,
};
use crate::engine::attachment_read::AttachmentReadAuthorizer;
use crate::engine::authz::Authorizer;
use crate::engine::core::BotCore;
use crate::engine::response_policy::{
    Location,
    ResponsePolicy,
    ResponsePolicyConfig,
    Trigger,
};
use crate::engine::user_map::UserMap;
use crate::frontend::cli::CliFrontend;
use crate::frontend::slack::{
    PendingApprovals,
    SlackFrontend,
    SlackSocketState,
    SlackState,
    on_error,
    on_interaction,
    on_push,
    spawn_approval_listener,
};

const ACP_DRAIN_GRACE: std::time::Duration = std::time::Duration::from_secs(20);
const DISPATCH_DRAIN_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

/// Run a bot instance in the foreground (called by `start --foreground`).
pub async fn cmd_run(name: &str) -> Result<()> {
    let instance_dir = config::config_dir(name)?;
    let cfg = config::load_config(&instance_dir)?;
    let secrets = config::load_secrets(&instance_dir)?;
    std::env::set_current_dir(&instance_dir)?;
    run_bot(cfg, secrets).await
}

/// Run the full bot lifecycle: build core, connect Slack, serve until shutdown.
async fn run_bot(cfg: Config, secrets: Secrets) -> Result<()> {
    let response_policy = build_response_policy(&cfg)?;
    let authz = build_authorizer(&cfg)?;
    let user_map = Arc::new(match cfg.users {
        Some(users) => UserMap::from_map(users),
        None => UserMap::empty(),
    });

    let FrontendConfig::Slack {
        bot_name,
        bot_member_id,
        conversation_history,
    } = cfg.frontend
    else {
        anyhow::bail!("Instance is not a Slack frontend — use `kiro-bot run` for cron");
    };
    let Secrets::Slack(slack_secrets) = secrets;

    set_working_directory(&cfg.working_directory)?;

    let approval_policy = cfg.agent.approval_policy;
    let max_active_work_items = cfg.agent.max_active_work_items;
    let (approval_tx, approval_rx) = if approval_policy != ApprovalPolicy::Deny {
        let (tx, rx) = mpsc::unbounded_channel();
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };
    let attachment_reads = Arc::new(AttachmentReadAuthorizer::default());

    let acp_cfg = AcpConfig {
        command: cfg.agent.command,
        model_id: cfg.agent.model,
        bot_user: bot_name,
        mcp_wait_ms: cfg.agent.mcp_wait_ms,
        default_mode: cfg.agent.default_mode,
        max_workers: cfg.agent.max_workers,
        idle_timeout_secs: cfg.agent.idle_timeout_secs,
        approval_policy,
        approval_tx,
        attachment_reads: attachment_reads.clone(),
    };

    let (work_tx, work_rx) = mpsc::unbounded_channel::<acp::Work>();
    let (ready_tx, ready_rx) = oneshot::channel();
    let acp_info = acp::spawn_acp_thread(work_rx, ready_tx, acp_cfg);
    ready_rx.await.context("ACP thread died")?;
    info!("ACP ready, starting Slack listener");

    let slack_connector = SlackClientHyperConnector::new()?.with_rate_control(
        SlackApiRateControlConfig::new()
            .with_max_retries(0)
            .with_max_delay_timeout(std::time::Duration::from_secs(10)),
    );
    let slack_client = Arc::new(SlackClient::new(slack_connector));
    let bot_token = SlackApiToken::new(slack_secrets.bot_token.clone().into());
    let feedback_writer = build_feedback_writer().await;

    let frontend = Arc::new(SlackFrontend::new(
        slack_client.clone(),
        bot_token.clone(),
        user_map.clone(),
        conversation_history.unwrap_or(10),
        attachment_reads,
        feedback_writer.is_some(),
    )?);

    // Hoisted above the coordinator so the self-id we write into DDB is the
    // same `<ip>:<port>` peers will POST to. `0` would yield a `<ip>:0`
    // self-id that peers can't reach — refuse it explicitly rather than
    // silently let `unwrap_or` paper over it.
    let dispatch_port = match std::env::var("KIRO_BOT_DISPATCH_PORT") {
        Ok(raw) => match raw.parse::<u16>() {
            Ok(0) | Err(_) => anyhow::bail!("KIRO_BOT_DISPATCH_PORT must be 1-65535, got {raw:?}"),
            Ok(p) => p,
        },
        Err(_) => 8080,
    };
    // Resolve our own task id once. On Fargate this is `<private-ipv4>:<port>`
    // so peers can forward straight here; outside Fargate it falls back to
    // `hostname-pid`. Used both as the lease owner identity (DynamoCoordinator)
    // and as the comparison target in the reaction-forward short-circuit;
    // resolving once avoids the two callsites diverging if metadata is flaky.
    let own_task_id = crate::engine::task_metadata::resolve_self_id(dispatch_port).await;
    let coordinator = crate::engine::coordinator_bootstrap::build_coordinator(own_task_id.clone()).await;
    let core = BotCore {
        work_sender: work_tx,
        work_capacity: BotCore::work_capacity(max_active_work_items),
        inflight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        authz,
        response_policy,
        acp_info,
        coordinator: coordinator.clone(),
        lease_manager: crate::engine::coordinator::LeaseManager::new(coordinator.clone()),
        rate_limit: cfg.rate_limit,
    };

    let pending_approvals: PendingApprovals = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    if let Some(rx) = approval_rx {
        spawn_approval_listener(
            rx,
            slack_client.clone(),
            bot_token.clone(),
            pending_approvals.clone(),
            coordinator.clone(),
        );
    }

    let (bot_user_id, bot_id) = {
        let session = slack_client.open_session(&bot_token);
        session.auth_test().await.map_or_else(
            |_| (String::new(), String::new()),
            |response| {
                (
                    response.user_id.to_string(),
                    response.bot_id.map(|id| id.to_string()).unwrap_or_default(),
                )
            },
        )
    };
    info!(bot_user_id, bot_id, "Bot authenticated");

    let state = Arc::new(SlackState {
        core,
        frontend,
        user_id: String::new(),
        member_id: bot_member_id.unwrap_or_default(),
        bot_user_id,
        bot_id,
        user_map,
        pending_approvals,
        feedback_writer,
        own_task_id,
    });

    let env = Arc::new(
        SlackClientEventsListenerEnvironment::new(slack_client.clone())
            .with_error_handler(on_error)
            .with_user_state(SlackSocketState::new(state.clone()))
            .with_user_state(state.clone()),
    );

    let socket_config = SlackClientSocketModeConfig::new().with_ping_interval_in_seconds(30);
    let listener = SlackClientSocketModeListener::new(
        &socket_config,
        env,
        SlackSocketModeListenerCallbacks::new()
            .with_interaction_events(on_interaction)
            .with_push_events(on_push),
    );
    // Phase 2: bind the dispatch server with a real Dispatcher that replays
    // forwarded Slack events through the same handlers as Slack-native
    // delivery. `dispatch_port` was resolved earlier so the self-id stitched
    // into the lease table matches the port we listen on here.
    //
    // The listener is bound *before* Slack ingress starts accepting events: a
    // peer holding the lease for a conversation may forward to us as soon as we
    // are reachable, and a `POST /dispatch` that lands before the socket exists
    // is refused outright.
    let dispatcher: Arc<dyn crate::engine::dispatch_server::Dispatcher> = Arc::new(
        crate::engine::coordinator_bootstrap::BotCoreDispatcher::new(state.clone()),
    );
    let bound_dispatch = crate::engine::dispatch_server::bind_dispatch_server(
        dispatch_port,
        dispatcher,
        crate::engine::dispatch_server::dispatch_token(),
    )
    .await?;

    listener
        .listen_for(&SlackApiToken::new(slack_secrets.app_token.clone().into()))
        .await?;
    info!("⚡ Bot started");

    let mut dispatch_handle = tokio::spawn(bound_dispatch.serve());

    let runtime_error = tokio::select! {
        exit_code = listener.serve() => listener_exit_error(exit_code),
        _ = shutdown_signal() => {
            info!("Shutdown signal received; draining runtime");
            None
        }
        failure = wait_for_acp_failure(state.core.acp_info.clone()) => {
            Some(anyhow::anyhow!("ACP runtime failed: {failure}"))
        }
        dispatch_result = &mut dispatch_handle => {
            Some(match dispatch_result {
                Ok(Ok(())) => anyhow::anyhow!("dispatch server stopped unexpectedly"),
                Ok(Err(error)) => anyhow::anyhow!("dispatch server failed: {error}"),
                Err(error) => anyhow::anyhow!("dispatch server task failed: {error}"),
            })
        }
    };
    dispatch_handle.abort();
    request_acp_shutdown(&state.core.work_sender).await;
    wait_for_dispatch_drain(&state.core.inflight).await;
    match runtime_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Run interactive CLI chat mode (no Slack).
pub async fn cmd_chat(name: &str) -> Result<()> {
    let instance_dir = config::config_dir(name)?;
    let cfg = config::load_config(&instance_dir)?;
    std::env::set_current_dir(&instance_dir)?;
    // Same two-step as run_bot: chdir into the instance so relative config
    // paths resolve, then hand the agent its real workspace. `new_session`
    // passes `current_dir()` as the session root, so without this the agent is
    // rooted in $KIRO_HOME/bots/<name> — which agents/kiro-help.json lists in
    // `read.deniedPaths`, making its `./**` allowance resolve to a denied path.
    set_working_directory(&cfg.working_directory)?;
    let max_active_work_items = cfg.agent.max_active_work_items;

    let acp_cfg = AcpConfig {
        command: cfg.agent.command,
        model_id: cfg.agent.model,
        bot_user: "cli-user".into(),
        mcp_wait_ms: cfg.agent.mcp_wait_ms,
        default_mode: cfg.agent.default_mode,
        max_workers: cfg.agent.max_workers,
        idle_timeout_secs: cfg.agent.idle_timeout_secs,
        approval_policy: ApprovalPolicy::Approve,
        approval_tx: None,
        attachment_reads: Arc::new(AttachmentReadAuthorizer::default()),
    };

    let (work_tx, work_rx) = mpsc::unbounded_channel::<acp::Work>();
    let (ready_tx, ready_rx) = oneshot::channel();
    let acp_info = acp::spawn_acp_thread(work_rx, ready_tx, acp_cfg);
    ready_rx.await.context("ACP thread died")?;

    let frontend = Arc::new(CliFrontend::new());
    let coordinator: Arc<dyn crate::engine::coordinator::Coordinator> =
        Arc::new(crate::engine::coordinator::NoopCoordinator::new());
    let core = BotCore {
        work_sender: work_tx,
        work_capacity: BotCore::work_capacity(max_active_work_items),
        inflight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        authz: None,
        response_policy: Arc::new(ResponsePolicyConfig::default_policy()),
        acp_info,
        coordinator: coordinator.clone(),
        lease_manager: crate::engine::coordinator::LeaseManager::new(coordinator),
        rate_limit: cfg.rate_limit,
    };

    eprintln!("Ready. Type messages (prefix #name for multi-conversation). Ctrl-C to quit.");
    crate::frontend::cli::run_cli(&core, frontend).await;
    request_acp_shutdown(&core.work_sender).await;
    wait_for_dispatch_drain(&core.inflight).await;
    Ok(())
}

async fn request_acp_shutdown(work_sender: &mpsc::UnboundedSender<acp::Work>) {
    let (reply_tx, reply_rx) = oneshot::channel();
    if work_sender
        .send(acp::Work::Shutdown {
            grace: ACP_DRAIN_GRACE,
            reply_tx,
        })
        .is_err()
    {
        tracing::warn!("ACP control channel already closed during shutdown");
        return;
    }
    match tokio::time::timeout(ACP_DRAIN_GRACE + std::time::Duration::from_secs(2), reply_rx).await {
        Ok(Ok(())) => info!("ACP runtime drained"),
        Ok(Err(_)) => tracing::warn!("ACP runtime stopped without acknowledging drain"),
        Err(_) => tracing::warn!("ACP runtime drain timed out"),
    }
}

async fn wait_for_dispatch_drain(inflight: &Arc<std::sync::Mutex<std::collections::HashSet<String>>>) {
    if tokio::time::timeout(DISPATCH_DRAIN_GRACE, async {
        loop {
            if inflight.lock().unwrap().is_empty() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .is_err()
    {
        let active_dispatches = inflight.lock().unwrap().len();
        tracing::warn!(active_dispatches, "dispatch drain timed out");
    }
}

async fn wait_for_acp_failure(acp_info: Arc<std::sync::Mutex<AcpInfo>>) -> String {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
    loop {
        interval.tick().await;
        let info = acp_info.lock().unwrap();
        if matches!(info.runtime_state, AcpRuntimeState::Failed | AcpRuntimeState::Stopped) {
            return info
                .last_failure
                .clone()
                .unwrap_or_else(|| format!("runtime entered {:?} state", info.runtime_state));
        }
    }
}

fn listener_exit_error(exit_code: i32) -> Option<anyhow::Error> {
    (exit_code != 0).then(|| anyhow::anyhow!("Slack listener stopped unexpectedly with code {exit_code}"))
}

#[cfg(unix)]
async fn shutdown_signal() {
    let mut terminate =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("install SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = terminate.recv() => {},
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_response_policy(cfg: &Config) -> Result<Arc<ResponsePolicyConfig>> {
    let policies: Vec<ResponsePolicy> = cfg
        .response_policies
        .iter()
        .map(|p| ResponsePolicy {
            scope: p.conversation.clone(),
            trigger: match p.trigger {
                TomlTrigger::Always => Trigger::Always,
                TomlTrigger::DirectedOnly => Trigger::DirectedOnly,
                TomlTrigger::ThreadOnly => Trigger::ThreadOnly,
            },
            location: match p.reply {
                TomlReply::Inline => Location::Same,
                TomlReply::Thread => Location::Thread,
            },
            thread_pattern: None,
        })
        .collect();

    Ok(Arc::new(if policies.is_empty() {
        ResponsePolicyConfig::default_policy()
    } else {
        ResponsePolicyConfig::from_policies(policies)?
    }))
}

fn build_authorizer(cfg: &Config) -> Result<Option<Arc<Authorizer>>> {
    if let Some(authz_cfg) = &cfg.authorization {
        let authorizer = Authorizer::new(
            &authz_cfg.cedar_policy_file,
            authz_cfg.cedar_template_values.as_deref(),
            authz_cfg.cedar_entities_file.as_deref(),
        )
        .context("Failed to load Cedar policies")?;
        info!("Cedar authorizer initialized");
        Ok(Some(Arc::new(authorizer)))
    } else {
        Ok(None)
    }
}

fn set_working_directory(wd: &Option<String>) -> Result<()> {
    if let Some(wd) = wd {
        let expanded = if let Some(suffix) = wd.strip_prefix("~/") {
            dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?
                .join(suffix)
        } else if wd == "~" {
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?
        } else {
            std::path::PathBuf::from(wd)
        };
        std::env::set_current_dir(&expanded)
            .with_context(|| format!("failed to set working_directory: {}", expanded.display()))?;
    }
    Ok(())
}

/// Run a cron/headless instance — single prompt, output, exit.
pub async fn cmd_cron(name: &str) -> Result<()> {
    crate::frontend::cron::run_once(name).await
}

/// Run a cron instance as a scheduled daemon loop.
pub async fn cmd_cron_daemon(name: &str) -> Result<()> {
    crate::frontend::cron::run_scheduled(name).await
}

/// Build a [`crate::engine::feedback::DynamoFeedbackWriter`] when the runtime
/// is configured for it (env var `KIRO_BOT_FEEDBACK_TABLE` set). Returns
/// `None` when feedback persistence is disabled — typical for the local CLI
/// frontends, the cron daemon, or any non-Slack invocation.
async fn build_feedback_writer() -> Option<std::sync::Arc<dyn crate::engine::feedback::FeedbackWriter>> {
    let table = match std::env::var("KIRO_BOT_FEEDBACK_TABLE") {
        Ok(v) if !v.is_empty() => v,
        _ => return None,
    };
    let cfg = aws_config::defaults(aws_config::BehaviorVersion::latest()).load().await;
    let client = aws_sdk_dynamodb::Client::new(&cfg);
    Some(std::sync::Arc::new(crate::engine::feedback::DynamoFeedbackWriter::new(
        client, table,
    )))
}

#[cfg(test)]
mod tests {
    use super::listener_exit_error;

    #[test]
    fn clean_listener_exit_is_not_an_error() {
        assert!(listener_exit_error(0).is_none());
    }

    #[test]
    fn failed_listener_exit_reports_code() {
        let error = listener_exit_error(23).expect("nonzero listener exit should fail");
        assert_eq!(error.to_string(), "Slack listener stopped unexpectedly with code 23");
    }
}
