use crate::theme::StyledText;
use crate::util::consts::env_var::{
    KIRO_CHAT_LOG_FILE,
    KIRO_TEST_TUI_JS_PATH,
};
use crate::util::env_var::is_log_stdout_enabled;
pub mod agent;
pub mod chat;
mod crew;
mod debug;
mod diagnostics;
pub mod experiment;
pub mod feed;
mod issue;
mod kas_acp_relay;
mod mcp;
mod settings;
pub mod update;
mod user;

use std::fmt::Display;
use std::io::{
    Write as _,
    stdout,
};
use std::path::Path;
use std::process::ExitCode;

pub use agent::Agent;
use agent::AgentArgs;
use anstream::println;
pub use chat::ConversationState;
pub use chat::tools::todo::TodoListState;
use clap::{
    ArgAction,
    CommandFactory,
    Parser,
    Subcommand,
    ValueEnum,
};
use eyre::{
    Context as _,
    Result,
    bail,
};
use feed::Feed;
use kiro_telemetry::metric::Engine;
use serde::Serialize;
use tracing::{
    Level,
    debug,
};

use crate::cli::chat::ChatArgs;
use crate::cli::mcp::McpSubcommand;
use crate::cli::user::{
    LoginArgs,
    WhoamiArgs,
    is_logged_in,
};
use crate::logging::{
    LogArgs,
    initialize_logging,
};
use crate::os::Os;
use crate::util::CLI_BINARY_NAME;
use crate::util::consts::env_var::KIRO_API_KEY;
use crate::util::paths::logs_dir;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Outputs the results as markdown
    #[default]
    Plain,
    /// Outputs the results as JSON
    Json,
    /// Outputs the results as pretty print JSON
    JsonPretty,
}

impl OutputFormat {
    pub fn print<T, TFn, J, JFn>(&self, text_fn: TFn, json_fn: JFn)
    where
        T: std::fmt::Display,
        TFn: FnOnce() -> T,
        J: Serialize,
        JFn: FnOnce() -> J,
    {
        match self {
            OutputFormat::Plain => println!("{}", text_fn()),
            OutputFormat::Json => println!("{}", serde_json::to_string(&json_fn()).unwrap()),
            OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(&json_fn()).unwrap()),
        }
    }
}

/// Output format for a `chat` run's response stream. Distinct from [`OutputFormat`], which
/// controls list-command output (`--list-models`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum RunOutputFormat {
    /// Human-readable text.
    #[default]
    Text,
    /// The run's ACP events as JSON Lines on stdout, one self-describing event per line.
    #[value(name = "stream-json")]
    StreamJson,
}

impl RunOutputFormat {
    pub fn is_structured(&self) -> bool {
        matches!(self, RunOutputFormat::StreamJson)
    }
}

/// Authentication owner for a v3 engine ACP session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AcpAuthMethod {
    /// Resolve access tokens for the v3 engine from the Kiro CLI credential store.
    Cli,
}

/// The Kiro CLI
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Subcommand)]
pub enum RootSubcommand {
    /// Manage agents
    Agent(AgentArgs),
    /// AI assistant in your terminal
    Chat(ChatArgs),
    /// Log in to Kiro
    Login(LoginArgs),
    /// Log out of Kiro
    Logout,
    /// Print info about the current login session
    Whoami(WhoamiArgs),
    /// Show the profile associated with this idc user
    Profile,
    /// Customize appearance & behavior
    #[command(alias("setting"))]
    Settings(settings::SettingsArgs),
    /// Run diagnostic tests
    #[command(alias("diagnostics"))]
    Diagnostic(diagnostics::DiagnosticArgs),
    /// Create a new Github issue
    Issue(issue::IssueArgs),
    /// Version
    #[command(hide = true)]
    Version {
        /// Show the changelog (use --changelog=all for all versions, or --changelog=x.x.x for a
        /// specific version)
        #[arg(long, num_args = 0..=1, default_missing_value = "")]
        changelog: Option<String>,
    },
    /// Launch Kiro Crew, installing it if it is not already installed
    Crew(crew::CrewArgs),
    /// Model Context Protocol (MCP)
    #[command(subcommand)]
    Mcp(McpSubcommand),
    /// Check for and install updates
    Update(update::UpdateArgs),
    /// Start Agent Client Protocol (ACP) agent
    #[command(hide = true)]
    Acp {
        /// Name of the agent to use when starting the first session
        #[arg(long)]
        agent: Option<String>,
        /// Model ID to use when starting the first session
        #[arg(long)]
        model: Option<String>,
        /// Initial effort level (e.g. low, medium, high, xhigh, max)
        #[arg(long)]
        effort: Option<String>,
        /// Auto-approve all tool permission requests
        #[arg(long, short = 'a')]
        trust_all_tools: bool,
        /// Trust only this set of tools
        #[arg(long, value_delimiter = ',', value_name = "TOOL_NAMES")]
        trust_tools: Option<Vec<String>>,
        /// Agent engine to use: "v1", "v2" (default), or "v3"
        #[arg(long, value_name = "ENGINE", default_value_t = chat::AgentEngine::V2)]
        agent_engine: chat::AgentEngine,
        /// Authentication owner for the v3 engine. Use "cli" to keep authentication inside this
        /// process.
        #[arg(long, visible_alias = "authMethod", value_name = "METHOD")]
        auth_method: Option<AcpAuthMethod>,
    },
    /// Start a persistent WebSocket server for the v3 engine
    Serve {
        /// Port to listen on
        #[arg(long, default_value = "8082")]
        port: u16,
    },
    /// ACP test client
    #[command(hide = true)]
    AcpClient {
        /// Path to the ACP agent executable
        #[arg(long)]
        agent: String,
    },
    /// Record voice and print transcription to stdout (used by TUI).
    #[cfg(feature = "voice")]
    #[command(hide = true)]
    Voice {
        /// Push-to-talk mode (disables silence auto-stop)
        #[arg(long)]
        ptt: bool,
        /// Proceed with downloading the speech model if it is not present.
        /// Without this flag, a missing model emits `status:needs_download`
        /// and exits so the caller (TUI) can ask the user to confirm first.
        #[arg(long)]
        confirm_download: bool,
    },
    /// Start a voice recording server for remote/cloud desktop use.
    #[cfg(feature = "voice")]
    #[command(hide = true)]
    VoiceServe {
        /// Port to listen on.
        #[arg(long, default_value = "19876")]
        port: u16,
        /// Address to bind to.
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
    },
    /// Set up voice mode for a cloud desktop (run locally).
    #[cfg(feature = "voice")]
    #[command(name = "voice-cloud-setup", hide = true)]
    VoiceCloudSetup {
        /// Cloud desktop hostname or SSH config alias
        host: String,
        /// Port for voice server
        #[arg(long, default_value = "19876")]
        port: u16,
        /// Path to kiro binary on cloud desktop
        #[arg(long)]
        remote_bin: Option<String>,
        /// SSH identity file
        #[arg(long, short = 'i')]
        identity: Option<String>,
    },
}

impl RootSubcommand {
    /// Whether the command should have an associated telemetry event.
    ///
    /// Emitting telemetry takes a long time so the answer is usually no.
    pub fn valid_for_telemetry(&self) -> bool {
        // Hidden `chat _ ...` internal subcommands are not user-initiated
        // chat; they are a tooling surface for tests and TUI IPC. Skip
        // telemetry for those calls.
        match self {
            Self::Chat(args) if args.command.is_none() => true,
            Self::Login(_) | Self::Profile | Self::Issue(_) => true,
            _ => false,
        }
    }

    pub async fn execute(self, os: &mut Os) -> Result<ExitCode> {
        // Check for auth on subcommands that require it.
        if !is_logged_in(&mut os.database).await && std::env::var("KIRO_TEST_MODE").is_err() {
            // Hidden `chat _ ...` internal subcommands run without auth so
            // tests can drive them in sandboxes that have no logged-in user.
            let is_internal_chat = matches!(self, Self::Chat(ref args) if args.command.is_some());
            // Use is_headless (not just no_interactive) so a logged-out stream-json run hits
            // this machine-readable error, not the interactive login prompt below.
            let is_headless_chat = matches!(self, Self::Chat(ref args) if args.is_headless());
            if is_headless_chat && !is_internal_chat {
                let message = format!(
                    "Not logged in. Set the {KIRO_API_KEY} environment variable or run `{CLI_BINARY_NAME} login` first."
                );
                // For a stream-json run, emit a terminal runError on stdout so the machine
                // consumer gets a record (auth fails before the ACP session launches, so there
                // is no sessionId yet); the human string still goes to stderr.
                if matches!(self, Self::Chat(ref args) if args.output_format.is_structured()) {
                    crate::launch::emit_stream_json_run_error(crate::launch::RunErrorStage::Auth, &message);
                }
                eprintln!("{message}");
                return Ok(ExitCode::FAILURE);
            } else if matches!(self, Self::Chat(_)) && !is_internal_chat {
                let options = ["Yes", "No"];
                match crate::util::choose(" You are not logged in. Login now?", &options)? {
                    Some(0) => {},
                    _ => bail!("Login is required to use chat"),
                }

                LoginArgs::default().execute(os).await?;
            } else if matches!(self, Self::Profile) {
                bail!(
                    "You are not logged in, please log in with {}",
                    StyledText::command(&format!("{CLI_BINARY_NAME} login"))
                );
            }
        }

        if !matches!(self, Self::Chat(_) | Self::Acp { .. }) {
            let telemetry_name = self.valid_for_telemetry().then(|| self.telemetry_name());
            crate::launch::emit_cli_invocation_telemetry(&os.telemetry, &os.database, telemetry_name, Engine::Unknown)
                .await;
        }

        // Auto-update: start background check (non-blocking, doesn't delay startup).
        // The actual install happens on exit via install_staged_update.
        //
        // Currently Windows-only. Mac/Linux auto-update is handled by the
        // autocomplete desktop app — we don't want two update systems active.
        // FUTURE: Re-enable for all platforms once the manifest follow-up lands.
        //
        // Skip auto-update when installed via Toolbox — Toolbox manages its own updates.
        #[cfg(target_os = "windows")]
        let (auto_install, update_handle) = {
            use crate::database::settings::Setting;
            use crate::telemetry::{
                InstallMethod,
                get_install_method,
            };

            let is_toolbox = matches!(get_install_method(), InstallMethod::Toolbox(_));
            if is_toolbox {
                tracing::debug!("Auto-update skipped: installed via Toolbox");
            }

            // app.disableAutoupdates: when true, auto-update is disabled.
            // Default is false (updates enabled), matching the autocomplete desktop app.
            let updates_disabled = os
                .database
                .settings
                .get_bool(Setting::DisableAutoupdates)
                .unwrap_or(false);

            let auto_install = !is_toolbox
                && !updates_disabled
                && std::env::var(crate::util::consts::env_var::KIRO_NO_AUTO_UPDATE).is_err();
            let handle = if !is_toolbox && matches!(self, Self::Chat(_) | Self::Acp { .. }) {
                Some(update::start_background_update_check(auto_install))
            } else {
                None
            };
            (auto_install, handle)
        };
        #[cfg(not(target_os = "windows"))]
        let (_auto_install, _update_handle): (bool, Option<()>) = (false, None);

        // On Windows, install staged update on exit (if one was downloaded in the background).
        // This spawns a detached installer that waits for this process to fully exit.
        #[cfg(target_os = "windows")]
        {
            let result = match self {
                Self::Agent(args) => args.execute(os).await,
                Self::Diagnostic(args) => args.execute(os).await,
                Self::Login(args) => args.execute(os).await,
                Self::Logout => user::logout(os).await,
                Self::Whoami(args) => args.execute(os).await,
                Self::Profile => user::profile(os).await,
                Self::Settings(settings_args) => settings_args.execute(os).await,
                Self::Crew(args) => args.execute(os).await,
                Self::Issue(args) => args.execute(os).await,
                Self::Version { changelog } => Cli::print_version(changelog).await,
                Self::Chat(args) => execute_chat(args, os).await,
                Self::Mcp(args) => args.execute(os, &mut std::io::stderr()).await,
                Self::Update(args) => args.execute(os).await,
                Self::Acp {
                    agent,
                    model,
                    effort,
                    trust_all_tools,
                    trust_tools,
                    agent_engine,
                    auth_method,
                } => {
                    reject_acp_auth_method_for_non_v3(agent_engine, auth_method);
                    if agent_engine == chat::AgentEngine::Kas {
                        reject_unsupported_v3_acp_flags(
                            agent.as_deref(),
                            model.as_deref(),
                            effort.as_deref(),
                            trust_all_tools,
                            trust_tools.as_deref(),
                        );
                        return execute_kas_acp(os, auth_method).await;
                    }
                    use std::sync::Arc;

                    use chat_cli_v2::os::Os as NewOs;
                    let legacy_session_exporter: Arc<
                        dyn chat_cli_v2::agent::session::legacy_compat::LegacySessionExporter,
                    > = Arc::new(crate::cli::chat::v1_export::LegacySessionExporterImpl::new(Arc::new(
                        os.database.clone(),
                    )));
                    let mut os = NewOs::new().await?;
                    let spawn_args = ::agent::types::AcpSpawnArgs {
                        agent,
                        model,
                        trust_all_tools,
                        trust_tools,
                        agent_engine: None,
                        effort,
                    };
                    chat_cli_v2::agent::acp::acp_agent::execute(&mut os, spawn_args, legacy_session_exporter).await
                },
                Self::Serve { port } => execute_kas_serve(os, port).await,
                Self::AcpClient { agent } => chat_cli_v2::agent::acp::acp_client::execute(agent).await,
                #[cfg(feature = "voice")]
                Self::Voice { ptt, confirm_download } => {
                    use crate::database::settings::Setting;
                    let server_url = os.database.settings.get_string(Setting::VoiceServerUrl);
                    let backend = if server_url.is_some() {
                        "RemoteServer".to_string()
                    } else {
                        "LocalWhisper".to_string()
                    };
                    let silence_timeout = if ptt {
                        None
                    } else {
                        Some(
                            os.database
                                .settings
                                .get_int(Setting::VoiceSilenceTimeout)
                                .and_then(|v| v.try_into().ok())
                                .unwrap_or(5u64),
                        )
                    };
                    let language = os.database.settings.get_string(Setting::VoiceLanguage);
                    let model_size = os.database.settings.get_string(Setting::VoiceModelSize);
                    let result = voice::voice_handler::voice_only_mode(
                        server_url,
                        silence_timeout,
                        language,
                        model_size,
                        confirm_download,
                    )
                    .await;
                    let (telem_result, reason, reason_desc) = match &result {
                        Ok(_) => (crate::telemetry::TelemetryResult::Succeeded, None, None),
                        Err(e) => (
                            crate::telemetry::TelemetryResult::Failed,
                            Some("VoiceError".to_string()),
                            Some(e.to_string()),
                        ),
                    };
                    let input_method = if ptt { "PTT" } else { "SlashCommand" };
                    os.telemetry
                        .send_voice_input(
                            None,
                            telem_result,
                            reason,
                            reason_desc,
                            backend,
                            input_method.to_string(),
                            None,
                            None,
                            None,
                            None,
                            None,
                        )
                        .ok();
                    result
                },
                #[cfg(feature = "voice")]
                Self::VoiceServe { port, bind } => voice::voice_serve::run_voice_server(&bind, port).await,
                #[cfg(feature = "voice")]
                Self::VoiceCloudSetup {
                    host,
                    port,
                    remote_bin,
                    identity,
                } => {
                    voice::voice_cloud_setup::run_voice_cloud_setup(
                        &host,
                        port,
                        remote_bin.as_deref(),
                        identity.as_deref(),
                    )
                    .await
                },
            };

            if let Some(handle) = update_handle {
                update::install_staged_update(handle, auto_install).await;
            }

            result
        }

        #[cfg(not(target_os = "windows"))]
        match self {
            Self::Agent(args) => args.execute(os).await,
            Self::Diagnostic(args) => args.execute(os).await,
            Self::Login(args) => args.execute(os).await,
            Self::Logout => user::logout(os).await,
            Self::Whoami(args) => args.execute(os).await,
            Self::Profile => user::profile(os).await,
            Self::Settings(settings_args) => settings_args.execute(os).await,
            Self::Crew(args) => args.execute(os).await,
            Self::Issue(args) => args.execute(os).await,
            Self::Version { changelog } => Cli::print_version(changelog).await,
            Self::Chat(args) => execute_chat(args, os).await,
            Self::Mcp(args) => args.execute(os, &mut std::io::stderr()).await,
            Self::Update(args) => args.execute(os).await,
            Self::Acp {
                agent,
                model,
                effort,
                trust_all_tools,
                trust_tools,
                agent_engine,
                auth_method,
            } => {
                reject_acp_auth_method_for_non_v3(agent_engine, auth_method);
                if agent_engine == chat::AgentEngine::Kas {
                    reject_unsupported_v3_acp_flags(
                        agent.as_deref(),
                        model.as_deref(),
                        effort.as_deref(),
                        trust_all_tools,
                        trust_tools.as_deref(),
                    );
                    return execute_kas_acp(os, auth_method).await;
                }
                use std::sync::Arc;

                use chat_cli_v2::os::Os as NewOs;
                let legacy_session_exporter: Arc<
                    dyn chat_cli_v2::agent::session::legacy_compat::LegacySessionExporter,
                > = Arc::new(crate::cli::chat::v1_export::LegacySessionExporterImpl::new(Arc::new(
                    os.database.clone(),
                )));
                let mut os = NewOs::new().await?;
                if let Err(e) = os.client.resolve_profile_if_missing(&mut os.database).await {
                    tracing::warn!("Failed to resolve profile: {e}");
                }
                let spawn_args = ::agent::types::AcpSpawnArgs {
                    agent,
                    model,
                    trust_all_tools,
                    trust_tools,
                    agent_engine: None,
                    effort,
                };
                chat_cli_v2::agent::acp::acp_agent::execute(&mut os, spawn_args, legacy_session_exporter).await
            },
            Self::Serve { port } => execute_kas_serve(os, port).await,
            Self::AcpClient { agent } => chat_cli_v2::agent::acp::acp_client::execute(agent).await,
            #[cfg(feature = "voice")]
            Self::Voice { ptt, confirm_download } => {
                use crate::database::settings::Setting;
                let server_url = os.database.settings.get_string(Setting::VoiceServerUrl);
                let backend = if server_url.is_some() {
                    "RemoteServer".to_string()
                } else {
                    "LocalWhisper".to_string()
                };
                let silence_timeout = if ptt {
                    None
                } else {
                    Some(
                        os.database
                            .settings
                            .get_int(Setting::VoiceSilenceTimeout)
                            .and_then(|v| v.try_into().ok())
                            .unwrap_or(5u64),
                    )
                };
                let language = os.database.settings.get_string(Setting::VoiceLanguage);
                let model_size = os.database.settings.get_string(Setting::VoiceModelSize);
                let result = voice::voice_handler::voice_only_mode(
                    server_url,
                    silence_timeout,
                    language,
                    model_size,
                    confirm_download,
                )
                .await;
                let (telem_result, reason, reason_desc) = match &result {
                    Ok(_) => (crate::telemetry::TelemetryResult::Succeeded, None, None),
                    Err(e) => (
                        crate::telemetry::TelemetryResult::Failed,
                        Some("VoiceError".to_string()),
                        Some(e.to_string()),
                    ),
                };
                let input_method = if ptt { "PTT" } else { "SlashCommand" };
                os.telemetry
                    .send_voice_input(
                        None,
                        telem_result,
                        reason,
                        reason_desc,
                        backend,
                        input_method.to_string(),
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                    .ok();
                result
            },
            #[cfg(feature = "voice")]
            Self::VoiceServe { port, bind } => voice::voice_serve::run_voice_server(&bind, port).await,
            #[cfg(feature = "voice")]
            Self::VoiceCloudSetup {
                host,
                port,
                remote_bin,
                identity,
            } => {
                voice::voice_cloud_setup::run_voice_cloud_setup(&host, port, remote_bin.as_deref(), identity.as_deref())
                    .await
            },
        }
    }
}

fn chat_telemetry_name() -> String {
    if std::env::args().any(|arg| arg == "--list") {
        "chat:list".to_string()
    } else {
        "chat".to_string()
    }
}

async fn execute_chat(mut args: ChatArgs, os: &mut Os) -> Result<ExitCode> {
    // stream-json implies non-interactive; force it before engine/interactivity resolution.
    if args.output_format.is_structured() {
        args.no_interactive = true;
    }

    if let Some(err) =
        args.remote_sandbox_gate_error(crate::rollout::rollout().is_enabled(crate::rollout::Feature::RemoteSandbox))
    {
        err.exit();
    }
    args.validate_sessions_mode()?;

    if let Some(command) = args.command.take() {
        return command.execute().await;
    }

    let telemetry_name = chat_telemetry_name();
    if args.list_models {
        crate::launch::emit_cli_invocation_telemetry(
            &os.telemetry,
            &os.database,
            Some(telemetry_name),
            Engine::Unknown,
        )
        .await;
        return crate::cli::chat::cli::model::print_model_list(os, args.format)
            .await
            .map_err(Into::into);
    }

    if let Some(result) = handle_session_flags(&args, os).await {
        let engine = match args.session_source {
            Some(chat::SessionSourceArg::V1) => Engine::V1,
            Some(chat::SessionSourceArg::V2) => Engine::V2,
            Some(chat::SessionSourceArg::V3) => Engine::V3,
            None => Engine::Unknown,
        };
        crate::launch::emit_cli_invocation_telemetry(&os.telemetry, &os.database, Some(telemetry_name), engine).await;
        return result;
    }

    #[cfg(not(target_os = "windows"))]
    if let Err(err) = os.client.resolve_profile_if_missing(&mut os.database).await {
        tracing::warn!("Failed to resolve profile: {err}");
    }

    let cleanup_notice = tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        eprintln!("Cleaning up old conversations...");
    });
    if let Err(err) = crate::cleanup::cleanup_old_data(&os.env, &os.fs, &os.database).await {
        tracing::error!("Cleanup failed: {err}");
    }
    cleanup_notice.abort();

    let tui_available = crate::embedded_tui::are_assets_embedded(os) || std::env::var(KIRO_TEST_TUI_JS_PATH).is_ok();
    let engine = match args.resolve_agent_engine(os) {
        Ok(engine) => engine,
        Err(err) => {
            // For a stream-json run, engine resolution fails before the ACP session launches
            // (most commonly the v1-rejection), so emit a terminal runError on stdout; without
            // it the consumer reads a clean EOF and cannot tell refusal from empty output.
            if args.output_format.is_structured() {
                crate::launch::emit_stream_json_run_error(crate::launch::RunErrorStage::Engine, &err.to_string());
            }
            crate::launch::emit_cli_invocation_telemetry(
                &os.telemetry,
                &os.database,
                Some(telemetry_name),
                Engine::Unknown,
            )
            .await;
            return Err(err);
        },
    };
    #[cfg(target_os = "windows")]
    let is_tui_supported = true;
    #[cfg(not(target_os = "windows"))]
    let is_tui_supported = crate::util::system_info::is_tui_supported();

    tracing::debug!(?engine, is_tui_supported, tui_available, "launch decision");
    if args.sessions
        && let Some(reason) = session_dashboard_unavailable()
    {
        // `--sessions` forces the KAS (v3) engine in `resolve_agent_engine`
        // (non-Kas already errored there), so the engine is always V3 here.
        crate::launch::emit_cli_invocation_telemetry(
            &os.telemetry,
            &os.database,
            Some(telemetry_name.clone()),
            Engine::V3,
        )
        .await;
        eprintln!("{reason}");
        return Ok(ExitCode::SUCCESS);
    }
    ensure_dashboard_tui_available(&args, is_tui_supported, tui_available)?;
    let fallback_to_v1 = !args.no_interactive && (!is_tui_supported || !tui_available);
    match engine {
        chat::AgentEngine::V1 => crate::launch::launch_v1(args, os, telemetry_name).await,
        chat::AgentEngine::V2 | chat::AgentEngine::Kas if fallback_to_v1 => {
            if !tui_available {
                tracing::error!("TUI assets not available, falling back to legacy UI");
            }
            if !is_tui_supported {
                eprintln!("The TUI is currently not supported for the current platform");
            }
            crate::launch::launch_v1(args, os, telemetry_name).await
        },
        chat::AgentEngine::V2 | chat::AgentEngine::Kas => {
            launch_acp_session(os, &mut args, engine, telemetry_name).await
        },
    }
}

/// A friendly, non-error reason the `--sessions` dashboard can't launch, or
/// `None` when it may proceed. The dashboard is a nightly-gated preview, so
/// this is purely the rollout gate — the engine is already forced to KAS (v3)
/// by `resolve_agent_engine` (a non-Kas `--sessions` errors there), so no
/// engine check is needed or reachable here.
fn session_dashboard_unavailable() -> Option<String> {
    if !crate::rollout::rollout().is_enabled(crate::rollout::Feature::SessionDashboard) {
        return Some(
            "The session dashboard is a preview that's currently only available in nightly builds.".to_string(),
        );
    }
    None
}

fn ensure_dashboard_tui_available(args: &ChatArgs, is_tui_supported: bool, tui_available: bool) -> Result<()> {
    if !args.sessions {
        return Ok(());
    }
    if !tui_available {
        bail!("--sessions requires the TUI, but TUI assets are not available in this build");
    }
    if !is_tui_supported {
        bail!("--sessions requires an interactive TUI supported by the current platform");
    }
    Ok(())
}

fn ensure_dashboard_interactive(args: &ChatArgs, non_interactive: bool) -> Result<()> {
    if args.sessions && non_interactive {
        bail!("--sessions requires an interactive terminal and cannot run with --no-interactive or piped stdin");
    }
    Ok(())
}

/// Build [`LaunchOptions`] and launch the ACP session.
/// When `--no-interactive` is set, resolves the prompt input from CLI args or
/// stdin and selects the non-interactive variant; otherwise runs the
/// interactive TUI.
async fn launch_acp_session(
    os: &Os,
    args: &mut ChatArgs,
    agent_engine: chat::AgentEngine,
    telemetry_name: String,
) -> Result<ExitCode> {
    let mode = args.mode;
    // Render headless when the session is non-interactive: explicit `--no-interactive`,
    // or stdin that isn't interactive. Avoids rendering the TUI on a pipe.
    let non_interactive = args.no_interactive || !crate::util::stdin_is_interactive();
    ensure_dashboard_interactive(args, non_interactive)?;
    // The non-interactive path never sends an execution target or the
    // remote-sessions endpoint, so `--cloud` there would silently run a local
    // session. Reject it until that path supports cloud.
    if non_interactive && (args.cloud || args.repo.is_some()) {
        let message = "--cloud/--repo are not supported in non-interactive mode yet; run without --no-interactive and with an interactive stdin";
        // Terminal record on the stream-json path so the consumer sees a refusal, not a bare
        // EOF; this is the only machine-readable record for this refusal.
        if args.output_format.is_structured() {
            crate::launch::emit_stream_json_run_error(crate::launch::RunErrorStage::Cloud, message);
        }
        bail!("{message}");
    }
    let options = if non_interactive {
        let input = match args.resolve_non_interactive_input() {
            Ok(input) => input,
            Err(err) => {
                if args.output_format.is_structured() {
                    crate::launch::emit_stream_json_run_error(crate::launch::RunErrorStage::Input, &err.to_string());
                }
                return Err(err);
            },
        };
        crate::launch::LaunchOptions::non_interactive(
            agent_engine,
            mode,
            input,
            args.output_format.is_structured(),
            args.trust_all_tools,
            args.agent.clone(),
            args.model.clone(),
            args.trust_tools.clone(),
        )
    } else {
        crate::launch::LaunchOptions::interactive(agent_engine, mode)
    };
    crate::launch::launch(options, os, telemetry_name).await
}

fn is_acp_auth_method_supported(agent_engine: chat::AgentEngine, auth_method: Option<AcpAuthMethod>) -> bool {
    auth_method.is_none() || agent_engine == chat::AgentEngine::Kas
}

fn reject_acp_auth_method_for_non_v3(agent_engine: chat::AgentEngine, auth_method: Option<AcpAuthMethod>) {
    if is_acp_auth_method_supported(agent_engine, auth_method) {
        return;
    }
    let mut command = Cli::command();
    match command.find_subcommand_mut("acp") {
        Some(acp) => acp.error(
            clap::error::ErrorKind::ArgumentConflict,
            "--auth-method is only supported with --agent-engine=v3",
        ),
        None => command.error(
            clap::error::ErrorKind::ArgumentConflict,
            "--auth-method is only supported with --agent-engine=v3",
        ),
    }
    .exit();
}

/// Names of `acp` flags that are inert on the v3 engine, in declaration order.
fn unsupported_v3_acp_flags(
    agent: Option<&str>,
    model: Option<&str>,
    effort: Option<&str>,
    trust_all_tools: bool,
    trust_tools: Option<&[String]>,
) -> Vec<&'static str> {
    let mut flags = Vec::new();
    if agent.is_some() {
        flags.push("--agent");
    }
    if model.is_some() {
        flags.push("--model");
    }
    if effort.is_some() {
        flags.push("--effort");
    }
    if trust_all_tools {
        flags.push("--trust-all-tools");
    }
    if trust_tools.is_some() {
        flags.push("--trust-tools");
    }
    flags
}

/// Exit with a clap argument error if any `acp` flag unsupported on v3 was
/// passed. No-op when none are set.
fn reject_unsupported_v3_acp_flags(
    agent: Option<&str>,
    model: Option<&str>,
    effort: Option<&str>,
    trust_all_tools: bool,
    trust_tools: Option<&[String]>,
) {
    let unsupported = unsupported_v3_acp_flags(agent, model, effort, trust_all_tools, trust_tools);
    if unsupported.is_empty() {
        return;
    }
    let msg = format!(
        "the following arguments are not supported with --agent-engine=v3: {}",
        unsupported.join(", ")
    );
    let mut command = Cli::command();
    match command.find_subcommand_mut("acp") {
        Some(acp) => acp.error(clap::error::ErrorKind::ArgumentConflict, msg),
        None => command.error(clap::error::ErrorKind::ArgumentConflict, msg),
    }
    .exit();
}

/// Spawn the KAS TypeScript agent as an ACP server over stdio.
/// Extracts embedded node + KAS assets if needed, then execs
/// `node --experimental-wasm-modules acp-server.js --transport=stdio
///  --auth=acp-callback`.
async fn execute_kas_acp(os: &Os, auth_method: Option<AcpAuthMethod>) -> Result<ExitCode> {
    match auth_method {
        None => {
            let mut child = spawn_kas_process(os, KasStdio::Inherit).await?;
            let status = child.wait().await?;
            Ok(status.code().map_or(ExitCode::FAILURE, |c| ExitCode::from(c as u8)))
        },
        Some(AcpAuthMethod::Cli) => {
            let mut signals = kas_acp_relay::ShutdownSignals::new()?;
            let relay_io = kas_acp_relay::RelayIo::new()?;
            let child = tokio::select! {
                child = spawn_kas_process(os, KasStdio::Proxied) => child?,
                exit_code = signals.recv() => return Ok(exit_code),
            };
            kas_acp_relay::run(child, signals, relay_io).await
        },
    }
}

/// Stdio configuration for a spawned KAS process.
#[derive(Debug, Clone, Copy)]
pub(crate) enum KasStdio {
    /// Inherit parent stdio. Used for passthrough `kiro-cli acp`.
    Inherit,
    /// Pipe stdin/stdout (for ACP client use), null stderr.
    Piped,
    /// Pipe stdin/stdout through the CLI auth relay and inherit stderr.
    Proxied,
}

/// Spawn a KAS process with `--transport=stdio`.
///
/// Resolves the node binary and server script from `KIRO_KAS_SERVER_PATH` or
/// embedded assets. KAS is launched in `--auth=acp-callback` mode: it makes
/// the ACP client (the parent of this process) responsible for fielding
/// `_kiro/auth/getAccessToken` whenever KAS needs an access token.
///
/// Read the @kiro/agent package version from its package.json relative to acp-server.js path.
fn read_kas_version(server_js: &Path) -> String {
    server_js
        .parent()  // dist/server/
        .and_then(|p| p.parent())  // dist/
        .and_then(|p| p.parent())  // @kiro/agent/
        .map(|p| p.join("package.json"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v["version"].as_str().map(String::from))
        .unwrap_or_else(|| "unknown".into())
}

/// Whether the user has opted in to content collection for service improvement.
/// Sourced from the same `ShareCodeWhispererContent` setting the V1/V2
/// OptOutInterceptor uses (default: opted in). Passed to KAS via
/// `KIRO_CONTENT_COLLECTION_ENABLED`; without it KAS defaults to opted out and
/// stamps `x-amzn-codewhisperer-optout` on every request, suppressing
/// DataHub/KCO conversation storage for v3.
pub(crate) fn content_collection_enabled(os: &Os) -> bool {
    os.database
        .settings
        .get_bool(crate::database::settings::Setting::ShareCodeWhispererContent)
        .unwrap_or(true)
}

/// Internal chat-cli ACP-client paths handle the callback via
/// `chat_cli_v2::auth::kas_token::handle_ext_method`. External
/// clients connecting to a `KasStdio::Inherit` spawn (e.g. `kiro-cli acp`)
/// MUST implement the same callback themselves. `KasStdio::Proxied` instead
/// routes that callback through the CLI-owned auth relay.
pub(crate) async fn spawn_kas_process(os: &Os, stdio: KasStdio) -> Result<tokio::process::Child> {
    if !crate::util::platform::can_run_kas() {
        bail!("The v3 engine is currently not supported on this system.");
    }

    let (node_bin, server_js) = crate::embedded_tui::ensure_kas_assets(os, false).await?;
    let node_bin = node_bin.ok_or_else(|| {
        eyre::eyre!("Cannot resolve the node binary for the v3 engine: KIRO_KAS_NODE_PATH is not set and the embedded node binary is unavailable")
    })?;
    let server_js = server_js.ok_or_else(|| {
        eyre::eyre!("Cannot resolve the server for the v3 engine: KIRO_KAS_SERVER_PATH is not set and the embedded server is unavailable")
    })?;

    debug!(
        node = %node_bin.display(),
        server = %server_js.display(),
        ?stdio,
        "spawning KAS process"
    );

    let mut cmd = tokio::process::Command::new(&node_bin);
    cmd.arg("--experimental-wasm-modules")
        .arg(&server_js)
        .arg("--transport=stdio")
        // Auth: KAS calls back to this process's ACP client for tokens. See
        // function-level docs above.
        .arg("--auth=acp-callback")
        .kill_on_drop(true);

    match stdio {
        KasStdio::Piped => {
            cmd.stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                // stderr is null'd in Piped mode so KAS chatter doesn't
                // leak into one-shot output. To debug startup failures,
                // run `kiro-cli acp --agent-engine=kas` (Inherit mode).
                .stderr(std::process::Stdio::null());
        },
        KasStdio::Inherit => {
            cmd.stdin(std::process::Stdio::inherit())
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit());
        },
        KasStdio::Proxied => {
            cmd.stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::inherit());
            kas_acp_relay::configure_proxied_command(&mut cmd);
        },
    }

    let kas_version = read_kas_version(&server_js);

    cmd.env(
        "KIRO_CUSTOM_USER_AGENT",
        format!(
            "KiroCLI/{} KAS/{} os/{} md/appVersion-{} app/AmazonQ-For-CLI",
            env!("CARGO_PKG_VERSION"),
            kas_version,
            std::env::consts::OS,
            env!("CARGO_PKG_VERSION"),
        ),
    );
    cmd.env(
        crate::util::consts::env_var::KIRO_CONTENT_COLLECTION_ENABLED,
        content_collection_enabled(os).to_string(),
    );

    let child = cmd.spawn().with_context(|| {
        format!(
            "failed to spawn the v3 engine: node binary `{}` (server: {})",
            node_bin.display(),
            server_js.display()
        )
    })?;

    Ok(child)
}

/// Spawn KAS as a persistent WebSocket server on the given port.
async fn execute_kas_serve(os: &Os, port: u16) -> Result<ExitCode> {
    let (node_bin, server_js) = crate::embedded_tui::ensure_kas_assets(os, false).await?;
    let node_bin = node_bin.ok_or_else(|| {
        eyre::eyre!("Cannot resolve the node binary for the v3 engine: KIRO_KAS_NODE_PATH is not set and the embedded node binary is unavailable")
    })?;
    let server_js = server_js.ok_or_else(|| {
        eyre::eyre!("Cannot resolve the server for the v3 engine: KIRO_KAS_SERVER_PATH is not set and the embedded server is unavailable")
    })?;

    debug!(
        "Spawning KAS serve: {} --experimental-wasm-modules {} --transport=ws --auth=acp-callback (port {})",
        node_bin.display(),
        server_js.display(),
        port,
    );

    eprintln!("Kiro agent server running on port {}", port);
    eprintln!("Connect with: kiro-cli --remote ws://<this-host>:{}", port);

    let kas_version = read_kas_version(&server_js);

    let mut child = tokio::process::Command::new(&node_bin)
        .arg("--experimental-wasm-modules")
        .arg(&server_js)
        .arg("--transport=ws")
        .arg("--auth=acp-callback")
        .env("ACP_WS_PORT", port.to_string())
        .env(
            "KIRO_CUSTOM_USER_AGENT",
            format!(
                "KiroCLI/{} KAS/{} os/{} md/appVersion-{} app/AmazonQ-For-CLI",
                env!("CARGO_PKG_VERSION"),
                kas_version,
                std::env::consts::OS,
                env!("CARGO_PKG_VERSION"),
            ),
        )
        .env(
            crate::util::consts::env_var::KIRO_CONTENT_COLLECTION_ENABLED,
            content_collection_enabled(os).to_string(),
        )
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(false)
        .spawn()
        .with_context(|| {
            format!(
                "failed to spawn the v3 engine server: {} {}",
                node_bin.display(),
                server_js.display()
            )
        })?;

    let status = child.wait().await?;
    Ok(status.code().map_or(ExitCode::FAILURE, |c| ExitCode::from(c as u8)))
}

/// Handle `--list-sessions` and `--delete-session` before TUI launch.
///
/// Returns `Some(Result)` if a flag was handled, `None` to continue normal dispatch.
async fn handle_session_flags(args: &ChatArgs, os: &Os) -> Option<Result<ExitCode>> {
    use crate::cli::chat::SessionSourceArg;
    use crate::cli::chat::cli::persist::SessionSource;

    if !args.list_sessions && args.delete_session.is_none() {
        return None;
    }
    crate::cli::chat::cli::persist::handle_list_delete_session_flags(
        args.list_sessions,
        args.all_cwds,
        args.delete_session.as_deref(),
        args.session_source.map(|s| match s {
            SessionSourceArg::V1 => SessionSource::V1,
            SessionSourceArg::V2 => SessionSource::V2,
            SessionSourceArg::V3 => SessionSource::Kas,
        }),
        // Gates both the cloud-session listing (rows tagged non-local are
        // hidden while off) and cloud-aware delete (bare `--delete-session <id>`
        // removes a cloud session, prefix resolution consults the cloud store).
        // Off (rollout 0%) on released builds → local stores only.
        crate::rollout::rollout().is_enabled(crate::rollout::Feature::RemoteSandbox),
        args.format,
        os,
    )
    .await
    .map(Ok)
}

impl Default for RootSubcommand {
    fn default() -> Self {
        Self::Chat(ChatArgs::default())
    }
}

impl Display for RootSubcommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Agent(_) => "agent",
            Self::Chat(_) => "chat",
            Self::Login(_) => "login",
            Self::Logout => "logout",
            Self::Whoami(_) => "whoami",
            Self::Profile => "profile",
            Self::Settings(_) => "settings",
            Self::Crew(_) => "crew",
            Self::Diagnostic(_) => "diagnostic",
            Self::Issue(_) => "issue",
            Self::Version { .. } => "version",
            Self::Mcp(_) => "mcp",
            Self::Update(_) => "update",
            Self::Acp { .. } => "acp",
            Self::Serve { .. } => "serve",
            Self::AcpClient { .. } => "acp-client",
            #[cfg(feature = "voice")]
            Self::Voice { .. } => "voice",
            #[cfg(feature = "voice")]
            Self::VoiceServe { .. } => "voice-serve",
            #[cfg(feature = "voice")]
            Self::VoiceCloudSetup { .. } => "voice-cloud-setup",
        };

        write!(f, "{name}")
    }
}

impl RootSubcommand {
    /// Subcommand name as recorded in `CliSubcommandExecuted` telemetry.
    ///
    /// Identical to `Display` except that root-level `--list` (alias for
    /// `--resume-picker`) is recorded as `chat:list` so we can track adoption
    /// of the alias separately from `--resume-picker`.
    pub fn telemetry_name(&self) -> String {
        if matches!(self, Self::Chat(_)) {
            return chat_telemetry_name();
        }
        self.to_string()
    }
}

#[derive(Debug, Parser, PartialEq, Default)]
#[command(version, about, name = crate::util::CHAT_BINARY_NAME)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommand: Option<RootSubcommand>,
    /// Increase logging verbosity
    #[arg(long, short = 'v', action = ArgAction::Count, global = true)]
    pub verbose: u8,
    /// Launch chat in TUI mode
    #[arg(long)]
    tui: bool,
    /// Launch chat in legacy UI mode
    #[arg(long, visible_alias = "classic")]
    legacy_ui: bool,
    /// Launch the next generation Kiro agent
    #[arg(long, conflicts_with = "legacy_ui")]
    v3: bool,
    /// Resume the most recent conversation from this directory
    #[arg(short, long)]
    resume: bool,
    /// Resume a specific conversation by session ID
    #[arg(long, value_name = "SESSION_ID", conflicts_with_all = ["resume", "resume_picker"])]
    resume_id: Option<String>,
    /// Interactively select a conversation to resume from this directory
    #[arg(long, conflicts_with = "resume", visible_alias = "list")]
    resume_picker: bool,
}

impl Cli {
    pub async fn execute(self) -> Result<ExitCode> {
        let subcommand = self.subcommand.unwrap_or_else(|| {
            RootSubcommand::Chat(ChatArgs {
                tui: self.tui,
                legacy_ui: self.legacy_ui,
                v3: self.v3,
                resume: self.resume,
                resume_id: self.resume_id,
                resume_picker: self.resume_picker,
                ..Default::default()
            })
        });

        // Initialize our logger and keep around the guard so logging can perform as expected.
        let _log_guard = initialize_logging(LogArgs {
            log_level: match self.verbose > 0 {
                true => Some(
                    match self.verbose {
                        1 => Level::WARN,
                        2 => Level::INFO,
                        3 => Level::DEBUG,
                        _ => Level::TRACE,
                    }
                    .to_string(),
                ),
                false => None,
            },
            log_to_stdout: is_log_stdout_enabled() || self.verbose > 0,
            log_file_path: std::env::var(KIRO_CHAT_LOG_FILE)
                .ok()
                .map(std::path::PathBuf::from)
                .or_else(|| match subcommand {
                    RootSubcommand::Chat { .. } | RootSubcommand::Acp { .. } => {
                        Some(logs_dir().expect("logs dir must be set").join("kiro-chat.log"))
                    },
                    _ => None,
                }),
            delete_old_log_file: false,
        });

        debug!(command =? std::env::args().collect::<Vec<_>>(), "Command being ran");

        let mut os = Os::new().await?;
        let result = subcommand.execute(&mut os).await;

        let telemetry_result = os.telemetry.finish().await;
        let exit_code = result?;
        telemetry_result?;

        Ok(exit_code)
    }

    fn print_changelog_entry(entry: &feed::Entry) -> Result<()> {
        println!("Version {} ({})", entry.version, entry.date);

        if entry.changes.is_empty() {
            println!("  No changes recorded for this version.");
        } else {
            for change in &entry.changes {
                let type_label = match change.change_type.as_str() {
                    "added" => "Added",
                    "fixed" => "Fixed",
                    "changed" => "Changed",
                    other => other,
                };

                println!("  - {}: {}", type_label, change.description);
            }
        }

        println!();
        Ok(())
    }

    async fn print_version(changelog: Option<String>) -> Result<ExitCode> {
        // If no changelog is requested, display normal version information
        if changelog.is_none() {
            let _ = writeln!(stdout(), "{}", Self::command().render_version());
            return Ok(ExitCode::SUCCESS);
        }

        let changelog_value = changelog.unwrap_or_default();
        let feed = Feed::load_remote().await;

        // Display changelog for all versions
        if changelog_value == "all" {
            let entries = feed.get_all_changelogs();
            if entries.is_empty() {
                println!("No changelog information available.");
            } else {
                println!("Changelog for all versions:");
                for entry in entries {
                    Self::print_changelog_entry(&entry)?;
                }
            }
            return Ok(ExitCode::SUCCESS);
        }

        // Display changelog for a specific version (--changelog=x.x.x)
        if !changelog_value.is_empty() {
            match feed.get_version_changelog(&changelog_value) {
                Some(entry) => {
                    println!("Changelog for version {}:", changelog_value);
                    Self::print_changelog_entry(&entry)?;
                    return Ok(ExitCode::SUCCESS);
                },
                None => {
                    println!("No changelog information available for version {}.", changelog_value);
                    return Ok(ExitCode::SUCCESS);
                },
            }
        }

        // Display changelog for the current version (--changelog only)
        let current_version = env!("CARGO_PKG_VERSION");
        match feed.get_version_changelog(current_version) {
            Some(entry) => {
                println!("Changelog for version {}:", current_version);
                Self::print_changelog_entry(&entry)?;
            },
            None => {
                println!("No changelog information available for version {}.", current_version);
            },
        }

        Ok(ExitCode::SUCCESS)
    }
}

#[cfg(test)]
mod test {
    use chat::WrapMode::{
        Always,
        Auto,
        Never,
    };

    use super::*;
    use crate::util::CHAT_BINARY_NAME;
    use crate::util::test::assert_parse;

    #[test]
    fn debug_assert() {
        Cli::command().debug_assert();
    }

    fn parse_acp_auth_method(args: &[&str]) -> Option<AcpAuthMethod> {
        let args = std::iter::once(CHAT_BINARY_NAME)
            .chain(std::iter::once("acp"))
            .chain(args.iter().copied());
        match Cli::try_parse_from(args).expect("parse ACP arguments").subcommand {
            Some(RootSubcommand::Acp { auth_method, .. }) => auth_method,
            subcommand => panic!("expected ACP subcommand, got {subcommand:?}"),
        }
    }

    #[test]
    fn acp_auth_method_defaults_to_client_owned() {
        assert_eq!(parse_acp_auth_method(&["--agent-engine=v3"]), None);
    }

    #[test]
    fn acp_auth_method_cli_parses_canonical_flag() {
        assert_eq!(
            parse_acp_auth_method(&["--agent-engine=v3", "--auth-method=cli"]),
            Some(AcpAuthMethod::Cli)
        );
    }

    #[test]
    fn acp_auth_method_cli_parses_camel_case_alias() {
        assert_eq!(
            parse_acp_auth_method(&["--agent-engine=v3", "--authMethod=cli"]),
            Some(AcpAuthMethod::Cli)
        );
    }

    #[test]
    fn acp_auth_method_rejects_unknown_value() {
        let error = Cli::try_parse_from([CHAT_BINARY_NAME, "acp", "--agent-engine=v3", "--auth-method=client"])
            .expect_err("unknown auth method must fail parsing");
        assert_eq!(error.kind(), clap::error::ErrorKind::InvalidValue);
    }

    #[test]
    fn acp_auth_method_cli_requires_v3() {
        assert!(is_acp_auth_method_supported(
            chat::AgentEngine::Kas,
            Some(AcpAuthMethod::Cli)
        ));
        assert!(!is_acp_auth_method_supported(
            chat::AgentEngine::V2,
            Some(AcpAuthMethod::Cli)
        ));
        assert!(!is_acp_auth_method_supported(
            chat::AgentEngine::V1,
            Some(AcpAuthMethod::Cli)
        ));
        assert!(is_acp_auth_method_supported(chat::AgentEngine::V2, None));
    }

    #[test]
    fn run_output_format_is_structured() {
        assert!(!RunOutputFormat::Text.is_structured());
        assert!(!RunOutputFormat::default().is_structured());
        assert!(RunOutputFormat::StreamJson.is_structured());
    }

    /// `--output-format stream-json` must count as headless even without an explicit
    /// `--no-interactive`, so a logged-out run takes the machine-error path in the auth
    /// check rather than the interactive login prompt. Regression guard for that ordering.
    #[test]
    fn stream_json_is_headless_without_no_interactive() {
        let plain = ChatArgs::default();
        assert!(!plain.is_headless(), "default chat is interactive");

        let explicit = ChatArgs {
            no_interactive: true,
            ..Default::default()
        };
        assert!(explicit.is_headless(), "--no-interactive is headless");

        let stream_json = ChatArgs {
            output_format: RunOutputFormat::StreamJson,
            ..Default::default()
        };
        assert!(
            stream_json.is_headless(),
            "--output-format stream-json must be headless even without --no-interactive"
        );
    }

    #[test]
    fn chat_parses_output_format_stream_json() {
        fn parse(extra: &[&str]) -> ChatArgs {
            let mut argv = vec![CHAT_BINARY_NAME, "chat"];
            argv.extend_from_slice(extra);
            match <Cli as clap::Parser>::parse_from(argv).subcommand {
                Some(RootSubcommand::Chat(args)) => args,
                other => panic!("expected chat subcommand, got {other:?}"),
            }
        }

        assert_eq!(parse(&[]).output_format, RunOutputFormat::Text);
        assert_eq!(
            parse(&["--output-format", "stream-json"]).output_format,
            RunOutputFormat::StreamJson
        );
        // Independent of the list-oriented `--format`.
        let both = parse(&["--output-format", "stream-json", "--format", "json"]);
        assert_eq!(both.output_format, RunOutputFormat::StreamJson);
        assert_eq!(both.format, OutputFormat::Json);
    }

    #[test]
    fn unsupported_v3_acp_flags_empty_when_none_set() {
        assert!(unsupported_v3_acp_flags(None, None, None, false, None).is_empty());
    }

    #[test]
    fn unsupported_v3_acp_flags_lists_all_set_in_declaration_order() {
        let trust_tools = vec!["fs_read".to_string()];
        let flags = unsupported_v3_acp_flags(
            Some("kiro-cli"),
            Some("gpt-5.5"),
            Some("high"),
            true,
            Some(&trust_tools),
        );
        assert_eq!(flags, vec![
            "--agent",
            "--model",
            "--effort",
            "--trust-all-tools",
            "--trust-tools"
        ]);
    }

    #[test]
    fn unsupported_v3_acp_flags_reports_only_set_subset() {
        let flags = unsupported_v3_acp_flags(None, Some("gpt-5.5"), None, true, None);
        assert_eq!(flags, vec!["--model", "--trust-all-tools"]);
    }

    /// The content-collection opt-in passed to KAS must default to opted IN
    /// (matching V1/V2) and reflect the user's explicit setting. Regression
    /// guard for v3 requests being silently opted out of DataHub/KCO storage.
    #[tokio::test]
    async fn content_collection_enabled_reflects_setting() {
        use crate::database::settings::Setting;

        let mut os = Os::new().await.unwrap();

        // Default (unset): opted in, so KAS collects content like V1/V2.
        assert!(content_collection_enabled(&os));

        // Explicit opt-out is honored.
        os.database
            .settings
            .set(Setting::ShareCodeWhispererContent, false, None)
            .await
            .unwrap();
        assert!(!content_collection_enabled(&os));

        // Explicit opt-in is honored.
        os.database
            .settings
            .set(Setting::ShareCodeWhispererContent, true, None)
            .await
            .unwrap();
        assert!(content_collection_enabled(&os));
    }

    /// Test flag parsing for the top level [Cli]
    #[test]
    fn test_flags() {
        assert_eq!(Cli::parse_from([CHAT_BINARY_NAME, "-v"]), Cli {
            subcommand: None,
            verbose: 1,
            tui: false,
            legacy_ui: false,
            v3: false,
            resume: false,
            resume_id: None,
            resume_picker: false,
        });

        assert_eq!(Cli::parse_from([CHAT_BINARY_NAME, "-vvv"]), Cli {
            subcommand: None,
            verbose: 3,
            tui: false,
            legacy_ui: false,
            v3: false,
            resume: false,
            resume_id: None,
            resume_picker: false,
        });

        assert_eq!(Cli::parse_from([CHAT_BINARY_NAME, "chat", "-vv"]), Cli {
            subcommand: Some(RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                all_cwds: false,
                sessions: false,
                list_models: false,
                format: OutputFormat::Plain,
                delete_session: None,
                session_source: None,
                input: None,
                agent: None,
                model: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: false,
                wrap: None,
                require_mcp_startup: false,
                tui: false,
                legacy_ui: false,
                ..Default::default()
            })),
            verbose: 2,
            tui: false,
            legacy_ui: false,
            v3: false,
            resume: false,
            resume_id: None,
            resume_picker: false,
        });
    }

    #[test]
    fn test_version_changelog() {
        assert_parse!(["version", "--changelog"], RootSubcommand::Version {
            changelog: Some("".to_string()),
        });
    }

    #[test]
    fn test_version_changelog_all() {
        assert_parse!(["version", "--changelog=all"], RootSubcommand::Version {
            changelog: Some("all".to_string()),
        });
    }

    #[test]
    fn test_version_changelog_specific() {
        assert_parse!(["version", "--changelog=1.8.0"], RootSubcommand::Version {
            changelog: Some("1.8.0".to_string()),
        });
    }

    #[test]
    fn test_chat_with_context_profile() {
        assert_parse!(
            ["chat", "--profile", "my-profile"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                all_cwds: false,
                sessions: false,
                list_models: false,
                format: OutputFormat::Plain,
                delete_session: None,
                session_source: None,
                input: None,
                agent: Some("my-profile".to_string()),
                model: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: false,
                wrap: None,
                require_mcp_startup: false,
                tui: false,
                legacy_ui: false,
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_chat_with_context_profile_and_input() {
        assert_parse!(
            ["chat", "--profile", "my-profile", "Hello"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                all_cwds: false,
                sessions: false,
                list_models: false,
                format: OutputFormat::Plain,
                delete_session: None,
                session_source: None,
                input: Some("Hello".to_string()),
                agent: Some("my-profile".to_string()),
                model: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: false,
                wrap: None,
                require_mcp_startup: false,
                tui: false,
                legacy_ui: false,
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_chat_with_context_profile_and_accept_all() {
        assert_parse!(
            ["chat", "--profile", "my-profile", "--trust-all-tools"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                all_cwds: false,
                sessions: false,
                list_models: false,
                format: OutputFormat::Plain,
                delete_session: None,
                session_source: None,
                input: None,
                agent: Some("my-profile".to_string()),
                model: None,
                trust_all_tools: true,
                trust_tools: None,
                no_interactive: false,
                wrap: None,
                require_mcp_startup: false,
                tui: false,
                legacy_ui: false,
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_chat_with_no_interactive_and_resume() {
        assert_parse!(
            ["chat", "--no-interactive", "--resume"],
            RootSubcommand::Chat(ChatArgs {
                resume: true,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                all_cwds: false,
                sessions: false,
                list_models: false,
                format: OutputFormat::Plain,
                delete_session: None,
                session_source: None,
                input: None,
                agent: None,
                model: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: true,
                wrap: None,
                require_mcp_startup: false,
                tui: false,
                legacy_ui: false,
                ..Default::default()
            })
        );
        assert_parse!(
            ["chat", "--non-interactive", "-r"],
            RootSubcommand::Chat(ChatArgs {
                resume: true,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                all_cwds: false,
                sessions: false,
                list_models: false,
                format: OutputFormat::Plain,
                delete_session: None,
                session_source: None,
                input: None,
                agent: None,
                model: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: true,
                wrap: None,
                require_mcp_startup: false,
                tui: false,
                legacy_ui: false,
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_chat_with_resume_id() {
        assert_parse!(
            ["chat", "--resume-id", "abc-123"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: Some("abc-123".to_string()),
                resume_picker: false,
                list_sessions: false,
                all_cwds: false,
                sessions: false,
                list_models: false,
                format: OutputFormat::Plain,
                delete_session: None,
                session_source: None,
                input: None,
                agent: None,
                model: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: false,
                wrap: None,
                require_mcp_startup: false,
                tui: false,
                legacy_ui: false,
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_chat_with_tool_trust_all() {
        assert_parse!(
            ["chat", "--trust-all-tools"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                all_cwds: false,
                sessions: false,
                list_models: false,
                format: OutputFormat::Plain,
                delete_session: None,
                session_source: None,
                input: None,
                agent: None,
                model: None,
                trust_all_tools: true,
                trust_tools: None,
                no_interactive: false,
                wrap: None,
                require_mcp_startup: false,
                tui: false,
                legacy_ui: false,
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_chat_with_tool_trust_none() {
        assert_parse!(
            ["chat", "--trust-tools="],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                all_cwds: false,
                sessions: false,
                list_models: false,
                format: OutputFormat::Plain,
                delete_session: None,
                session_source: None,
                input: None,
                agent: None,
                model: None,
                trust_all_tools: false,
                trust_tools: Some(vec!["".to_string()]),
                no_interactive: false,
                wrap: None,
                require_mcp_startup: false,
                tui: false,
                legacy_ui: false,
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_chat_with_tool_trust_some() {
        assert_parse!(
            ["chat", "--trust-tools=fs_read,fs_write"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                all_cwds: false,
                sessions: false,
                list_models: false,
                format: OutputFormat::Plain,
                delete_session: None,
                session_source: None,
                input: None,
                agent: None,
                model: None,
                trust_all_tools: false,
                trust_tools: Some(vec!["fs_read".to_string(), "fs_write".to_string()]),
                no_interactive: false,
                wrap: None,
                require_mcp_startup: false,
                tui: false,
                legacy_ui: false,
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_chat_with_require_mcp_startup() {
        assert_parse!(
            ["chat", "--require-mcp-startup"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                all_cwds: false,
                sessions: false,
                list_models: false,
                format: OutputFormat::Plain,
                delete_session: None,
                session_source: None,
                input: None,
                agent: None,
                model: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: false,
                wrap: None,
                require_mcp_startup: true,
                tui: false,
                legacy_ui: false,
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_chat_with_different_wrap_modes() {
        assert_parse!(
            ["chat", "-w", "never"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                all_cwds: false,
                sessions: false,
                list_models: false,
                format: OutputFormat::Plain,
                delete_session: None,
                session_source: None,
                input: None,
                agent: None,
                model: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: false,
                wrap: Some(Never),
                require_mcp_startup: false,
                tui: false,
                legacy_ui: false,
                ..Default::default()
            })
        );
        assert_parse!(
            ["chat", "--wrap", "always"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                all_cwds: false,
                sessions: false,
                list_models: false,
                format: OutputFormat::Plain,
                delete_session: None,
                session_source: None,
                input: None,
                agent: None,
                model: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: false,
                wrap: Some(Always),
                require_mcp_startup: false,
                tui: false,
                legacy_ui: false,
                ..Default::default()
            })
        );
        assert_parse!(
            ["chat", "--wrap", "auto"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                all_cwds: false,
                sessions: false,
                list_models: false,
                format: OutputFormat::Plain,
                delete_session: None,
                session_source: None,
                input: None,
                agent: None,
                model: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: false,
                wrap: Some(Auto),
                require_mcp_startup: false,
                tui: false,
                legacy_ui: false,
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_chat_with_v3() {
        assert_parse!(
            ["chat", "--v3"],
            RootSubcommand::Chat(ChatArgs {
                v3: true,
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_top_level_v3() {
        assert_eq!(Cli::parse_from([CHAT_BINARY_NAME, "--v3"]), Cli {
            subcommand: None,
            verbose: 0,
            tui: false,
            legacy_ui: false,
            v3: true,
            resume: false,
            resume_id: None,
            resume_picker: false,
        });
    }

    #[test]
    fn test_v3_conflicts_with_classic() {
        // --v3 and --classic (alias of --legacy-ui) are mutually exclusive.
        assert!(Cli::try_parse_from([CHAT_BINARY_NAME, "chat", "--v3", "--classic"]).is_err());
        assert!(Cli::try_parse_from([CHAT_BINARY_NAME, "--v3", "--classic"]).is_err());
    }

    #[test]
    fn test_v3_conflicts_with_agent_engine() {
        // --v3 is a shorthand for --agent-engine=kas, so combining them is rejected.
        assert!(Cli::try_parse_from([CHAT_BINARY_NAME, "chat", "--v3", "--agent-engine=v2"]).is_err());
    }

    #[test]
    fn sessions_flag_rejects_other_chat_actions_during_parsing() {
        for incompatible in [
            vec!["--resume"],
            vec!["--resume-id", "session-id"],
            vec!["--resume-picker"],
            vec!["--no-interactive"],
            vec!["--list-sessions"],
            vec!["--list-models"],
            vec!["--delete-session", "session-id"],
            vec!["prompt"],
        ] {
            let mut argv = vec![CHAT_BINARY_NAME, "chat", "--sessions"];
            argv.extend(incompatible);
            let error = Cli::try_parse_from(argv).unwrap_err();
            assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
        }
    }

    #[test]
    fn sessions_mode_validation_precedes_programmatic_early_actions() {
        let args = ChatArgs {
            sessions: true,
            list_models: true,
            ..Default::default()
        };
        let error = args.validate_sessions_mode().unwrap_err().to_string();
        assert!(error.contains("--list-models"), "{error}");
    }

    #[test]
    fn sessions_mode_requires_an_available_interactive_tui() {
        let args = ChatArgs {
            sessions: true,
            ..Default::default()
        };
        assert!(ensure_dashboard_tui_available(&args, true, false).is_err());
        assert!(ensure_dashboard_tui_available(&args, false, true).is_err());
        assert!(ensure_dashboard_tui_available(&args, true, true).is_ok());
        assert!(ensure_dashboard_interactive(&args, true).is_err());
        assert!(ensure_dashboard_interactive(&args, false).is_ok());

        let regular_chat = ChatArgs::default();
        assert!(ensure_dashboard_tui_available(&regular_chat, false, false).is_ok());
        assert!(ensure_dashboard_interactive(&regular_chat, true).is_ok());
    }

    #[test]
    fn test_chat_with_list_models() {
        assert_parse!(
            ["chat", "--list-models"],
            RootSubcommand::Chat(ChatArgs {
                list_models: true,
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_chat_with_list_models_json() {
        assert_parse!(
            ["chat", "--list-models", "--format", "json"],
            RootSubcommand::Chat(ChatArgs {
                list_models: true,
                format: OutputFormat::Json,
                ..Default::default()
            })
        );
    }

    mod resolve_agent_engine {
        use super::*;

        async fn make_os() -> crate::os::Os {
            crate::os::Os::new().await.unwrap()
        }

        #[tokio::test]
        async fn defaults_to_new_tui_engine() {
            // In test environments stdin is piped; the default should still match
            // an interactive session (the new-TUI engine).
            let os = make_os().await;
            let args = ChatArgs::default();
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::V2);
        }

        #[tokio::test]
        async fn non_interactive_defaults_to_tui() {
            let os = make_os().await;
            let non_interactive = ChatArgs {
                no_interactive: true,
                ..Default::default()
            }
            .resolve_agent_engine(&os)
            .unwrap();
            assert_eq!(non_interactive, chat::AgentEngine::V2);
        }

        #[tokio::test]
        async fn explicit_engine_overrides_default() {
            let os = make_os().await;
            let args = ChatArgs {
                agent_engine: Some(chat::AgentEngine::Kas),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::Kas);
        }

        #[tokio::test]
        async fn v3_flag_resolves_to_kas() {
            let os = make_os().await;
            let args = ChatArgs {
                v3: true,
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::Kas);
        }

        #[tokio::test]
        async fn v3_flag_resolves_to_kas_non_interactive() {
            let os = make_os().await;
            let mut args = ChatArgs {
                v3: true,
                no_interactive: true,
                input: Some("hello".to_string()),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::Kas);
            assert_eq!(args.resolve_non_interactive_input().unwrap(), "hello");
        }

        #[tokio::test]
        async fn sessions_flag_implies_kas() {
            let os = make_os().await;
            let args = ChatArgs {
                sessions: true,
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::Kas);
        }

        #[tokio::test]
        async fn sessions_flag_rejects_explicit_incompatible_engine() {
            let os = make_os().await;
            for engine in [chat::AgentEngine::V1, chat::AgentEngine::V2] {
                let args = ChatArgs {
                    sessions: true,
                    agent_engine: Some(engine),
                    ..Default::default()
                };
                let error = args.resolve_agent_engine(&os).unwrap_err().to_string();
                assert!(error.contains("--sessions requires the V3 agent"), "{error}");
            }
        }

        #[tokio::test]
        async fn explicit_v2_in_non_interactive() {
            let os = make_os().await;
            let args = ChatArgs {
                no_interactive: true,
                agent_engine: Some(chat::AgentEngine::V2),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::V2);
        }

        // The test harness force-enables the V2NonInteractive rollout, so the non-interactive
        // default resolves to V2 here. stream-json accepts that default unchanged (on a stable
        // build the default is V1 and the same path would reject; see stream_json_rejects_*).
        #[tokio::test]
        async fn stream_json_accepts_default_engine_when_unspecified() {
            let os = make_os().await;
            let args = ChatArgs {
                no_interactive: true,
                output_format: crate::cli::RunOutputFormat::StreamJson,
                input: Some("hi".to_string()),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::V2);
        }

        #[tokio::test]
        async fn stream_json_rejects_explicit_v1() {
            let os = make_os().await;
            let args = ChatArgs {
                no_interactive: true,
                agent_engine: Some(chat::AgentEngine::V1),
                output_format: crate::cli::RunOutputFormat::StreamJson,
                input: Some("hi".to_string()),
                ..Default::default()
            };
            assert!(args.resolve_agent_engine(&os).is_err());
        }

        // stream-json engine selection: accept the effective v2/v3 engine, reject v1 from any source.

        async fn make_os_with_engine_setting(value: &str) -> crate::os::Os {
            use crate::database::settings::Setting;
            let mut os = make_os().await;
            os.database
                .settings
                .set(Setting::ChatAgentEngine, value, None)
                .await
                .unwrap();
            os
        }

        #[tokio::test]
        async fn stream_json_keeps_explicit_v3_setting() {
            for value in ["v3", "kas"] {
                let os = make_os_with_engine_setting(value).await;
                let args = ChatArgs {
                    no_interactive: true,
                    output_format: crate::cli::RunOutputFormat::StreamJson,
                    input: Some("hi".to_string()),
                    ..Default::default()
                };
                assert_eq!(
                    args.resolve_agent_engine(&os).unwrap(),
                    chat::AgentEngine::Kas,
                    "chat.agentEngine={value} + stream-json should stay Kas, not V2"
                );
            }
        }

        #[tokio::test]
        async fn stream_json_keeps_v2_setting() {
            let os = make_os_with_engine_setting("v2").await;
            let args = ChatArgs {
                no_interactive: true,
                output_format: crate::cli::RunOutputFormat::StreamJson,
                input: Some("hi".to_string()),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::V2);
        }

        #[tokio::test]
        async fn stream_json_explicit_v2_flag_beats_v1_setting() {
            let os = make_os_with_engine_setting("v1").await;
            let args = ChatArgs {
                no_interactive: true,
                agent_engine: Some(chat::AgentEngine::V2),
                output_format: crate::cli::RunOutputFormat::StreamJson,
                input: Some("hi".to_string()),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::V2);
        }

        #[tokio::test]
        async fn stream_json_rejects_v1_setting() {
            let os = make_os_with_engine_setting("v1").await;
            let args = ChatArgs {
                no_interactive: true,
                output_format: crate::cli::RunOutputFormat::StreamJson,
                input: Some("hi".to_string()),
                ..Default::default()
            };
            assert!(args.resolve_agent_engine(&os).is_err());
        }

        #[tokio::test]
        async fn stream_json_v3_flag_stays_kas() {
            let os = make_os().await;
            let args = ChatArgs {
                v3: true,
                no_interactive: true,
                output_format: crate::cli::RunOutputFormat::StreamJson,
                input: Some("hi".to_string()),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::Kas);
        }

        #[tokio::test]
        async fn legacy_ui_flag_conflicts_with_non_interactive_v2_default() {
            // Tests run with piped stdin and rollout enabled, so default_engine
            // returns V2. --legacy-ui then conflicts with V2 (it requires V1).
            let os = make_os().await;
            let args = ChatArgs {
                legacy_ui: true,
                ..Default::default()
            };
            assert!(args.resolve_agent_engine(&os).is_err());
        }

        #[tokio::test]
        async fn tui_flag_with_explicit_v2() {
            let os = make_os().await;
            let args = ChatArgs {
                tui: true,
                agent_engine: Some(chat::AgentEngine::V2),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::V2);
        }

        #[tokio::test]
        async fn conflict_legacy_ui_with_kas() {
            let os = make_os().await;
            let args = ChatArgs {
                legacy_ui: true,
                agent_engine: Some(chat::AgentEngine::Kas),
                ..Default::default()
            };
            assert!(args.resolve_agent_engine(&os).is_err());
        }

        #[tokio::test]
        async fn conflict_legacy_ui_with_v2() {
            let os = make_os().await;
            let args = ChatArgs {
                legacy_ui: true,
                agent_engine: Some(chat::AgentEngine::V2),
                ..Default::default()
            };
            assert!(args.resolve_agent_engine(&os).is_err());
        }

        #[tokio::test]
        async fn conflict_tui_with_v1() {
            let os = make_os().await;
            let args = ChatArgs {
                tui: true,
                agent_engine: Some(chat::AgentEngine::V1),
                ..Default::default()
            };
            assert!(args.resolve_agent_engine(&os).is_err());
        }

        #[tokio::test]
        async fn legacy_ui_with_v1_is_ok() {
            let os = make_os().await;
            let args = ChatArgs {
                legacy_ui: true,
                agent_engine: Some(chat::AgentEngine::V1),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::V1);
        }

        #[tokio::test]
        async fn resume_id_unknown_to_local_stores_resolves_to_kas() {
            // A `--resume-id` that exists in neither the V1 nor V2 store lives
            // in the KAS store (local KAS or cloud), so with no explicit engine
            // the resolution must pick KAS — this is what makes
            // `kiro chat --resume-id <cloud-id>` work without `--v3`.
            // (Tests run with the remote-sandbox rollout force-enabled.)
            let os = make_os().await;
            let args = ChatArgs {
                resume_id: Some("00000000-dead-beef-0000-000000000000".to_string()),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::Kas);
        }

        #[tokio::test]
        async fn resume_id_defers_to_explicit_engine() {
            // An explicit --agent-engine always wins over the resume-id inference.
            let os = make_os().await;
            let args = ChatArgs {
                resume_id: Some("00000000-dead-beef-0000-000000000000".to_string()),
                agent_engine: Some(chat::AgentEngine::V2),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::V2);
        }

        #[tokio::test]
        async fn resume_id_of_v2_session_from_another_directory_stays_local() {
            // V2 resume loads a session by id no matter which directory it was
            // created in, so the ownership probe must not be cwd-scoped: a
            // cross-directory V2 id (full or 8-char prefix) must never be
            // rerouted to KAS, where the resume would fail as not-found.
            let os = make_os().await;
            let sessions_dir = tempfile::tempdir().unwrap();
            let session_id = "11111111-2222-4333-8444-555555555555";
            std::fs::write(
                sessions_dir.path().join(format!("{session_id}.json")),
                serde_json::to_string(&serde_json::json!({
                    "session_id": session_id,
                    "cwd": "/somewhere/else/entirely",
                    "created_at": chrono::Utc::now(),
                    "updated_at": chrono::Utc::now(),
                    "title": "created in another directory",
                }))
                .unwrap(),
            )
            .unwrap();
            let probe_cwd = std::path::Path::new("/current/working/dir");

            // Full id and displayed 8-char prefix both stay local.
            assert!(chat::resume_id_owned_locally(
                &os.database,
                Some(sessions_dir.path()),
                probe_cwd,
                session_id
            ));
            assert!(chat::resume_id_owned_locally(
                &os.database,
                Some(sessions_dir.path()),
                probe_cwd,
                "11111111"
            ));
            // An id no store owns still routes to KAS.
            assert!(!chat::resume_id_owned_locally(
                &os.database,
                Some(sessions_dir.path()),
                probe_cwd,
                "00000000-dead-beef-0000-000000000000"
            ));
        }

        #[tokio::test]
        async fn non_interactive_with_input_resolves() {
            let os = make_os().await;
            let mut args = ChatArgs {
                no_interactive: true,
                input: Some("hello".to_string()),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::V2);
            assert_eq!(args.resolve_non_interactive_input().unwrap(), "hello");
        }

        #[tokio::test]
        async fn non_interactive_with_kas_and_input() {
            let os = make_os().await;
            let mut args = ChatArgs {
                no_interactive: true,
                agent_engine: Some(chat::AgentEngine::Kas),
                input: Some("test prompt".to_string()),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::Kas);
            assert_eq!(args.resolve_non_interactive_input().unwrap(), "test prompt");
        }

        #[tokio::test]
        async fn non_interactive_without_input_errors() {
            let _os = make_os().await;
            let mut args = ChatArgs {
                no_interactive: true,
                input: None,
                ..Default::default()
            };
            // stdin is piped but empty in tests, so this should error
            assert!(args.resolve_non_interactive_input().is_err());
        }

        #[tokio::test]
        async fn non_interactive_v2_with_input() {
            let os = make_os().await;
            let mut args = ChatArgs {
                no_interactive: true,
                agent_engine: Some(chat::AgentEngine::V2),
                input: Some("query".to_string()),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::V2);
            assert_eq!(args.resolve_non_interactive_input().unwrap(), "query");
        }

        // ── Remote sandbox gating (`--cloud` / `--repo`) ──────────────────
        // In unit tests the global rollout is `init_for_tests_enable_all`,
        // which enables `remote_sandbox`, so the feature gate passes and we
        // exercise the V3-only conflict logic. (The build-level dark-ship gate
        // — OFF in released builds — is proven against the real embedded
        // config in `rollout.rs`.)

        #[tokio::test]
        async fn remote_with_v3_resolves_to_kas() {
            let os = make_os().await;
            let args = ChatArgs {
                cloud: true,
                v3: true,
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::Kas);
        }

        #[tokio::test]
        async fn remote_with_explicit_kas_resolves_to_kas() {
            let os = make_os().await;
            let args = ChatArgs {
                cloud: true,
                agent_engine: Some(chat::AgentEngine::Kas),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::Kas);
        }

        #[tokio::test]
        async fn repo_with_v3_resolves_to_kas() {
            let os = make_os().await;
            let args = ChatArgs {
                repo: Some(vec!["owner/name".to_string()]),
                v3: true,
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::Kas);
        }

        #[tokio::test]
        async fn remote_requires_v3_errors_on_v2() {
            let os = make_os().await;
            let args = ChatArgs {
                cloud: true,
                agent_engine: Some(chat::AgentEngine::V2),
                ..Default::default()
            };
            assert!(args.resolve_agent_engine(&os).is_err());
        }

        #[tokio::test]
        async fn remote_requires_v3_errors_on_v1() {
            let os = make_os().await;
            let args = ChatArgs {
                cloud: true,
                agent_engine: Some(chat::AgentEngine::V1),
                ..Default::default()
            };
            assert!(args.resolve_agent_engine(&os).is_err());
        }

        #[tokio::test]
        async fn bare_cloud_auto_selects_kas() {
            // `kiro chat --cloud` needs no `--v3`: the cloud flag implies the
            // KAS engine (the only one hosting the remote execution target).
            let os = make_os().await;
            let args = ChatArgs {
                cloud: true,
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::Kas);
        }

        #[tokio::test]
        async fn bare_repo_auto_selects_kas() {
            let os = make_os().await;
            let args = ChatArgs {
                cloud: true,
                repo: Some(vec!["owner/name".to_string()]),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::Kas);
        }

        #[tokio::test]
        async fn repo_requires_v3_errors_on_v2() {
            let os = make_os().await;
            let args = ChatArgs {
                repo: Some(vec!["owner/name".to_string()]),
                agent_engine: Some(chat::AgentEngine::V2),
                ..Default::default()
            };
            assert!(args.resolve_agent_engine(&os).is_err());
        }
    }
}
