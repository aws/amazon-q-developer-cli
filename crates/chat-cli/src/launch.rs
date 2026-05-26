use std::process::ExitCode;
use std::time::Duration;

pub use chat_cli_v2::launch_options::{
    AgentEngine,
    AgentMode,
    Interactivity,
    LaunchOptions,
};
use eyre::{
    Context as _,
    Result,
};
use tracing::{
    debug,
    info,
};

use crate::embedded_tui::{
    extract_kas_assets_if_needed,
    extract_tui_assets_if_needed,
};
use crate::os::Os;
use crate::util::paths::kas_token_path;

/// Launch the session according to the configured options.
pub async fn launch(options: LaunchOptions, os: &Os) -> Result<ExitCode> {
    let LaunchOptions {
        agent_engine,
        mode,
        interactivity,
        trust_all_tools,
    } = options;

    if let Interactivity::NonInteractive { input } = interactivity {
        return launch_acp_non_interactive(os, agent_engine, mode, input, trust_all_tools).await;
    }

    launch_acp_interactive(os, agent_engine, mode).await
}

/// Determine the `FORCE_COLOR` value to pass to the bun/chalk process.
///
/// Returns `None` when color should not be forced (i.e. `NO_COLOR` is set),
/// otherwise returns the appropriate chalk color level:
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

/// Launch the interactive TUI. Extracts embedded assets and spawns bun with the TUI JS bundle.
async fn launch_acp_interactive(os: &Os, agent_engine: AgentEngine, mode: Option<AgentMode>) -> Result<ExitCode> {
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
        .kill_on_drop(true);

    // Write feed.json to data dir and pass the path to the TUI (avoids 100KB env var).
    let feed_path = crate::util::paths::feed_json_path()?;
    std::fs::write(&feed_path, include_str!("cli/feed.json"))?;
    cmd.env("KIRO_FEED_FILE", &feed_path);
    if let Some(ref force_color) = force_color {
        cmd.env("FORCE_COLOR", force_color);
    }

    // Resolve telemetry identity for the TUI
    let telemetry_enabled = !crate::util::env_var::is_telemetry_disabled()
        && os
            .database
            .settings
            .get_bool(crate::database::settings::Setting::TelemetryEnabled)
            .unwrap_or(true);
    cmd.env("KIRO_TELEMETRY_ENABLED", telemetry_enabled.to_string());

    match agent_engine {
        AgentEngine::Kas => {
            // Resolve user identity for KAS telemetry (not needed for Rust engine)
            if let Ok(Ok(output)) = tokio::time::timeout(Duration::from_secs(5), os.client.get_usage_limits()).await
                && let Some(info) = output.user_info()
            {
                cmd.env("KIRO_USER_ID", info.user_id());
            }

            let token_path = kas_token_path(os)?;
            // Seed the file at the same path KAS will read.
            // KAS sidecar lifecycle: see chat_cli_v2::auth::kas_token_sync.
            chat_cli_v2::auth::kas_token_sync::populate_kas_from_store_at_path(&token_path).await;
            cmd.env("KIRO_KAS_TOKEN_PATH", &token_path);
            cmd.env("KIRO_AGENT_ENGINE", "kas");

            if let Ok(kas_server_path) = std::env::var("KIRO_KAS_SERVER_PATH") {
                cmd.env("KIRO_AGENT_PATH", "node");
                cmd.env("KIRO_KAS_SERVER_PATH", &kas_server_path);
                info!("Using KAS agent engine, server path override: {}", kas_server_path);
            } else if let Some((node_bin, server_js)) = extract_kas_assets_if_needed(os).await? {
                cmd.env("KIRO_AGENT_PATH", &node_bin);
                cmd.env("KIRO_KAS_SERVER_PATH", &server_js);
                info!(
                    "Using KAS agent engine, embedded node: {}, server: {}",
                    node_bin.display(),
                    server_js.display()
                );
            } else {
                cmd.env("KIRO_AGENT_PATH", "node");
                info!("Using KAS agent engine, server resolved from @kiro/agent package");
            }
        },
        AgentEngine::V2 => {
            cmd.env("KIRO_AGENT_PATH", &current_exe);
        },
        AgentEngine::V1 => {
            unreachable!("V1 engine does not use the ACP launch path");
        },
    }

    if let Some(mode) = mode {
        cmd.env("KIRO_MODE", mode.to_string());
    }

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

    let exit_code = status
        .and_then(|s| s.code())
        .map_or(ExitCode::FAILURE, |e| ExitCode::from(e as u8));

    Ok(exit_code)
}

/// Drive a non-interactive V2 session.
async fn launch_acp_non_interactive(
    os: &Os,
    agent_engine: AgentEngine,
    mode: Option<AgentMode>,
    input: String,
    trust_all_tools: bool,
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
            if self.trust_all_tools {
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

        async fn ext_method(&self, _args: acp::ExtRequest) -> acp::Result<acp::ExtResponse> {
            Err(acp::Error::method_not_found())
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
        cmd.arg(format!("--token-path={}", kas_token_path(os)?.display()));
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
                NonInteractiveAcpClient { trust_all_tools },
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
                        .title(Some("Kiro CLI (non-interactive)".to_string())),
                )),
            )
            .await
            .context("ACP initialize failed")?;

            let cwd = std::env::current_dir().context("failed to resolve current working directory")?;
            let session = conn
                .new_session(acp::NewSessionRequest::new(cwd))
                .await
                .context("ACP new_session failed")?;

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
    result
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
