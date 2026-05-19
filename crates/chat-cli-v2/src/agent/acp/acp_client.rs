//! Interactive ACP Test Client for testing ACP agents.
//!
//! Usage (from workspace root):
//! ```bash
//! # Run the interactive test client
//! cargo run -p chat_cli -- acp-client --agent ./target/debug/chat_cli
//! or
//! ./target/debug/chat_cli acp-client --agent ./target/debug/chat_cli
//! ```

use std::process::ExitCode;

use agent_client_protocol::{
    self as acp,
    Agent as _,
};
use eyre::Result;
use tokio_util::compat::{
    TokioAsyncReadCompatExt,
    TokioAsyncWriteCompatExt,
};

struct AcpClient;

#[async_trait::async_trait(?Send)]
impl acp::Client for AcpClient {
    async fn session_notification(&self, args: acp::SessionNotification) -> acp::Result<()> {
        match args.update {
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk { content, .. }) => match content {
                acp::ContentBlock::Text(text) => println!("Agent: {}", text.text),
                _ => println!("Agent: <non-text content>"),
            },
            acp::SessionUpdate::ToolCall(tool_call) => {
                println!("🔧 Tool Call: {tool_call:#?}");
            },
            acp::SessionUpdate::ToolCallUpdate(tool_call_update) => {
                println!("✅ Tool Call Update: {tool_call_update:#?}");
            },
            _ => {
                // Handle other session update types if needed
            },
        }
        Ok(())
    }

    async fn request_permission(
        &self,
        args: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        println!("Permission request from server: {args:?}");

        // Auto-approve first option if available
        let option_id = args
            .options
            .first()
            .map(|opt| opt.option_id.clone())
            .ok_or_else(acp::Error::internal_error)?;

        Ok(acp::RequestPermissionResponse::new(
            acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(option_id)),
        ))
    }

    async fn write_text_file(&self, _args: acp::WriteTextFileRequest) -> acp::Result<acp::WriteTextFileResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn read_text_file(&self, _args: acp::ReadTextFileRequest) -> acp::Result<acp::ReadTextFileResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn create_terminal(&self, _args: acp::CreateTerminalRequest) -> acp::Result<acp::CreateTerminalResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn terminal_output(&self, _args: acp::TerminalOutputRequest) -> acp::Result<acp::TerminalOutputResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn release_terminal(&self, _args: acp::ReleaseTerminalRequest) -> acp::Result<acp::ReleaseTerminalResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn wait_for_terminal_exit(
        &self,
        _args: acp::WaitForTerminalExitRequest,
    ) -> acp::Result<acp::WaitForTerminalExitResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn kill_terminal(&self, _args: acp::KillTerminalRequest) -> acp::Result<acp::KillTerminalResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn ext_method(&self, _args: acp::ExtRequest) -> acp::Result<acp::ExtResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn ext_notification(&self, _args: acp::ExtNotification) -> acp::Result<()> {
        Err(acp::Error::method_not_found())
    }
}

pub async fn execute(agent_path: String) -> Result<ExitCode> {
    let mut child = tokio::process::Command::new(&agent_path)
        .arg("acp")  // Add the acp subcommand
        .env("KIRO_LOG_LEVEL", "debug")
        .env("KIRO_CHAT_LOG_FILE", "/tmp/kiro-headless-chat.log")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let outgoing = child.stdin.take().unwrap().compat_write();
    let incoming = child.stdout.take().unwrap().compat();

    let local_set = tokio::task::LocalSet::new();
    local_set
        .run_until(async move {
            let (conn, handle_io) = acp::ClientSideConnection::new(AcpClient, outgoing, incoming, |fut| {
                tokio::task::spawn_local(fut);
            });

            tokio::task::spawn_local(handle_io);

            // Initialize connection
            conn.initialize(acp::InitializeRequest::new(acp::ProtocolVersion::V1).client_info(Some(
                acp::Implementation::new("acp-test-client", "0.1.0").title(Some("ACP Test Client".to_string())),
            )))
            .await?;

            // Create session
            let session = conn
                .new_session(acp::NewSessionRequest::new(std::env::current_dir()?))
                .await?;

            println!("Session created: {:#?}", session);

            // Interactive prompt loop
            println!("ACP Test Client - Type messages to send to agent (Ctrl+C to exit)");
            println!("Commands: /mode <agent_name> - switch agent");
            loop {
                print!("> ");
                std::io::Write::flush(&mut std::io::stdout())?;

                let mut input = String::new();
                if std::io::stdin().read_line(&mut input)? == 0 {
                    break; // EOF
                }

                let input = input.trim();
                if input.is_empty() {
                    continue;
                }

                // Handle /mode command
                if let Some(mode_id) = input.strip_prefix("/mode ") {
                    let mode_id = mode_id.trim();
                    match conn
                        .set_session_mode(acp::SetSessionModeRequest::new(
                            session.session_id.clone(),
                            acp::SessionModeId::new(mode_id),
                        ))
                        .await
                    {
                        Ok(_) => println!("Switched to agent: {}", mode_id),
                        Err(e) => println!("Failed to switch agent: {}", e),
                    }
                    continue;
                }

                conn.prompt(acp::PromptRequest::new(session.session_id.clone(), vec![
                    acp::ContentBlock::Text(acp::TextContent::new(input.to_string())),
                ]))
                .await?;
            }

            Ok::<(), eyre::Error>(())
        })
        .await?;

    Ok(ExitCode::SUCCESS)
}
