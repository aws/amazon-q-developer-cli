//! # ACP Test Harness
//!
//! This module provides infrastructure for testing ACP (Agent Client Protocol) integration
//! with mock LLMs, allowing tests to be written in a natural conversational style.
//!
//! ## Example Test
//!
//! ```rust
//! #[tokio::test]
//! async fn test_hello_world_conversation() -> eyre::Result<()> {
//!     let harness = TestHarness::new().await?
//!         .set_mock_llm(|mut ctx| async move {
//!             // First exchange
//!             if let Some(msg) = ctx.read_user_message().await {
//!                 if msg.contains("Hi, Claude") {
//!                     ctx.respond_to_user("Hi, you! What's your name?".to_string()).await.unwrap();
//!                 }
//!             }
//!             // Second exchange  
//!             if let Some(msg) = ctx.read_user_message().await {
//!                 if msg.contains("Ferris") {
//!                     ctx.respond_to_user("Hi Ferris, I'm Q!".to_string()).await.unwrap();
//!                 }
//!             }
//!         });
//!
//!     let client = harness.into_client().await;
//!     let mut session = client.new_session().await?;
//!     
//!     // First turn: User says "Hi, Claude"
//!     let mut read = session.say_to_agent("Hi, Claude").await?;
//!     let response = read.read_from_agent().await?;
//!     assert_eq!(response.text(), "Hi, you! What's your name?");
//!     
//!     // Second turn: User says "Ferris"  
//!     let mut read = session.say_to_agent("Ferris").await?;
//!     let response = read.read_from_agent().await?;
//!     assert_eq!(response.text(), "Hi Ferris, I'm Q!");
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Why This Design?
//!
//! The main ACP client interface (`acp::ClientSideConnection::prompt()`) blocks until the entire
//! conversation turn is complete. This makes it difficult to write "scripted" tests that feel
//! like natural back-and-forth conversations.
//!
//! This harness solves that by running the ACP client in a separate task with message channels,
//! allowing tests to send messages and read responses in a natural conversational flow.
//!
//! ## Implementation Flow
//!
//! When you call `session.say_to_agent("Hi, Claude")`:
//!
//! 1. **Test Thread**: Creates `ToAgent::Prompt` message, sends to ClientActor
//! 2. **ClientActor Task**: Receives message, calls `client_conn.prompt()` (blocks)
//! 3. **ACP Protocol**: Prompt flows through duplex stream to AgentActor as JSON-RPC
//! 4. **AgentActor Task**: Receives ACP request, forwards to QAgent
//! 5. **QAgent**: Processes prompt through MockLLM script
//! 6. **MockLLM**: Executes script, generates "Hi, you! What's your name?"
//! 7. **Response Flow**: QAgent → AgentActor → ACP session notifications → duplex stream
//! 8. **ACP Library**: Manages byte-level protocol, calls ClientActor callbacks
//! 9. **ClientActor Callbacks**: Convert ACP callbacks to `FromAgent` messages, send to test
//! 10. **Test Thread**: `read.read_from_agent()` receives the response
//!
//! ## Architecture
//!
//! ```text
//! Test Thread                    ClientActor Task              AgentActor Task
//!     │                               │                            │
//!     │ say_to_agent("Hi")            │                            │
//!     ├─────ToAgent::Prompt──────────→│                            │
//!     │                               │                            │
//!     │                               │ acp.prompt() ──ACP/JSON──→ │
//!     │                               :   (blocks)    duplex       │ QAgent.process()
//!     │                               :      │        stream       │ MockLLM.script()
//!     │                               :      │                     │
//!     │                               :      │                     │
//!     │                               :      │ ←───ACP/JSON────────│ notifications
//!     │ read_from_agent()             :      │                     │ are sent back
//!     │ ←────FromAgent::Response──────:──────│ callbacks           │ with streaming
//!     │                               :      │ push events a       │ text from agent
//!     |                               :      │ tokio channel       │
//!     |                               :      │                     │ 
//!     │ read_from_agent()             :      │ ←───ACP/JSON────────│
//!     │ ←────FromAgent::Response──────:──────│
//!     │                               │
//!     │ read_from_agent()             │
//!     │ ←────FromAgent::Response──────│ final "stop" is sent when all done
//! ```
//!
//! The key insight is that while `client_conn.prompt()` is blocked waiting for the turn to
//! complete, the ACP library is actively managing the protocol and calling back into the
//! `AcpTestClientActorCallbacks`. These callbacks convert ACP events (session notifications,
//! file operations, etc.) into `FromAgent` messages that get sent to the test thread.
//!
//! ## Architecture
//!
//! ```text
//! Test Thread                    ClientActor Task              AgentActor Task
//!     │                               │                            │
//!     │ say_to_agent("Hi")            │                            │
//!     ├─────ToAgent::Prompt──────────→│                            │
//!     │                               │ acp.prompt() ──duplex──→   │
//!     │                               │     (blocks)               │ QAgent.process()
//!     │                               │                            │ MockLLM.script()
//!     │                               │ ←──ACP response────────────│
//!     │ read_from_agent()             │                            │
//!     │←────FromAgent::Response───────│                            │
//! ```
//!
//! ## Key Components
//!
//! - **TestHarness**: Configures mock environment and spawns actor tasks
//! - **AcpTestClient/Session**: High-level conversational API for tests
//! - **ToAgent/FromAgent**: Message types for cross-task communication
//! - **ClientActor**: Manages `acp::ClientSideConnection` in separate task
//! - **AgentActor**: Manages `QAgent` and handles ACP server protocol
//! - **SessionsMap**: Routes messages to correct session event channels

use agent_client_protocol::{self as acp, Agent, NewSessionRequest, NewSessionResponse, PromptRequest, Client};
use futures::{AsyncRead, AsyncWrite};
use parking_lot::Mutex;
use std::{collections::{BTreeMap, HashMap}, path::PathBuf, process::ExitCode, sync::Arc};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::{cli::acp::{AcpArgs, QAgent}, database::settings::Setting, os::Os};

pub(crate) struct TestHarness {
    os: Os,
}

impl TestHarness {
    pub async fn new() -> eyre::Result<Self> {
        Ok(TestHarness {
            os: Os::new().await?,
        })
    }

    /// Provide the "script" for the LLM.
    /// You define this script with a rust function with the ability to read what the user wrote and send back responses.
    pub fn set_mock_llm<F>(mut self, script: impl FnOnce(crate::mock_llm::MockLLMContext) -> F) -> Self
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.os.client.set_mock_llm(script);
        self
    }

    /// Launch the test and get a handle to the client that you can use to communicate with the agent.
    /// This will communicate via the ACP implementation.
    pub async fn into_client(self) -> eyre::Result<AcpTestClient> {
        // Create duplex streams for communication
        let (client_write, agent_read) = tokio::io::duplex(1024);
        let (agent_write, client_read) = tokio::io::duplex(1024);
        
        // Use the spawnable server with custom streams
        let _handle = super::spawn_acp_server_with_streams(
            "test-agent".to_string(),
            self.os,
            agent_write.compat_write(),
            agent_read.compat(),
        ).await?;
        
        // Start the client actor
        spawn_test_client_actor(
            client_write.compat_write(),
            client_read.compat(),
        ).await
    }
}

pub struct AcpTestClient {
    client_tx: tokio::sync::mpsc::Sender<ToAgent>,
}

impl AcpTestClient {
    /// Initiative a new session.
    pub async fn new_session(&self) -> eyre::Result<AcpTestSession> {
        let (event_tx, event_rx) = tokio::sync::mpsc::channel(128);
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.client_tx.send(ToAgent::NewSession { 
            request: NewSessionRequest { 
                cwd: std::env::current_dir()?, 
                mcp_servers: vec![], 
                meta: None 
            }, 
            event_tx, 
            response_tx 
        }).await?;
        let acp::NewSessionResponse { session_id, .. } = response_rx.await??;
        Ok(AcpTestSession { session_id, client_tx: self.client_tx.clone(), event_rx })
    }

}

pub struct AcpTestSession {
    session_id: acp::SessionId,
    client_tx: tokio::sync::mpsc::Sender<ToAgent>,
    event_rx: tokio::sync::mpsc::Receiver<FromAgent>,
}

impl AcpTestSession {
    /// Say something to the agent. You get back a `AcpTestSessionRead`
    /// that will allow you to read responses. You cannot use the session again
    /// until you stop using that.
    pub async fn say_to_agent<'s>(&'s mut self, message: impl IntoPromptContent) -> eyre::Result<AcpTestSessionRead<'s>> {
        let request = acp::PromptRequest {
            session_id: self.session_id.clone(),
            prompt: message.into_prompt_content(),
            meta: None,
        };
        self.client_tx.send(ToAgent::Prompt { request }).await?;
        Ok(AcpTestSessionRead { session: self })
    }
}

pub struct AcpTestSessionRead<'r> {
    session: &'r mut AcpTestSession,
}

impl AcpTestSessionRead<'_> {
    /// Read the next message from the agent, blocking until one arrives (or erroring if agent has terminated).
    pub async fn read_from_agent(&mut self) -> eyre::Result<FromAgent> {
        self.session.event_rx.recv().await.ok_or_else(|| eyre::eyre!("agent terminated"))
    }
    
    /// Read session notifications (agent responses) until the turn stops
    pub async fn read_agent_response(&mut self) -> eyre::Result<String> {
        let mut response_text = String::new();
        
        loop {
            match self.read_from_agent().await? {
                FromAgent::SessionNotification(notification, response_tx) => {
                    // Send acknowledgment
                    let _ = response_tx.send(());
                    
                    // Extract text from session notification
                    match notification.update {
                        acp::SessionUpdate::AgentMessageChunk { content } => {
                            match content {
                                acp::ContentBlock::Text(text_content) => {
                                    response_text.push_str(&text_content.text);
                                }
                                _ => {} // Ignore non-text content for now
                            }
                        }
                        _ => {} // Ignore other update types for now
                    }
                }
                FromAgent::Stop(result) => {
                    result?; // Propagate any errors
                    break;
                }
                _ => {} // Ignore other message types for now
            }
        }
        
        Ok(response_text)
    }
}

pub enum AcpTestClientRequest {
    PromptWithText(String),
}

/// Messages that can be sent to the (mock'd) agent
pub enum ToAgent {
    /// Initiate a new session.
    NewSession {
        /// Request details
        request: NewSessionRequest,

        /// Where to send events that occur related to this session
        event_tx: tokio::sync::mpsc::Sender<FromAgent>,

        /// Where to send the response with the session-id
        response_tx: tokio::sync::oneshot::Sender<eyre::Result<acp::NewSessionResponse>>,
    },

    /// Send a prompt. Responses will be sent to the event-tx for that session.
    Prompt {
        /// Prompt to send.
        request: PromptRequest,
    },
}

/// Messages that can be received from the (mock'd) agent
pub enum FromAgent {
    RequestPermission(acp::RequestPermissionRequest, tokio::sync::oneshot::Sender<acp::RequestPermissionResponse>),
    WriteTextFile(acp::WriteTextFileRequest, tokio::sync::oneshot::Sender<acp::WriteTextFileResponse>),
    ReadTextFile(acp::ReadTextFileRequest, tokio::sync::oneshot::Sender<acp::ReadTextFileResponse>),
    CreateTerminal(acp::CreateTerminalRequest, tokio::sync::oneshot::Sender<acp::CreateTerminalResponse>),
    TerminalOutput(acp::TerminalOutputRequest, tokio::sync::oneshot::Sender<acp::TerminalOutputResponse>),
    ReleaseTerminal(acp::ReleaseTerminalRequest, tokio::sync::oneshot::Sender<acp::ReleaseTerminalResponse>),
    WaitForTerminalExit(acp::WaitForTerminalExitRequest, tokio::sync::oneshot::Sender<acp::WaitForTerminalExitResponse>),
    KillTerminalCommand(acp::KillTerminalCommandRequest, tokio::sync::oneshot::Sender<acp::KillTerminalCommandResponse>),
    SessionNotification(acp::SessionNotification, tokio::sync::oneshot::Sender<()>),
    Stop(Result<acp::PromptResponse, acp::Error>),
}

/// Map from active session-ids to the "sender" associated with that session-id.
type SessionsMap = Arc<Mutex<HashMap<acp::SessionId, tokio::sync::mpsc::Sender<FromAgent>>>>;

async fn spawn_test_client_actor(
    outgoing_bytes: impl Unpin + AsyncWrite + Send + 'static,
    incoming_bytes: impl Unpin + AsyncRead + Send + 'static,
) -> eyre::Result<AcpTestClient> {
    let sessions: SessionsMap = Default::default();

    let (client_conn, client_handle_io) = acp::ClientSideConnection::new(
        AcpTestClientActorCallbacks { sessions: sessions.clone() },
        outgoing_bytes,
        incoming_bytes,
        |fut| { tokio::task::spawn_local(fut); }
    );

    // Start I/O handler in LocalSet - use spawn_local instead of spawn
    tokio::task::spawn_local(async move {
        client_handle_io.await
    });

    let (client_tx, mut client_rx) = tokio::sync::mpsc::channel(128);
        
    tokio::task::spawn_local(async move {
        // Initialize the connection first
        if let Err(e) = client_conn.initialize(acp::InitializeRequest {
            protocol_version: acp::V1,
            client_capabilities: acp::ClientCapabilities::default(),
            meta: None,
        }).await {
            eprintln!("Failed to initialize ACP connection: {}", e);
            return;
        }

        while let Some(message) = client_rx.recv().await {
            match message {
                ToAgent::NewSession { request, event_tx, response_tx } => {
                    let closure = async || -> eyre::Result<NewSessionResponse>  {
                        let response = client_conn.new_session(request).await?;
                        sessions.lock().insert(response.session_id.clone(), event_tx);
                        Ok(response)
                    };
                    let _ = response_tx.send(closure().await);
                }

                ToAgent::Prompt { request } => {
                    let session_tx = sessions.lock().get(&request.session_id).cloned();
                    if let Some(session_tx) = session_tx {
                        match client_conn.prompt(request).await {
                            Ok(result) => {
                                let _ = session_tx.send(FromAgent::Stop(Ok(result))).await;
                            }
                            Err(e) => {
                                let _ = session_tx.send(FromAgent::Stop(Err(e))).await;
                            }
                        }
                    }
                }
            }
        }
    });

    Ok(AcpTestClient { client_tx })
}

struct AcpTestClientActorCallbacks {
    sessions: SessionsMap,
}

impl AcpTestClientActorCallbacks {
    fn session_tx(&self, session_id: &acp::SessionId) -> Result<tokio::sync::mpsc::Sender<FromAgent>, acp::Error> {
        match self.sessions.lock().get(session_id) {
            Some(tx) =>  Ok(tx.clone()),
            None => Err(acp::Error {
                code: 22,
                message: format!("no tx for session-id {session_id:?} found"),
                data: None,
            }),
        }
    }

    async fn send_and_await_reply<M, R>(&self, session_id: &acp::SessionId, message: impl FnOnce(M, tokio::sync::oneshot::Sender<R>) -> FromAgent, args: M) -> Result<R, acp::Error> {
        let session_tx = self.session_tx(session_id)?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        session_tx.send(message(args, tx)).await.map_err(|e| acp::Error {
            code: 22,
            message: e.to_string(),
            data: None,
        })?;
        let response = rx.await.map_err(|e| acp::Error {
            code: 22,
            message: e.to_string(),
            data: None,
        })?;
        Ok(response)
    }
}

impl acp::Client for AcpTestClientActorCallbacks {
    async fn request_permission(
        &self,
        args: acp::RequestPermissionRequest,
    ) -> Result<acp::RequestPermissionResponse, acp::Error> {
        self.send_and_await_reply(&args.session_id.clone(), FromAgent::RequestPermission, args).await
    }
        
    // Claude: fill in the rest of these methods in a similar pattern to the one above

    async fn write_text_file(
        &self,
        args: acp::WriteTextFileRequest,
    ) -> Result<acp::WriteTextFileResponse, acp::Error> {
        self.send_and_await_reply(&args.session_id.clone(), FromAgent::WriteTextFile, args).await
    }
    
    async fn read_text_file(
        &self,
        args: acp::ReadTextFileRequest,
    ) -> Result<acp::ReadTextFileResponse, acp::Error> {
        self.send_and_await_reply(&args.session_id.clone(), FromAgent::ReadTextFile, args).await
    }
    
    async fn session_notification(
        &self,
        args: acp::SessionNotification,
    ) -> Result<(), acp::Error> {
        self.send_and_await_reply(&args.session_id.clone(), FromAgent::SessionNotification, args).await
    }
    
    async fn create_terminal(
        &self,
        args: acp::CreateTerminalRequest,
    ) -> Result<acp::CreateTerminalResponse, acp::Error> {
        self.send_and_await_reply(&args.session_id.clone(), FromAgent::CreateTerminal, args).await
    }
    
    async fn terminal_output(
        &self,
        args: acp::TerminalOutputRequest,
    ) -> Result<acp::TerminalOutputResponse, acp::Error> {
        self.send_and_await_reply(&args.session_id.clone(), FromAgent::TerminalOutput, args).await
    }
    
    async fn release_terminal(
        &self,
        args: acp::ReleaseTerminalRequest,
    ) -> Result<acp::ReleaseTerminalResponse, acp::Error> {
        self.send_and_await_reply(&args.session_id.clone(), FromAgent::ReleaseTerminal, args).await
    }
    
    async fn wait_for_terminal_exit(
        &self,
        args: acp::WaitForTerminalExitRequest,
    ) -> Result<acp::WaitForTerminalExitResponse, acp::Error> {
        self.send_and_await_reply(&args.session_id.clone(), FromAgent::WaitForTerminalExit, args).await
    }
    
    async fn kill_terminal_command(
        &self,
        args: acp::KillTerminalCommandRequest,
    ) -> Result<acp::KillTerminalCommandResponse, acp::Error> {
        self.send_and_await_reply(&args.session_id.clone(), FromAgent::KillTerminalCommand, args).await
    }
    
    async fn ext_method(
        &self,
        _method: std::sync::Arc<str>,
        _params: std::sync::Arc<serde_json::value::RawValue>,
    ) -> Result<std::sync::Arc<serde_json::value::RawValue>, acp::Error> {
        Err(acp::Error::method_not_found())
    }
    
    async fn ext_notification(
        &self,
        _method: std::sync::Arc<str>,
        _params: std::sync::Arc<serde_json::value::RawValue>,
    ) -> Result<(), acp::Error> {
        Err(acp::Error::method_not_found())
    }
}

pub trait IntoPromptContent {
    fn into_prompt_content(self) -> Vec<acp::ContentBlock>;
}

impl IntoPromptContent for String {
    fn into_prompt_content(self) -> Vec<acp::ContentBlock> {
        vec![
            acp::ContentBlock::Text(acp::TextContent { annotations: None, text: self, meta: None })
        ]
    }
}

impl IntoPromptContent for &str {
    fn into_prompt_content(self) -> Vec<acp::ContentBlock> {
        self.to_string().into_prompt_content()
    }
}
