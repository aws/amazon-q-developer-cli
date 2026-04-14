//! Bot runtime — wires config, secrets, frontends, and ACP together.

use std::sync::Arc;

use anyhow::{
    Context,
    Result,
};
use kiro_bot::config::{
    self,
    Config,
    FrontendConfig,
    Secrets,
    TomlReply,
    TomlTrigger,
};
use kiro_bot::engine::acp::{
    self,
    AcpConfig,
    ApprovalPolicy,
};
use kiro_bot::engine::authz::Authorizer;
use kiro_bot::engine::core::BotCore;
use kiro_bot::engine::response_policy::{
    Location,
    ResponsePolicy,
    ResponsePolicyConfig,
    Trigger,
};
use kiro_bot::engine::user_map::UserMap;
use kiro_bot::frontend::cli::CliFrontend;
use kiro_bot::frontend::slack::{
    PendingApprovals,
    SlackFrontend,
    SlackState,
    on_error,
    on_push,
    spawn_approval_listener,
};
use slack_morphism::prelude::*;
use tokio::sync::{
    mpsc,
    oneshot,
};
use tracing::info;

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
    let (approval_tx, approval_rx) = if approval_policy == ApprovalPolicy::Ask {
        let (tx, rx) = mpsc::unbounded_channel();
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

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
    };

    let (work_tx, work_rx) = mpsc::unbounded_channel::<acp::Work>();
    let (ready_tx, ready_rx) = oneshot::channel();
    let acp_info = acp::spawn_acp_thread(work_rx, ready_tx, acp_cfg);
    ready_rx.await.context("ACP thread died")?;
    info!("ACP ready, starting Slack listener");

    let slack_client = Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));
    let bot_token = SlackApiToken::new(slack_secrets.bot_token.clone().into());

    let frontend = Arc::new(SlackFrontend {
        client: slack_client.clone(),
        bot_token: bot_token.clone(),
        user_map: user_map.clone(),
        conversation_history: conversation_history.unwrap_or(10),
        last_seen: std::sync::Mutex::new(std::collections::HashMap::new()),
    });

    let core = BotCore {
        work_sender: work_tx,
        inflight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        authz,
        response_policy,
        acp_info,
    };

    let pending_approvals: PendingApprovals = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    if let Some(rx) = approval_rx {
        spawn_approval_listener(rx, slack_client.clone(), bot_token.clone(), pending_approvals.clone());
    }

    let bot_user_id = {
        let session = slack_client.open_session(&bot_token);
        session
            .auth_test()
            .await
            .map(|r| r.user_id.to_string())
            .unwrap_or_default()
    };
    info!(bot_user_id, "Bot authenticated");

    let state = SlackState {
        core,
        frontend,
        user_id: String::new(),
        member_id: bot_member_id.unwrap_or_default(),
        bot_user_id,
        user_map,
        pending_approvals,
    };

    let env = Arc::new(
        SlackClientEventsListenerEnvironment::new(slack_client.clone())
            .with_error_handler(on_error)
            .with_user_state(state),
    );

    let socket_config = SlackClientSocketModeConfig::new().with_ping_interval_in_seconds(30);
    let listener = SlackClientSocketModeListener::new(
        &socket_config,
        env,
        SlackSocketModeListenerCallbacks::new().with_push_events(on_push),
    );
    listener
        .listen_for(&SlackApiToken::new(slack_secrets.app_token.clone().into()))
        .await?;
    info!("⚡ Bot started");

    tokio::select! {
        _ = listener.serve() => {}
        _ = tokio::signal::ctrl_c() => { info!("Shutting down..."); }
    }
    Ok(())
}

/// Run interactive CLI chat mode (no Slack).
pub async fn cmd_chat(name: &str) -> Result<()> {
    let instance_dir = config::config_dir(name)?;
    let cfg = config::load_config(&instance_dir)?;
    std::env::set_current_dir(&instance_dir)?;

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
    };

    let (work_tx, work_rx) = mpsc::unbounded_channel::<acp::Work>();
    let (ready_tx, ready_rx) = oneshot::channel();
    let acp_info = acp::spawn_acp_thread(work_rx, ready_tx, acp_cfg);
    ready_rx.await.context("ACP thread died")?;

    let frontend = Arc::new(CliFrontend::new());
    let core = BotCore {
        work_sender: work_tx,
        inflight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        authz: None,
        response_policy: Arc::new(ResponsePolicyConfig::default_policy()),
        acp_info,
    };

    eprintln!("Ready. Type messages (prefix #name for multi-conversation). Ctrl-C to quit.");
    kiro_bot::frontend::cli::run_cli(&core, frontend).await;
    Ok(())
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
    kiro_bot::frontend::cron::run_once(name).await
}

/// Run a cron instance as a scheduled daemon loop.
pub async fn cmd_cron_daemon(name: &str) -> Result<()> {
    kiro_bot::frontend::cron::run_scheduled(name).await
}
