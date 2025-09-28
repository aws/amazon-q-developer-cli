use std::sync::Arc;
use agent_client_protocol as acp;
use anyhow::Result;
use serde_json::value::RawValue;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

struct SimpleClient;

impl acp::Client for SimpleClient {
    async fn request_permission(
        &self,
        args: acp::RequestPermissionRequest,
    ) -> Result<acp::RequestPermissionResponse, acp::Error> {
        println!("Permission requested: {:?}", args);
        Ok(acp::RequestPermissionResponse {
            outcome: acp::RequestPermissionOutcome::Selected {
                option_id: acp::PermissionOptionId(Arc::from("allow-once")),
            },
            meta: None,
        })
    }

    async fn write_text_file(
        &self,
        args: acp::WriteTextFileRequest,
    ) -> Result<acp::WriteTextFileResponse, acp::Error> {
        println!("Write file: {:?}", args.path);
        Ok(acp::WriteTextFileResponse { meta: None })
    }

    async fn read_text_file(
        &self,
        args: acp::ReadTextFileRequest,
    ) -> Result<acp::ReadTextFileResponse, acp::Error> {
        println!("Read file: {:?}", args.path);
        Ok(acp::ReadTextFileResponse {
            content: "Hello from file!".to_string(),
            meta: None,
        })
    }

    async fn create_terminal(
        &self,
        _args: acp::CreateTerminalRequest,
    ) -> Result<acp::CreateTerminalResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn terminal_output(
        &self,
        _args: acp::TerminalOutputRequest,
    ) -> Result<acp::TerminalOutputResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn release_terminal(
        &self,
        _args: acp::ReleaseTerminalRequest,
    ) -> Result<acp::ReleaseTerminalResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn wait_for_terminal_exit(
        &self,
        _args: acp::WaitForTerminalExitRequest,
    ) -> Result<acp::WaitForTerminalExitResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn kill_terminal_command(
        &self,
        _args: acp::KillTerminalCommandRequest,
    ) -> Result<acp::KillTerminalCommandResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn session_notification(&self, args: acp::SessionNotification) -> Result<(), acp::Error> {
        match args.update {
            acp::SessionUpdate::AgentMessageChunk { content } => {
                let text = match content {
                    acp::ContentBlock::Text(text_content) => text_content.text,
                    _ => "<non-text>".to_string(),
                };
                println!("Agent: {}", text);
            }
            _ => {
                println!("Other update: {:?}", args.update);
            }
        }
        Ok(())
    }

    async fn ext_method(
        &self,
        _method: Arc<str>,
        _params: Arc<RawValue>,
    ) -> Result<Arc<RawValue>, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn ext_notification(
        &self,
        _method: Arc<str>,
        _params: Arc<RawValue>,
    ) -> Result<(), acp::Error> {
        Err(acp::Error::method_not_found())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("Starting ACP client test...");
    
    // Start Q CLI in ACP mode as subprocess
    let mut child = tokio::process::Command::new("cargo")
        .args(&["run", "--bin", "chat_cli", "--", "acp"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    
    let stdin = child.stdin.take().unwrap().compat_write();
    let stdout = child.stdout.take().unwrap().compat();
    
    let local_set = tokio::task::LocalSet::new();
    let result = local_set.run_until(async move {
        // Set up client connection
        let (client_conn, client_handle_io) = acp::ClientSideConnection::new(
            SimpleClient,
            stdin,
            stdout,
            |fut| { tokio::task::spawn_local(fut); }
        );
        
        // Start I/O handler
        tokio::task::spawn_local(client_handle_io);
        
        println!("Initializing ACP protocol...");
        
        // Initialize protocol
        use acp::Agent;
        let init_response = client_conn.initialize(acp::InitializeRequest {
            protocol_version: acp::V1,
            client_capabilities: acp::ClientCapabilities::default(),
            meta: None,
        }).await?;
        
        println!("Initialized! Protocol version: {:?}", init_response.protocol_version);
        
        // Create session
        println!("Creating session...");
        let session_response = client_conn.new_session(acp::NewSessionRequest {
            mcp_servers: Vec::new(),
            cwd: std::env::current_dir()?,
            meta: None,
        }).await?;
        
        println!("Session created: {:?}", session_response.session_id);
        
        // Send a message
        println!("Sending message: 'Hello, Q!'");
        let prompt_response = client_conn.prompt(acp::PromptRequest {
            session_id: session_response.session_id.clone(),
            prompt: vec![acp::ContentBlock::Text(acp::TextContent {
                annotations: None,
                text: "Hello, Q!".to_string(),
                meta: None,
            })],
            meta: None,
        }).await?;
        
        println!("Prompt response: {:?}", prompt_response.stop_reason);
        
        // Wait a bit for any streaming responses
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        
        println!("Test completed successfully!");
        
        Ok::<(), anyhow::Error>(())
    }).await;
    
    match result {
        Ok(_) => println!("ACP client test passed!"),
        Err(e) => println!("ACP client test failed: {}", e),
    }
    
    Ok(())
}
