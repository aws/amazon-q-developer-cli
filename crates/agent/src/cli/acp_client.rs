//! Interactive ACP Test Client for testing ACP agents.
//!
//! Usage (from workspace root):
//! ```bash
//! # Build the agent
//! cargo build
//!
//! # Run the interactive test client (from workspace root)
//! cargo run -p agent -- acp-client ./target/debug/agent
//! ```

use std::process::ExitCode;

use agent_client_protocol as acp;
use eyre::Result;
use tokio_util::compat::{
    TokioAsyncReadCompatExt,
    TokioAsyncWriteCompatExt,
};

struct AcpClient;

impl acp::Client for AcpClient {
    async fn session_notification(&self, args: acp::SessionNotification) -> Result<(), acp::Error> {
        match args.update {
            acp::SessionUpdate::AgentMessageChunk { content } => match content {
                acp::ContentBlock::Text(text) => println!("Agent: {}", text.text),
                _ => println!("Agent: <non-text content>"),
            },
            acp::SessionUpdate::ToolCall(tool_call) => {
                println!("🔧 Tool Call: {:#?}", tool_call);
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
    ) -> Result<acp::RequestPermissionResponse, acp::Error> {
        eprintln!("ACP Client received permission request: {:?}", args);
        
        // Auto-approve first option if available
        let option_id = args
            .options
            .first()
            .map(|opt| opt.id.clone())
            .ok_or_else(|| acp::Error::internal_error())?;

        eprintln!("ACP Client auto-approving with option: {:?}", option_id);

        Ok(acp::RequestPermissionResponse {
            outcome: acp::RequestPermissionOutcome::Selected { option_id },
        })
    }

    async fn write_text_file(&self, _args: acp::WriteTextFileRequest) -> Result<(), acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn read_text_file(&self, _args: acp::ReadTextFileRequest) -> Result<acp::ReadTextFileResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }
}

pub async fn execute(agent_path: String) -> Result<ExitCode> {
    let mut child = tokio::process::Command::new(&agent_path)
        .arg("acp")  // Add the acp subcommand
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
            acp::Agent::initialize(&conn, acp::InitializeRequest {
                protocol_version: acp::V1,
                client_capabilities: acp::ClientCapabilities::default(),
            })
            .await?;

            // Create session
            let session = acp::Agent::new_session(&conn, acp::NewSessionRequest {
                mcp_servers: Vec::new(),
                cwd: std::env::current_dir()?,
            })
            .await?;

            // Interactive prompt loop
            println!("ACP Test Client - Type messages to send to agent (Ctrl+C to exit)");
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

                acp::Agent::prompt(&conn, acp::PromptRequest {
                    session_id: session.session_id.clone(),
                    prompt: vec![acp::ContentBlock::Text(acp::TextContent {
                        text: input.to_string(),
                        annotations: None,
                    })],
                })
                .await?;
            }

            Ok::<(), eyre::Error>(())
        })
        .await?;

    Ok(ExitCode::SUCCESS)
}
