use crate::theme::StyledText;
use crate::util::consts::env_var::{
    KIRO_CHAT_LOG_FILE,
    KIRO_TEST_TUI_JS_PATH,
};
use crate::util::env_var::is_log_stdout_enabled;
pub mod agent;
pub mod chat;
mod debug;
mod diagnostics;
pub mod experiment;
pub mod feed;
mod issue;
mod mcp;
mod settings;
pub mod update;
mod user;

use std::fmt::Display;
use std::io::{
    Write as _,
    stdout,
};
use std::path::PathBuf;
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
        /// Agent engine to use: "v1", "v2" (default), or "kas"
        #[arg(long, value_name = "ENGINE", default_value_t = chat::AgentEngine::V2)]
        agent_engine: chat::AgentEngine,
    },
    /// Start a persistent KAS agent server over WebSocket
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
    },
    /// Start a voice recording server for remote/cloud desktop use.
    #[cfg(feature = "voice")]
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
    #[command(name = "voice-cloud-setup")]
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
            if matches!(self, Self::Chat(ref args) if args.no_interactive) && !is_internal_chat {
                eprintln!(
                    "Not logged in. Set the {KIRO_API_KEY} environment variable or run `{CLI_BINARY_NAME} login` first."
                );
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

        // Daily heartbeat check
        if os.database.record_heartbeat_if_needed() {
            os.telemetry.send_daily_heartbeat().ok();
        }

        // Send executed telemetry.
        if self.valid_for_telemetry() {
            os.telemetry
                .send_cli_subcommand_executed(&os.database, &self)
                .await
                .ok();
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
                Self::Issue(args) => args.execute(os).await,
                Self::Version { changelog } => Cli::print_version(changelog),
                Self::Chat(mut args) => {
                    // Hidden internal subcommands (`chat _ export-session`,
                    // `chat _ import-session`). Bypass auth/login, telemetry,
                    // and TUI launch; emit a single JSON line and exit.
                    if let Some(command) = args.command.take() {
                        return command.execute().await;
                    }

                    // Handle --list-models before TUI launch
                    if args.list_models {
                        return crate::cli::chat::cli::model::print_model_list(os, args.format)
                            .await
                            .map_err(|e| e.into());
                    }

                    // Handle headless session commands before TUI launch
                    if let Some(result) = handle_session_flags(&args, os).await {
                        return result;
                    }

                    // Run cleanup before chat starts; show a message if it takes >3 seconds
                    let msg_handle = tokio::spawn(async {
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        eprintln!("Cleaning up old conversations...");
                    });
                    if let Err(e) = crate::cleanup::cleanup_old_data(&os.env, &os.fs, &os.database).await {
                        tracing::error!("Cleanup failed: {}", e);
                    }
                    msg_handle.abort();

                    let tui_available =
                        crate::embedded_tui::are_assets_embedded(os) || std::env::var(KIRO_TEST_TUI_JS_PATH).is_ok();
                    let engine = args.resolve_agent_engine(os)?;
                    match engine {
                        chat::AgentEngine::V1 => args.execute(os).await,
                        chat::AgentEngine::V2 | chat::AgentEngine::Kas => {
                            if !args.no_interactive && !tui_available {
                                tracing::error!("TUI assets not available, falling back to legacy UI");
                                args.execute(os).await
                            } else {
                                launch_acp_session(os, &mut args, engine).await
                            }
                        },
                    }
                },
                Self::Mcp(args) => args.execute(os, &mut std::io::stderr()).await,
                Self::Update(args) => args.execute(os).await,
                Self::Acp {
                    agent,
                    model,
                    effort,
                    trust_all_tools,
                    trust_tools,
                    agent_engine,
                } => {
                    if agent_engine == chat::AgentEngine::Kas {
                        return execute_kas_acp(os).await;
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
                Self::Voice { ptt } => {
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
                    let result =
                        voice::voice_handler::voice_only_mode(server_url, silence_timeout, language, model_size).await;
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
            Self::Issue(args) => args.execute(os).await,
            Self::Version { changelog } => Cli::print_version(changelog),
            Self::Chat(mut args) => {
                // Hidden internal subcommands (`chat _ export-session`,
                // `chat _ import-session`). Bypass auth/login, telemetry,
                // and TUI launch; emit a single JSON line and exit.
                if let Some(command) = args.command.take() {
                    return command.execute().await;
                }

                // Handle --list-models before TUI launch
                if args.list_models {
                    return crate::cli::chat::cli::model::print_model_list(os, args.format)
                        .await
                        .map_err(|e| e.into());
                }

                // Handle headless session commands before TUI launch
                if let Some(result) = handle_session_flags(&args, os).await {
                    return result;
                }

                if let Err(e) = os.client.resolve_profile_if_missing(&mut os.database).await {
                    tracing::warn!("Failed to resolve profile: {e}");
                }

                // Run cleanup before chat starts; show a message if it takes >3 seconds
                let msg_handle = tokio::spawn(async {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    eprintln!("Cleaning up old conversations...");
                });
                if let Err(e) = crate::cleanup::cleanup_old_data(&os.env, &os.fs, &os.database).await {
                    tracing::error!("Cleanup failed: {}", e);
                }
                msg_handle.abort();

                let tui_available =
                    crate::embedded_tui::are_assets_embedded(os) || std::env::var(KIRO_TEST_TUI_JS_PATH).is_ok();
                let engine = args.resolve_agent_engine(os)?;
                let is_tui_supported = crate::util::system_info::is_tui_supported();
                tracing::debug!(?engine, is_tui_supported, tui_available, "launch decision");
                match engine {
                    chat::AgentEngine::V1 => args.execute(os).await,
                    chat::AgentEngine::V2 | chat::AgentEngine::Kas => {
                        if !args.no_interactive && (!is_tui_supported || !tui_available) {
                            if !tui_available {
                                tracing::error!("TUI assets not available, falling back to legacy UI");
                            }
                            if !is_tui_supported {
                                eprintln!("The TUI is currently not supported for the current platform");
                            }
                            args.execute(os).await
                        } else {
                            launch_acp_session(os, &mut args, engine).await
                        }
                    },
                }
            },
            Self::Mcp(args) => args.execute(os, &mut std::io::stderr()).await,
            Self::Update(args) => args.execute(os).await,
            Self::Acp {
                agent,
                model,
                effort,
                trust_all_tools,
                trust_tools,
                agent_engine,
            } => {
                if agent_engine == chat::AgentEngine::Kas {
                    return execute_kas_acp(os).await;
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
            Self::Voice { ptt } => {
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
                let result =
                    voice::voice_handler::voice_only_mode(server_url, silence_timeout, language, model_size).await;
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

/// Build [`LaunchOptions`] and launch the ACP session.
/// When `--no-interactive` is set, resolves the prompt input from CLI args or
/// stdin and selects the non-interactive variant; otherwise runs the
/// interactive TUI.
async fn launch_acp_session(os: &Os, args: &mut ChatArgs, agent_engine: chat::AgentEngine) -> Result<ExitCode> {
    let mode = args.mode;
    let options = if args.no_interactive {
        let input = args.resolve_non_interactive_input()?;
        crate::launch::LaunchOptions::non_interactive(
            agent_engine,
            mode,
            input,
            args.trust_all_tools,
            args.agent.clone(),
            args.model.clone(),
            args.trust_tools.clone(),
        )
    } else {
        crate::launch::LaunchOptions::interactive(agent_engine, mode)
    };
    crate::launch::launch(options, os).await
}

/// Spawn the KAS TypeScript agent as an ACP server over stdio.
/// Extracts embedded node + KAS assets if needed, then execs
/// `node --experimental-wasm-modules acp-server.js --transport=stdio
///  --auth=acp-callback`.
async fn execute_kas_acp(os: &Os) -> Result<ExitCode> {
    let mut child = spawn_kas_process(os, KasStdio::Inherit).await?;
    let status = child.wait().await?;
    Ok(status.code().map_or(ExitCode::FAILURE, |c| ExitCode::from(c as u8)))
}

/// Stdio configuration for a spawned KAS process.
#[derive(Debug, Clone, Copy)]
pub(crate) enum KasStdio {
    /// Inherit parent stdio. Used for passthrough `kiro-cli acp`.
    Inherit,
    /// Pipe stdin/stdout (for ACP client use), null stderr.
    Piped,
}

/// Spawn a KAS process with `--transport=stdio`.
///
/// Resolves the node binary and server script from `KIRO_KAS_SERVER_PATH` or
/// embedded assets. KAS is launched in `--auth=acp-callback` mode: it makes
/// the ACP client (the parent of this process) responsible for fielding
/// `_kiro/auth/getAccessToken` whenever KAS needs an access token.
///
/// Internal chat-cli ACP-client paths handle the callback via
/// `chat_cli_v2::auth::kas_token::handle_ext_method`. External
/// clients connecting to a `KasStdio::Inherit` spawn (e.g. `kiro-cli acp`)
/// MUST implement the same callback themselves.
pub(crate) async fn spawn_kas_process(os: &Os, stdio: KasStdio) -> Result<tokio::process::Child> {
    let (node_bin, server_js) = if let Ok(kas_server_path) = std::env::var("KIRO_KAS_SERVER_PATH") {
        (PathBuf::from("node"), PathBuf::from(kas_server_path))
    } else if let Some(paths) = crate::embedded_tui::extract_kas_assets_if_needed(os).await? {
        paths
    } else {
        bail!("KAS assets not embedded and KIRO_KAS_SERVER_PATH not set");
    };

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
    }

    let child = cmd
        .spawn()
        .with_context(|| format!("failed to spawn KAS: {} {}", node_bin.display(), server_js.display()))?;

    Ok(child)
}

/// Spawn KAS as a persistent WebSocket server on the given port.
async fn execute_kas_serve(os: &Os, port: u16) -> Result<ExitCode> {
    let (node_bin, server_js) = if let Ok(kas_server_path) = std::env::var("KIRO_KAS_SERVER_PATH") {
        (PathBuf::from("node"), PathBuf::from(kas_server_path))
    } else if let Some(paths) = crate::embedded_tui::extract_kas_assets_if_needed(os).await? {
        paths
    } else {
        bail!("KAS assets not available. Install nightly or set KIRO_KAS_SERVER_PATH.");
    };

    debug!(
        "Spawning KAS serve: {} --experimental-wasm-modules {} --transport=ws --auth=acp-callback (port {})",
        node_bin.display(),
        server_js.display(),
        port,
    );

    eprintln!("Kiro agent server running on port {}", port);
    eprintln!("Connect with: kiro-cli --remote ws://<this-host>:{}", port);

    let mut child = tokio::process::Command::new(&node_bin)
        .arg("--experimental-wasm-modules")
        .arg(&server_js)
        .arg("--transport=ws")
        .arg("--auth=acp-callback")
        .env("ACP_WS_PORT", port.to_string())
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(false)
        .spawn()
        .with_context(|| {
            format!(
                "failed to spawn KAS serve: {} {}",
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
        args.delete_session.as_deref(),
        args.session_source.map(|s| match s {
            SessionSourceArg::V1 => SessionSource::V1,
            SessionSourceArg::V2 => SessionSource::V2,
            SessionSourceArg::V3 => SessionSource::Kas,
        }),
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
        if matches!(self, Self::Chat(_)) && std::env::args().any(|a| a == "--list") {
            return "chat:list".to_string();
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

    fn print_version(changelog: Option<String>) -> Result<ExitCode> {
        // If no changelog is requested, display normal version information
        if changelog.is_none() {
            let _ = writeln!(stdout(), "{}", Self::command().render_version());
            return Ok(ExitCode::SUCCESS);
        }

        let changelog_value = changelog.unwrap_or_default();
        let feed = Feed::load();

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

    /// Test flag parsing for the top level [Cli]
    #[test]
    fn test_flags() {
        assert_eq!(Cli::parse_from([CHAT_BINARY_NAME, "-v"]), Cli {
            subcommand: None,
            verbose: 1,
            tui: false,
            legacy_ui: false,
            resume: false,
            resume_id: None,
            resume_picker: false,
        });

        assert_eq!(Cli::parse_from([CHAT_BINARY_NAME, "-vvv"]), Cli {
            subcommand: None,
            verbose: 3,
            tui: false,
            legacy_ui: false,
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
        async fn defaults_to_v1_when_stdin_not_terminal() {
            // In test environments stdin is piped, so default is V1
            let os = make_os().await;
            let args = ChatArgs::default();
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::V1);
        }

        #[tokio::test]
        async fn defaults_to_v1_non_interactive() {
            let os = make_os().await;
            let args = ChatArgs {
                no_interactive: true,
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::V1);
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
        async fn explicit_v2_in_non_interactive() {
            let os = make_os().await;
            let args = ChatArgs {
                no_interactive: true,
                agent_engine: Some(chat::AgentEngine::V2),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::V2);
        }

        #[tokio::test]
        async fn legacy_ui_flag_defaults_to_v1() {
            let os = make_os().await;
            let args = ChatArgs {
                legacy_ui: true,
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::V1);
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
        async fn non_interactive_with_input_resolves() {
            let os = make_os().await;
            let mut args = ChatArgs {
                no_interactive: true,
                input: Some("hello".to_string()),
                ..Default::default()
            };
            assert_eq!(args.resolve_agent_engine(&os).unwrap(), chat::AgentEngine::V1);
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
    }
}
