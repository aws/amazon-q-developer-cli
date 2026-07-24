use std::ffi::OsString;
use std::path::{
    Path,
    PathBuf,
};
use std::process::ExitCode;
use std::time::{
    SystemTime,
    UNIX_EPOCH,
};

pub use chat_cli_v2::launch_options::{
    AgentEngine,
    AgentMode,
    Interactivity,
    LaunchOptions,
};
use eyre::{
    Context as _,
    Result,
    bail,
};
use kiro_telemetry::metric::{
    AgentKind,
    ClientApplication,
    Engine,
    ExitReason,
};
use tracing::{
    debug,
    info,
};
use uuid::Uuid;

use crate::database::Database;
use crate::embedded_tui::extract_tui_assets_if_needed;
use crate::os::Os;
use crate::telemetry::TelemetryThread;
use crate::util::consts::env_var::{
    KIRO_CHAT_CLI_BIN,
    KIRO_KAS_NODE_PATH,
    KIRO_KAS_SERVER_PATH,
    KIRO_REMOTE_SESSIONS_ENDPOINT,
    KIRO_TELEMETRY_CLIENT_ID,
    KIRO_TUI_FORCE_COLOR,
    KIRO_VERSION_OVERRIDE,
};
use crate::util::launch_spinner::start_launch_spinner;

mod v1;
pub use v1::launch as launch_v1;

/// Launch the session according to the configured options.
pub async fn launch(options: LaunchOptions, os: &Os, telemetry_name: String) -> Result<ExitCode> {
    let LaunchOptions {
        agent_engine,
        mode,
        interactivity,
        trust_all_tools,
        agent,
        model,
        trust_tools,
    } = options;

    emit_cli_invocation_telemetry(
        &os.telemetry,
        &os.database,
        Some(telemetry_name),
        Engine::from_agent_kind(agent_kind_for_agent_engine(agent_engine)),
    )
    .await;
    emit_cli_session_started(&os.telemetry, &os.database, agent_engine).await;

    let non_interactive = matches!(&interactivity, Interactivity::NonInteractive { .. });
    run_kas_gc_on_startup(os, agent_engine, !non_interactive).await;

    let mut cli_session_completion_emitted = false;
    let result = if let Interactivity::NonInteractive { input } = interactivity {
        launch_acp_non_interactive(
            os,
            agent_engine,
            mode,
            input,
            trust_all_tools,
            agent,
            model,
            trust_tools,
            &mut cli_session_completion_emitted,
        )
        .await
    } else {
        launch_acp_interactive(os, agent_engine, mode, &mut cli_session_completion_emitted).await
    };

    if should_emit_launch_error_completion(&result, cli_session_completion_emitted) {
        emit_cli_session_completed(&os.telemetry, &os.database, agent_engine, ExitReason::Crash).await;
    }

    result
}

/// Garbage-collect stale extracted KAS bundles at launch. Only relevant to the
/// KAS engine. Runs synchronously for non-interactive sessions (no UI to block,
/// and it makes the cleanup observable); for interactive sessions it is
/// dispatched to a detached task so TUI startup is never blocked by directory
/// deletion. Best-effort: failures are swallowed.
async fn run_kas_gc_on_startup(os: &Os, agent_engine: AgentEngine, background: bool) {
    if !matches!(agent_engine, AgentEngine::Kas) {
        return;
    }
    let Ok(kas_root) = crate::util::paths::kas_bundle_dir() else {
        return;
    };
    let current = crate::embedded_tui::kas_version();
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    if background {
        tokio::spawn(async move {
            // A fresh handle so the task owns its state; both crates' databases
            // resolve to the same file.
            let Ok(database) = crate::database::Database::new_default().await else {
                return;
            };
            crate::embedded_tui::run_gc(
                &database,
                &kas_root,
                current.as_deref(),
                now_ms,
                crate::embedded_tui::KAS_VERSION_MAX_AGE,
                crate::embedded_tui::is_pid_alive,
            )
            .await;
        });
    } else {
        crate::embedded_tui::run_gc(
            &os.database,
            &kas_root,
            current.as_deref(),
            now_ms,
            crate::embedded_tui::KAS_VERSION_MAX_AGE,
            crate::embedded_tui::is_pid_alive,
        )
        .await;
    }
}

async fn emit_cli_session_started(telemetry: &TelemetryThread, database: &Database, agent_engine: AgentEngine) {
    if let Err(err) = telemetry
        .send_cli_session_started(database, client_application_for_agent_engine(agent_engine))
        .await
    {
        debug!(%err, ?agent_engine, "failed to emit CLI session-start telemetry");
    }
}

pub(crate) async fn emit_cli_invocation_telemetry(
    telemetry: &TelemetryThread,
    database: &Database,
    telemetry_name: Option<String>,
    engine: Engine,
) {
    if database.record_heartbeat_if_needed()
        && let Err(err) = telemetry.send_daily_heartbeat(database, engine).await
    {
        debug!(%err, ?engine, "failed to emit daily-heartbeat telemetry");
    }
    if let Some(telemetry_name) = telemetry_name
        && let Err(err) = telemetry
            .send_cli_subcommand_executed(database, telemetry_name, engine)
            .await
    {
        debug!(%err, ?engine, "failed to emit CLI subcommand telemetry");
    }
}

async fn emit_cli_session_completed(
    telemetry: &TelemetryThread,
    database: &Database,
    agent_engine: AgentEngine,
    exit_reason: ExitReason,
) {
    if let Err(err) = telemetry
        .send_cli_session_completed(database, exit_reason, agent_kind_for_agent_engine(agent_engine))
        .await
    {
        debug!(%err, ?agent_engine, ?exit_reason, "failed to emit CLI session-completion telemetry");
    }
}

fn client_application_for_agent_engine(agent_engine: AgentEngine) -> ClientApplication {
    match agent_engine {
        AgentEngine::V1 => ClientApplication::ChatCli,
        AgentEngine::V2 => ClientApplication::ChatCliV2,
        AgentEngine::Kas => ClientApplication::ChatCliV3,
    }
}

/// `app/AmazonQ-For-CLI` is required for backend ALB routing / `ClientMetadataUtil`;
/// KAS derives the rest of the user agent itself.
fn client_info_user_agent_meta() -> agent_client_protocol::Meta {
    let mut meta = agent_client_protocol::Meta::new();
    meta.insert("userAgentTags".to_string(), serde_json::json!(["app/AmazonQ-For-CLI"]));
    meta
}

fn agent_kind_for_agent_engine(agent_engine: AgentEngine) -> AgentKind {
    match agent_engine {
        AgentEngine::V1 => AgentKind::V1,
        AgentEngine::V2 => AgentKind::V2,
        AgentEngine::Kas => AgentKind::Kas,
    }
}

fn exit_reason_for_status(status: Option<&std::process::ExitStatus>) -> ExitReason {
    match status {
        None => ExitReason::UserInterrupt,
        Some(status) if status.success() => ExitReason::Clean,
        Some(_) => ExitReason::Crash,
    }
}

fn exit_reason_for_exit_code(exit_code: ExitCode) -> ExitReason {
    if exit_code == ExitCode::SUCCESS {
        ExitReason::Clean
    } else {
        ExitReason::Crash
    }
}

fn should_emit_launch_error_completion(result: &Result<ExitCode>, completion_emitted: bool) -> bool {
    result.is_err() && !completion_emitted
}

/// Determine the color level for the TUI process.
///
/// Returns `None` when color should not be forced (i.e. `NO_COLOR` is set),
/// otherwise returns the color level:
/// - User's explicit `FORCE_COLOR` value if already set
/// - `"3"` (truecolor) when `COLORTERM` is `truecolor` or `24bit`
/// - `"3"` (truecolor) when a known truecolor-capable terminal is detected by identity
/// - `"2"` (256-color) as fallback
///
/// Terminal identity detection covers cases where `COLORTERM` is stripped (e.g. tmux, SSH)
/// but the terminal still supports truecolor.
fn is_truecolor_terminal() -> bool {
    std::env::var("KITTY_WINDOW_ID").is_ok()
        || std::env::var("ALACRITTY_LOG").is_ok()
        || matches!(
            std::env::var("TERM_PROGRAM").ok().as_deref(),
            Some("kitty" | "ghostty" | "WezTerm" | "iTerm.app")
        )
        || matches!(
            std::env::var("TERM").ok().as_deref(),
            Some("xterm-kitty" | "xterm-ghostty")
        )
}

fn resolve_force_color(
    no_color: bool,
    force_color: Option<String>,
    colorterm: Option<&str>,
    truecolor_terminal: bool,
) -> Option<String> {
    if no_color {
        return None;
    }
    if let Some(val) = force_color {
        return Some(val);
    }
    match colorterm {
        Some("truecolor" | "24bit") => Some("3".to_string()),
        _ if truecolor_terminal => Some("3".to_string()),
        _ => Some("2".to_string()),
    }
}

/// Environment variables unconditionally forwarded to the TUI child process.
/// Extracted into a pure helper so the forwarded set stays unit-testable.
///
/// The caller resolves the version override and effective telemetry identity so
/// this helper stays free of process-env reads and is testable with explicit inputs.
fn tui_child_env(
    current_exe: &Path,
    version: OsString,
    telemetry_client_id: Uuid,
    force_color: Option<String>,
) -> Vec<(&'static str, OsString)> {
    let mut env = vec![
        // Path to chat_cli itself, so the TUI can invoke its headless
        // `chat _ export-session` / `chat _ import-session` subcommands for
        // /chat save and /chat load, so V2 spawns the ACP child from this
        // binary, and so the TUI voice helper
        // (packages/tui/src/commands/voice-helper.ts) spawns the `voice`
        // subcommand of the same binary for microphone capture and Whisper
        // transcription. Engine-agnostic: V2 and KAS both route their slash
        // commands through the same Rust binary.
        (KIRO_CHAT_CLI_BIN, current_exe.as_os_str().to_owned()),
        // Forward the resolved version so the TUI bundle reports the real
        // release version (consumed by `getCliVersion()` in
        // `packages/tui/src/utils/version.ts`) instead of its baked-in
        // `0.0.0-dev` placeholder / `99.99.99-dev` dev fallback. Keeps the
        // survey User-Agent and KAS clientInfo aligned with the Rust user agent.
        (KIRO_VERSION_OVERRIDE, version),
        (
            KIRO_TELEMETRY_CLIENT_ID,
            telemetry_client_id.hyphenated().to_string().into(),
        ),
    ];
    // Kept on a private variable, not FORCE_COLOR: the TUI forwards its env to
    // the tools it spawns, which must not inherit a forced color level.
    if let Some(force_color) = force_color {
        env.push((KIRO_TUI_FORCE_COLOR, force_color.into()));
    }
    env
}

/// Default BFF endpoint for remote/cloud sandbox sessions, keyed to the auth
/// stage. KAS treats a set endpoint as the opt-in to run cloud machinery, so
/// owning the default here (rather than in KAS) keeps that gate a deliberate
/// client choice while removing per-client duplication. Stage is inferred from
/// the auth portal URL — the only stage signal the CLI has — so cloud sessions
/// hit the same stage the user authenticated against.
fn default_remote_sessions_endpoint(auth_portal_url: Option<&str>) -> &'static str {
    // Exact match is intentional: an unrecognized portal (variant spelling,
    // port, private IdC) conservatively maps to prod; preprod testers who need
    // a nonstandard portal set the endpoint env var explicitly.
    match auth_portal_url.map(str::trim) {
        Some("https://gamma.app.kiro.dev") => "https://gamma.app.kiro.dev",
        Some("https://beta.app.kiro.dev") => "https://beta.app.kiro.dev",
        _ => "https://app.kiro.dev",
    }
}

/// Resolve the endpoint the launcher sets on the KAS child, or `None` to leave
/// it unset (KAS then keeps cloud sessions dark). An explicit
/// `KIRO_REMOTE_SESSIONS_ENDPOINT` always wins so preprod testing can override;
/// otherwise the stage default applies only when the `remote_sandbox` rollout
/// is enabled for this user.
fn resolve_remote_sessions_endpoint(
    parent_override: Option<String>,
    rollout_enabled: bool,
    auth_portal_url: Option<&str>,
) -> Option<String> {
    if let Some(v) = parent_override.filter(|v| !v.trim().is_empty()) {
        return Some(v);
    }
    if rollout_enabled {
        return Some(default_remote_sessions_endpoint(auth_portal_url).to_string());
    }
    None
}

/// Launch the interactive TUI. Extracts embedded assets and spawns bun with the TUI JS bundle.
async fn launch_acp_interactive(
    os: &Os,
    agent_engine: AgentEngine,
    mode: Option<AgentMode>,
    cli_session_completion_emitted: &mut bool,
) -> Result<ExitCode> {
    // Show a spinner immediately so the user knows the CLI is starting. The
    // guard stops the spinner and clears its line on every exit path — normal
    // return, `?`/`bail!` early return, or panic unwind — so a startup failure
    // never leaves a stuck spinner colliding with the error message. It is
    // `None` (no spinner) when stderr is not a TTY or NO_COLOR is set.
    let spinner = start_launch_spinner();

    // Long-lived session: resolve the Toolbox version in the background so a later
    // mid-session issue report / diagnostics does not block on it. A plain OS thread
    // (not a Tokio task) is used so process exit never waits on the probe.
    std::thread::spawn(|| {
        kiro_telemetry_host::get_accurate_install_method();
    });

    let asset_paths = extract_tui_assets_if_needed(os).await?;

    let args: Vec<String> = std::env::args().collect();
    let current_exe = std::env::current_exe()?;

    let force_color = resolve_force_color(
        std::env::var_os("NO_COLOR").is_some(),
        std::env::var("FORCE_COLOR").ok(),
        std::env::var("COLORTERM").ok().as_deref(),
        is_truecolor_terminal(),
    );

    let mut cmd = tokio::process::Command::new(&asset_paths.bun_path);
    cmd.arg(&asset_paths.tui_js_path)
        .args(&args[1..])
        .env("JSC_numberOfGCMarkers", "1")
        // Surface the real CLI version to the TUI. The embedded TUI bundle's
        // package.json is pinned to "0.0.0-dev" in-repo and isn't bumped by
        // the tag-based release flow, so the TUI reads this env (carrying
        // CARGO_PKG_VERSION, set from KIRO_VERSION at build time) to show the
        // correct version in its footer.
        .env("KIRO_VERSION", env!("CARGO_PKG_VERSION"))
        // Overwrite inherited values so the stable-internal Lite rollout cannot be bypassed.
        .env(
            "KIRO_LITE_ROLLOUT_ENABLED",
            if crate::rollout::rollout().is_enabled(crate::rollout::Feature::Lite) {
                "1"
            } else {
                "0"
            },
        )
        // ICECAP infra-safety gate. The TUI advertises the `infrastructureSafety`
        // capability and honors the `infraSafetyMonitor` / `infraSafetyEnforce`
        // settings only when this is "1". Set server-authoritatively from the
        // Feature::InfraSafety rollout decision (internal-only via rollout.json)
        // and write "0" otherwise — overwriting any inherited value so a user
        // can't force it on by exporting the var in their shell.
        .env(
            "KIRO_INFRA_SAFETY_ROLLOUT_ENABLED",
            if crate::rollout::rollout().is_enabled(crate::rollout::Feature::InfraSafety) {
                "1"
            } else {
                "0"
            },
        )
        .kill_on_drop(true);

    // Forward the unconditional env vars (binary paths, version) to the TUI.
    // Honor a user/parent-provided KIRO_VERSION_OVERRIDE instead of clobbering
    // it; fall back to the crate's compile-time version when unset.
    let version_override =
        std::env::var_os(KIRO_VERSION_OVERRIDE).unwrap_or_else(|| OsString::from(env!("CARGO_PKG_VERSION")));
    for (key, value) in tui_child_env(&current_exe, version_override, os.telemetry.client_id(), force_color) {
        cmd.env(key, value);
    }

    // Propagate voice.serverUrl setting so the TUI uses a remote voice server
    // (cloud desktop scenario) instead of spawning the local voice binary.
    #[cfg(feature = "voice")]
    if let Some(url) = os
        .database
        .settings
        .get_string(crate::database::settings::Setting::VoiceServerUrl)
    {
        cmd.env("KIRO_VOICE_SERVER_URL", &url);
    }

    // Write feed.json to data dir and pass the path to the TUI (avoids 100KB env var).
    // The parent directory is normally created by extract_tui_assets_if_needed when
    // assets are extracted, but in test mode (KIRO_TEST_TUI_JS_PATH set) the
    // extraction path is skipped, so the data dir must be created here.
    let feed_path = crate::util::paths::feed_json_path()?;
    if let Some(parent) = feed_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Nightly builds serve the remotely published feed via a cache refreshed
    // in the background, so launch never blocks on the network: this launch
    // snapshots what the previous launch fetched (bundled feed on cache miss).
    // Written atomically so a concurrent launch never tears the file under a
    // running TUI. Other channels always get the bundled feed.
    crate::cli::feed::atomic_write(&feed_path, &crate::cli::feed::Feed::load_cached_json())?;
    crate::cli::feed::Feed::refresh_cache_in_background();
    cmd.env("KIRO_FEED_FILE", &feed_path);

    // Signal Amazon-internal authentication to the TUI so KAS-mode /feedback
    // routes to Taskei instead of GitHub. Sourced from the auth token (same
    // source as `whoami`), set on the child only — no global env mutation.
    let is_amzn = matches!(
        crate::auth::builder_id::BuilderIdToken::load(&os.database, None).await,
        Ok(Some(token)) if token.is_amzn_user()
    );
    if is_amzn {
        cmd.env("KIRO_INTERNAL", "1");
    }

    cmd.env(
        "KIRO_ENABLED_FEATURES",
        serde_json::to_string(&crate::rollout::rollout().enabled_features()).unwrap_or_default(),
    );

    // Resolve telemetry identity for the TUI
    let telemetry_enabled = !crate::util::env_var::is_telemetry_disabled()
        && os
            .database
            .settings
            .get_bool(crate::database::settings::Setting::TelemetryEnabled)
            .unwrap_or(true);
    cmd.env("KIRO_TELEMETRY_ENABLED", telemetry_enabled.to_string());

    // Propagate the content-collection (service-improvement) opt-in to KAS, which
    // otherwise defaults to opted out and stamps x-amzn-codewhisperer-optout on every
    // request — suppressing DataHub/KCO conversation storage for v3. Sourced from the
    // same setting the V1/V2 OptOutInterceptor uses (default: opted in). The TUI child
    // forwards its env to the KAS subprocess, so setting it here covers the v3 path.
    cmd.env(
        crate::util::consts::env_var::KIRO_CONTENT_COLLECTION_ENABLED,
        crate::cli::content_collection_enabled(os).to_string(),
    );
    for env_var in [
        crate::util::consts::env_var::KIRO_TELEMETRY_OTEL,
        crate::util::consts::env_var::KIRO_TELEMETRY_EXPORT_INTERVAL_MS,
        crate::util::consts::env_var::KIRO_TELEMETRY_OTLP_LOGS_ENABLED,
    ] {
        if let Ok(value) = std::env::var(env_var)
            && !value.trim().is_empty()
        {
            cmd.env(env_var, value);
        }
    }

    // Preserve a non-empty override or supply the regional default only when telemetry is enabled.
    let parent_otlp_endpoint = std::env::var(crate::util::consts::env_var::KIRO_TELEMETRY_OTLP_ENDPOINT).ok();
    if telemetry_enabled {
        cmd.env(
            crate::util::consts::env_var::KIRO_TELEMETRY_OTLP_ENDPOINT,
            kiro_telemetry::resolve_otlp_endpoint(parent_otlp_endpoint, Some(os.client.region())),
        );
    }

    if let Some(user_id) = os.database.get_telemetry_user_id().ok().flatten() {
        cmd.env("KIRO_USER_ID", user_id);
    }

    match agent_engine {
        AgentEngine::Kas => {
            if !crate::util::platform::can_run_kas() {
                bail!("V3 is currently not supported on this system.");
            }

            cmd.env("KIRO_AGENT_ENGINE", "kas");

            let (node, server) = crate::embedded_tui::ensure_kas_assets(os, true).await?;
            // TEST-ONLY: fall back to bun instead of node to invoke the KAS server.
            let node = node.unwrap_or_else(|| PathBuf::from("bun"));
            cmd.env(KIRO_KAS_NODE_PATH, &node);
            if let Some(server) = server.as_ref() {
                cmd.env(KIRO_KAS_SERVER_PATH, server);
                info!(
                    "Using KAS agent engine, node: {}, server: {}",
                    node.display(),
                    server.display()
                );
            } else {
                info!(
                    "Using KAS agent engine, node: {}, server resolved from @kiro/agent package",
                    node.display()
                );
            }

            // Own the remote-sessions endpoint here so it isn't duplicated per
            // client. Setting it is KAS's opt-in for cloud machinery, so gate
            // the default on the rollout; an explicit env value still wins for
            // preprod. Leaving it unset keeps KAS's cloud path dark.
            if let Some(endpoint) = resolve_remote_sessions_endpoint(
                std::env::var(KIRO_REMOTE_SESSIONS_ENDPOINT).ok(),
                crate::rollout::rollout().is_enabled(crate::rollout::Feature::RemoteSandbox),
                std::env::var("KIRO_AUTH_PORTAL_URL").ok().as_deref(),
            ) {
                info!("Remote sessions endpoint: {endpoint}");
                cmd.env(KIRO_REMOTE_SESSIONS_ENDPOINT, endpoint);
            }
        },
        AgentEngine::V2 => {
            // KIRO_CHAT_CLI_BIN (set unconditionally above) is the canonical
            // path the TUI uses to spawn the V2 ACP child. No additional env
            // var is needed for V2.
        },
        AgentEngine::V1 => {
            unreachable!("V1 engine does not use the ACP launch path");
        },
    }

    if let Some(mode) = mode {
        cmd.env("KIRO_MODE", mode.to_string());
    }

    // Stop the spinner and clear its line before handing the terminal to the
    // TUI process. On any error path above, the guard's Drop already did this.
    drop(spinner);

    let mut child = cmd.spawn()?;

    let status;

    #[cfg(unix)]
    {
        use tokio::signal::unix::{
            SignalKind,
            signal,
        };
        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sighup = signal(SignalKind::hangup())?;
        tokio::select! {
            s = child.wait() => {
                status = Some(s?);
            }
            _ = sigterm.recv() => {
                let _ = child.kill().await;
                status = None;
            }
            _ = sighup.recv() => {
                let _ = child.kill().await;
                status = None;
            }
            _ = tokio::signal::ctrl_c() => {
                let _ = child.kill().await;
                status = None;
            }
        }
    }
    #[cfg(not(unix))]
    {
        tokio::select! {
            s = child.wait() => {
                status = Some(s?);
            }
            _ = tokio::signal::ctrl_c() => {
                let _ = child.kill().await;
                status = None;
            }
        }
    }

    let exit_reason = exit_reason_for_status(status.as_ref());
    let exit_code = status
        .as_ref()
        .and_then(|s| s.code())
        .map_or(ExitCode::FAILURE, |e| ExitCode::from(e as u8));

    emit_cli_session_completed(&os.telemetry, &os.database, agent_engine, exit_reason).await;
    *cli_session_completion_emitted = true;

    Ok(exit_code)
}

/// Drive a non-interactive V2 session.
#[allow(clippy::too_many_arguments)]
async fn launch_acp_non_interactive(
    os: &Os,
    agent_engine: AgentEngine,
    mode: Option<AgentMode>,
    input: String,
    trust_all_tools: bool,
    agent: Option<String>,
    model: Option<String>,
    trust_tools: Option<Vec<String>>,
    cli_session_completion_emitted: &mut bool,
) -> Result<ExitCode> {
    use agent_client_protocol::{
        self as acp,
        Agent as _,
    };
    use tokio_util::compat::{
        TokioAsyncReadCompatExt,
        TokioAsyncWriteCompatExt,
    };

    struct NonInteractiveAcpClient {
        trust_all_tools: bool,
        trust_tools: Option<Vec<String>>,
    }

    fn non_interactive_error(reason: &str) -> acp::Error {
        acp::Error::internal_error().data(Some(serde_json::json!({
            "reason": format!(
                "{reason} is not supported in non-interactive mode. \
                 Use --trust-all-tools to auto-approve tool use, or drop --no-interactive.",
            ),
        })))
    }

    #[async_trait::async_trait(?Send)]
    impl acp::Client for NonInteractiveAcpClient {
        async fn session_notification(&self, args: acp::SessionNotification) -> acp::Result<()> {
            use std::io::Write as _;
            match args.update {
                acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk {
                    content: acp::ContentBlock::Text(text),
                    ..
                }) => {
                    print!("{}", text.text);
                    let _ = std::io::stdout().flush();
                },
                acp::SessionUpdate::AgentMessageChunk(_) => {},
                acp::SessionUpdate::AgentThoughtChunk(_) => {},
                acp::SessionUpdate::ToolCall(tool_call) => {
                    eprintln!("\n[tool] {}", tool_call.title);
                },
                acp::SessionUpdate::ToolCallUpdate(update) => {
                    if let Some(status) = update.fields.status {
                        eprintln!("[tool] status: {status:?}");
                    }
                },
                _ => {},
            }
            Ok(())
        }

        async fn request_permission(
            &self,
            args: acp::RequestPermissionRequest,
        ) -> acp::Result<acp::RequestPermissionResponse> {
            let should_approve = self.trust_all_tools
                || self.trust_tools.as_ref().is_some_and(|tools| {
                    // Extract toolId from _meta.kiro.toolId (set by KAS)
                    let tool_id = args
                        .meta
                        .as_ref()
                        .and_then(|m| m.get("kiro"))
                        .and_then(|k| k.get("toolId"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    tools.iter().any(|t| t.eq_ignore_ascii_case(tool_id))
                });

            if should_approve {
                let option_id = args
                    .options
                    .iter()
                    .find(|opt| opt.kind == acp::PermissionOptionKind::AllowAlways)
                    .or_else(|| {
                        args.options
                            .iter()
                            .find(|opt| opt.kind == acp::PermissionOptionKind::AllowOnce)
                    })
                    .map(|opt| opt.option_id.clone())
                    .ok_or_else(acp::Error::internal_error)?;
                return Ok(acp::RequestPermissionResponse::new(
                    acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(option_id)),
                ));
            }
            eprintln!(
                "[denied] tool permission approval is not supported in non-interactive mode. \
                 Use --trust-all-tools to auto-approve."
            );
            Err(non_interactive_error("tool permission approval"))
        }

        async fn write_text_file(&self, _args: acp::WriteTextFileRequest) -> acp::Result<acp::WriteTextFileResponse> {
            Err(non_interactive_error("client-side file write"))
        }

        async fn read_text_file(&self, _args: acp::ReadTextFileRequest) -> acp::Result<acp::ReadTextFileResponse> {
            Err(non_interactive_error("client-side file read"))
        }

        async fn create_terminal(&self, _args: acp::CreateTerminalRequest) -> acp::Result<acp::CreateTerminalResponse> {
            Err(non_interactive_error("client-side terminal creation"))
        }

        async fn terminal_output(&self, _args: acp::TerminalOutputRequest) -> acp::Result<acp::TerminalOutputResponse> {
            Err(non_interactive_error("client-side terminal output"))
        }

        async fn release_terminal(
            &self,
            _args: acp::ReleaseTerminalRequest,
        ) -> acp::Result<acp::ReleaseTerminalResponse> {
            Err(non_interactive_error("client-side terminal release"))
        }

        async fn wait_for_terminal_exit(
            &self,
            _args: acp::WaitForTerminalExitRequest,
        ) -> acp::Result<acp::WaitForTerminalExitResponse> {
            Err(non_interactive_error("client-side terminal wait"))
        }

        async fn kill_terminal(&self, _args: acp::KillTerminalRequest) -> acp::Result<acp::KillTerminalResponse> {
            Err(non_interactive_error("client-side terminal kill"))
        }

        async fn ext_method(&self, args: acp::ExtRequest) -> acp::Result<acp::ExtResponse> {
            chat_cli_v2::auth::kas_token::handle_ext_method(args).await
        }

        async fn ext_notification(&self, _args: acp::ExtNotification) -> acp::Result<()> {
            Ok(())
        }
    }

    let current_exe = std::env::current_exe()?;
    let mut cmd = tokio::process::Command::new(&current_exe);
    cmd.arg("acp");
    if matches!(agent_engine, AgentEngine::Kas) {
        cmd.arg("--agent-engine=kas");
    }
    if trust_all_tools {
        cmd.arg("--trust-all-tools");
    }
    if let Some(mode) = mode {
        cmd.env("KIRO_MODE", mode.to_string());
    }
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true);
    debug!(
        ?current_exe,
        ?agent_engine,
        "spawning ACP server subprocess for non-interactive session"
    );
    let mut child = cmd.spawn().context("failed to spawn ACP server subprocess")?;

    let outgoing = child
        .stdin
        .take()
        .ok_or_else(|| eyre::eyre!("failed to capture ACP subprocess stdin"))?
        .compat_write();
    let incoming = child
        .stdout
        .take()
        .ok_or_else(|| eyre::eyre!("failed to capture ACP subprocess stdout"))?
        .compat();

    let local_set = tokio::task::LocalSet::new();
    let result: Result<ExitCode> = local_set
        .run_until(async move {
            let (conn, handle_io) = acp::ClientSideConnection::new(
                NonInteractiveAcpClient {
                    trust_all_tools,
                    trust_tools,
                },
                outgoing,
                incoming,
                |fut| {
                    tokio::task::spawn_local(fut);
                },
            );
            tokio::task::spawn_local(handle_io);

            conn.initialize(
                acp::InitializeRequest::new(acp::ProtocolVersion::V1).client_info(Some(
                    acp::Implementation::new("kiro-cli-non-interactive", env!("CARGO_PKG_VERSION"))
                        .title(Some("Kiro CLI (non-interactive)".to_string()))
                        .meta(client_info_user_agent_meta()),
                )),
            )
            .await
            .context("ACP initialize failed")?;

            let cwd = std::env::current_dir().context("failed to resolve current working directory")?;
            let session = conn
                .new_session(acp::NewSessionRequest::new(cwd))
                .await
                .context("ACP new_session failed")?;

            if let Some(ref agent) = agent
                && let Err(e) = conn
                    .set_session_mode(acp::SetSessionModeRequest::new(
                        session.session_id.clone(),
                        acp::SessionModeId::new(agent.clone()),
                    ))
                    .await
            {
                eprintln!("[warn] failed to set agent '{}': {}", agent, e.message);
            }

            if let Some(ref model) = model
                && let Err(e) = conn
                    .set_session_config_option(acp::SetSessionConfigOptionRequest::new(
                        session.session_id.clone(),
                        acp::SessionConfigId::new("model"),
                        acp::SessionConfigValueId::new(model.clone()),
                    ))
                    .await
            {
                eprintln!("[warn] failed to set model '{}': {}", model, e.message);
            }

            let response = conn
                .prompt(acp::PromptRequest::new(session.session_id, vec![
                    acp::ContentBlock::Text(acp::TextContent::new(input)),
                ]))
                .await;

            // Ensure trailing newline after streamed agent text.
            println!();

            let response = match response {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {}", e.message);
                    return Ok(ExitCode::FAILURE);
                },
            };

            let exit_code = match response.stop_reason {
                acp::StopReason::EndTurn | acp::StopReason::MaxTokens | acp::StopReason::MaxTurnRequests => {
                    ExitCode::SUCCESS
                },
                // Cancelled / Refusal / future variants (StopReason is non_exhaustive) all
                // surface as a failure exit. Listed under the catch-all rather than enumerated
                // so new variants don't accidentally promote to success.
                _ => ExitCode::FAILURE,
            };
            Ok(exit_code)
        })
        .await;

    let _ = child.kill().await;
    let exit_reason = match result.as_ref() {
        Ok(exit_code) => exit_reason_for_exit_code(*exit_code),
        Err(_) => ExitReason::Crash,
    };
    emit_cli_session_completed(&os.telemetry, &os.database, agent_engine, exit_reason).await;
    *cli_session_completion_emitted = true;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_endpoint_default_follows_auth_stage() {
        assert_eq!(default_remote_sessions_endpoint(None), "https://app.kiro.dev");
        assert_eq!(
            default_remote_sessions_endpoint(Some("https://app.kiro.dev")),
            "https://app.kiro.dev"
        );
        assert_eq!(
            default_remote_sessions_endpoint(Some("https://gamma.app.kiro.dev")),
            "https://gamma.app.kiro.dev"
        );
        assert_eq!(
            default_remote_sessions_endpoint(Some("  https://beta.app.kiro.dev  ")),
            "https://beta.app.kiro.dev"
        );
        // An unrecognized portal (e.g. a private IdC start URL) falls back to prod.
        assert_eq!(
            default_remote_sessions_endpoint(Some("https://example.com")),
            "https://app.kiro.dev"
        );
    }

    #[test]
    fn remote_endpoint_unset_when_rollout_off_and_no_override() {
        assert_eq!(resolve_remote_sessions_endpoint(None, false, None), None);
        // A blank env value is treated as unset, not as an override.
        assert_eq!(
            resolve_remote_sessions_endpoint(Some("   ".to_string()), false, None),
            None
        );
    }

    #[test]
    fn remote_endpoint_uses_stage_default_when_rollout_on() {
        assert_eq!(
            resolve_remote_sessions_endpoint(None, true, None),
            Some("https://app.kiro.dev".to_string())
        );
        assert_eq!(
            resolve_remote_sessions_endpoint(None, true, Some("https://gamma.app.kiro.dev")),
            Some("https://gamma.app.kiro.dev".to_string())
        );
        // A blank override must fall through to the stage default, not set a
        // blank endpoint.
        assert_eq!(
            resolve_remote_sessions_endpoint(Some("   ".to_string()), true, None),
            Some("https://app.kiro.dev".to_string())
        );
    }

    #[test]
    fn remote_endpoint_explicit_override_wins_regardless_of_rollout() {
        // Override is honored even when the rollout is off (preprod testing).
        assert_eq!(
            resolve_remote_sessions_endpoint(Some("http://127.0.0.1:8787".to_string()), false, None),
            Some("http://127.0.0.1:8787".to_string())
        );
        // And it beats the stage default when the rollout is on.
        assert_eq!(
            resolve_remote_sessions_endpoint(
                Some("http://127.0.0.1:8787".to_string()),
                true,
                Some("https://gamma.app.kiro.dev")
            ),
            Some("http://127.0.0.1:8787".to_string())
        );
    }

    #[test]
    fn test_tui_child_env_forwards_real_version() {
        // With no user/parent override, the call site resolves the version to
        // the crate's compile-time version and passes it in; otherwise the TUI
        // bundle would fall back to its baked-in `99.99.99-dev` placeholder
        // (e.g. in the survey User-Agent). Pass the default explicitly rather
        // than mutating process env to keep this test parallel-safe.
        let exe = Path::new("/tmp/kiro-cli");
        let env = tui_child_env(
            exe,
            OsString::from(env!("CARGO_PKG_VERSION")),
            uuid::uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e"),
            None,
        );
        let version = env
            .iter()
            .find(|(k, _)| *k == "KIRO_VERSION_OVERRIDE")
            .map(|(_, v)| v.clone());
        assert_eq!(
            version.as_deref(),
            Some(OsString::from(env!("CARGO_PKG_VERSION")).as_os_str()),
            "TUI child env must forward KIRO_VERSION_OVERRIDE set to the crate version"
        );
    }

    #[test]
    fn test_tui_child_env_forwards_explicit_override() {
        // A user/parent-provided KIRO_VERSION_OVERRIDE must win: whatever the
        // caller resolved is forwarded verbatim, not clobbered by the crate
        // version. Passing the value as an explicit param keeps the test free
        // of process-env mutation (parallel-test safe).
        let exe = Path::new("/tmp/kiro-cli");
        let env = tui_child_env(
            exe,
            OsString::from("7.7.7-test"),
            uuid::uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e"),
            None,
        );
        let version = env
            .iter()
            .find(|(k, _)| *k == "KIRO_VERSION_OVERRIDE")
            .map(|(_, v)| v.clone());
        assert_eq!(
            version.as_deref(),
            Some(OsString::from("7.7.7-test").as_os_str()),
            "TUI child env must forward the caller-resolved override value verbatim"
        );
    }

    #[test]
    fn test_tui_child_env_forwards_telemetry_client_id() {
        let exe = Path::new("/tmp/kiro-cli");
        let client_id = uuid::uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e");
        let env = tui_child_env(exe, OsString::from("7.7.7-test"), client_id, None);
        let forwarded = env
            .iter()
            .find(|(key, _)| *key == KIRO_TELEMETRY_CLIENT_ID)
            .map(|(_, value)| value.clone());

        assert_eq!(forwarded, Some(OsString::from("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e")));
    }

    #[test]
    fn test_tui_child_env_uses_private_force_color_var() {
        // The color level must ride on a private variable so the tools the TUI
        // spawns do not inherit a forced color level via FORCE_COLOR.
        let exe = Path::new("/tmp/kiro-cli");
        let env = tui_child_env(
            exe,
            OsString::from("7.7.7-test"),
            uuid::uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e"),
            Some("3".to_string()),
        );
        let forwarded = env
            .iter()
            .find(|(key, _)| *key == KIRO_TUI_FORCE_COLOR)
            .map(|(_, value)| value.clone());
        assert_eq!(forwarded, Some(OsString::from("3")));
        assert!(
            env.iter().all(|(key, _)| *key != "FORCE_COLOR"),
            "TUI child env must never set FORCE_COLOR"
        );
    }

    #[test]
    fn test_tui_child_env_omits_color_var_when_unresolved() {
        let exe = Path::new("/tmp/kiro-cli");
        let env = tui_child_env(
            exe,
            OsString::from("7.7.7-test"),
            uuid::uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e"),
            None,
        );
        assert!(
            env.iter()
                .all(|(key, _)| *key != KIRO_TUI_FORCE_COLOR && *key != "FORCE_COLOR"),
            "no color variable should be set when color is not forced"
        );
    }

    #[test]
    fn test_resolve_force_color_no_color_set() {
        assert_eq!(resolve_force_color(true, None, None, false), None);
    }

    #[test]
    fn test_resolve_force_color_no_color_overrides_force_color() {
        assert_eq!(
            resolve_force_color(true, Some("3".into()), Some("truecolor"), false),
            None
        );
    }

    #[test]
    fn test_resolve_force_color_respects_explicit_force_color() {
        assert_eq!(
            resolve_force_color(false, Some("1".into()), None, false),
            Some("1".into())
        );
    }

    #[test]
    fn test_resolve_force_color_truecolor() {
        assert_eq!(
            resolve_force_color(false, None, Some("truecolor"), false),
            Some("3".into())
        );
    }

    #[test]
    fn test_resolve_force_color_24bit() {
        assert_eq!(resolve_force_color(false, None, Some("24bit"), false), Some("3".into()));
    }

    #[test]
    fn test_resolve_force_color_fallback() {
        assert_eq!(resolve_force_color(false, None, None, false), Some("2".into()));
        assert_eq!(
            resolve_force_color(false, None, Some("256color"), false),
            Some("2".into())
        );
    }

    #[test]
    fn test_resolve_force_color_truecolor_terminal_identity() {
        // When COLORTERM is unset but terminal is identified as truecolor-capable
        assert_eq!(resolve_force_color(false, None, None, true), Some("3".into()));
        // COLORTERM still takes precedence
        assert_eq!(
            resolve_force_color(false, None, Some("truecolor"), true),
            Some("3".into())
        );
        // NO_COLOR still wins
        assert_eq!(resolve_force_color(true, None, None, true), None);
    }

    #[test]
    fn kas_launches_use_v3_client_application() {
        assert_eq!(
            client_application_for_agent_engine(AgentEngine::V2),
            ClientApplication::ChatCliV2
        );
        assert_eq!(
            client_application_for_agent_engine(AgentEngine::Kas),
            ClientApplication::ChatCliV3
        );
    }

    #[test]
    fn v2_and_kas_launches_use_engine_agent_kind() {
        assert_eq!(agent_kind_for_agent_engine(AgentEngine::V2), AgentKind::V2);
        assert_eq!(agent_kind_for_agent_engine(AgentEngine::Kas), AgentKind::Kas);
    }

    #[test]
    fn maps_launch_exit_to_session_completion_reason() {
        assert_eq!(exit_reason_for_exit_code(ExitCode::SUCCESS), ExitReason::Clean);
        assert_eq!(exit_reason_for_exit_code(ExitCode::FAILURE), ExitReason::Crash);
        assert_eq!(exit_reason_for_status(None), ExitReason::UserInterrupt);
    }

    #[test]
    fn emits_completion_for_launch_errors_until_completion_is_recorded() {
        let failed: Result<ExitCode> = Err(eyre::eyre!("setup failed"));
        let succeeded: Result<ExitCode> = Ok(ExitCode::SUCCESS);

        assert!(should_emit_launch_error_completion(&failed, false));
        assert!(!should_emit_launch_error_completion(&failed, true));
        assert!(!should_emit_launch_error_completion(&succeeded, false));
    }

    #[cfg(unix)]
    #[test]
    fn maps_signaled_child_exit_to_crash() {
        use std::os::unix::process::ExitStatusExt;

        let status = std::process::ExitStatus::from_raw(9);

        assert_eq!(exit_reason_for_status(Some(&status)), ExitReason::Crash);
    }

    #[test]
    fn client_info_user_agent_meta_carries_cli_app_tag() {
        let meta = client_info_user_agent_meta();
        let tags = meta
            .get("userAgentTags")
            .and_then(|v| v.as_array())
            .expect("userAgentTags should be a JSON array");
        let tags: Vec<&str> = tags.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(tags, vec!["app/AmazonQ-For-CLI"]);
    }
}
