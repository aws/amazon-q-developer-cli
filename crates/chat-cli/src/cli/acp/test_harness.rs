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

use agent_client_protocol::{self as acp, Agent, NewSessionRequest, NewSessionResponse, PromptRequest};
use futures::{AsyncRead, AsyncWrite};
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::os::Os;

/// Entry point for setting up ACP (Agent Client Protocol) tests.
/// 
/// The test harness creates a complete client-server test environment:
/// - Spawns a Q agent server using in-memory duplex streams
/// - Sets up a client actor to handle ACP protocol communication
/// - Provides a high-level API for testing agent interactions
/// 
/// # Architecture
/// ```text
/// Test Code → AcpTestClient → Client Actor → Duplex Stream → Q Agent Server
/// ```
/// 
/// # Usage
/// ```rust
/// let client = TestHarness::new()
///     .await?
///     .set_mock_llm(|ctx| async move {
///         // Mock LLM responses for testing
///     })
///     .into_client()
///     .await?;
/// ```
/// 
/// # Mock LLM
/// Use `set_mock_llm()` to provide scripted responses instead of calling
/// the real LLM service. This makes tests deterministic and fast.
pub(crate) struct TestHarness {
    /// Operating system interface with mock LLM capabilities
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
        
        // Use the spawnable server with custom streams - KEEP HANDLE ALIVE
        let server_handle = super::spawn_acp_server_with_streams(
            "test-agent".to_string(),
            self.os,
            agent_write.compat_write(),
            agent_read.compat(),
        ).await?;
        
        // Start the client actor
        let client_tx = spawn_test_client_actor(
            client_write.compat_write(),
            client_read.compat(),
        ).await?;
        
        // Store server handle in client to keep it alive for the test duration
        Ok(AcpTestClient {
            client_tx,
            _server_handle: server_handle,
        })
    }
}

/// Client interface for testing ACP (Agent Client Protocol) communication.
/// 
/// This represents the "client side" of the ACP protocol in tests. It communicates
/// with a spawned Q agent server via in-memory duplex streams instead of stdio.
/// 
/// The client sends `ToAgent` messages to a background client actor task, which
/// handles the actual ACP protocol communication with the server.
/// 
/// # Usage
/// ```rust
/// let client = TestHarness::new().await?.into_client().await?;
/// let mut session = client.new_session().await?;
/// ```
pub struct AcpTestClient {
    /// Channel to send messages to the client actor task.
    /// The client actor handles the actual ACP protocol communication.
    client_tx: tokio::sync::mpsc::Sender<ToAgent>,
    /// Handle to the ACP server. Keeping this alive prevents premature shutdown.
    _server_handle: super::AcpServerHandle,
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

/// Represents an active conversation session with the Q agent in tests.
/// 
/// Each session has a unique ID and maintains its own message channel for
/// receiving responses from the agent. Sessions are created via `AcpTestClient::new_session()`.
/// 
/// # Lifecycle
/// 1. Create session with `client.new_session()`
/// 2. Send messages with `session.say_to_agent(message)`
/// 3. Read responses with the returned `AcpTestSessionRead`
/// 
/// # Example
/// ```rust
/// let mut session = client.new_session().await?;
/// let mut read = session.say_to_agent("Hello!").await?;
/// let response = read.read_from_agent().await?;
/// ```
pub struct AcpTestSession {
    /// Unique identifier for this conversation session
    session_id: acp::SessionId,
    /// Channel to send messages to the client actor
    client_tx: tokio::sync::mpsc::Sender<ToAgent>,
    /// Channel to receive responses from the agent for this session
    event_rx: tokio::sync::mpsc::Receiver<FromAgent>,
}

impl AcpTestSession {
    /// Say something to the agent. You get back a `AcpTestSessionRead`
    /// that will allow you to read responses. You cannot use the session again
    /// until you stop using that.
    pub async fn say_to_agent<'s>(&'s mut self, message: impl IntoPromptContent) -> eyre::Result<AcpTestSessionRead<'s>> {
        eprintln!("DEBUG: say_to_agent called with message");
        let request = acp::PromptRequest {
            session_id: self.session_id.clone(),
            prompt: message.into_prompt_content(),
            meta: None,
        };
        eprintln!("DEBUG: Sending ToAgent::Prompt");
        self.client_tx.send(ToAgent::Prompt { request }).await?;
        eprintln!("DEBUG: ToAgent::Prompt sent successfully");
        Ok(AcpTestSessionRead { session: self })
    }
}

/// Handle for reading agent responses after sending a message.
/// 
/// This struct borrows the session mutably, preventing you from sending
/// additional messages until you're done reading the current response.
/// This enforces a request-response pattern in tests.
/// 
/// # Response Types
/// - `FromAgent::SessionNotification`: Streaming content chunks from the agent
/// - `FromAgent::Stop`: Final response indicating the agent is done
/// - Other variants: Tool calls, file operations, etc.
/// 
/// # Example
/// ```rust
/// let mut read = session.say_to_agent("Hello").await?;
/// 
/// // Read streaming responses
/// loop {
///     match read.read_from_agent().await? {
///         FromAgent::SessionNotification(notif, _) => {
///             // Handle streaming content
///         }
///         FromAgent::Stop(result) => {
///             // Agent finished responding
///             break;
///         }
///         _ => {
///             // Handle other message types
///         }
///     }
/// }
/// ```
pub struct AcpTestSessionRead<'r> {
    /// Mutable reference to the session, preventing concurrent message sending
    session: &'r mut AcpTestSession,
}

impl AcpTestSessionRead<'_> {
    /// Read the next message from the agent, blocking until one arrives (or erroring if agent has terminated).
    pub async fn read_from_agent(&mut self) -> eyre::Result<FromAgent> {
        eprintln!("AcpTestSessionRead::read_from_agent(): read_from_agent called, waiting for response");
        let result = self.session.event_rx.recv().await.ok_or_else(|| eyre::eyre!("agent terminated"));
        match &result {
            Ok(msg) => {
                match msg {
                    FromAgent::SessionNotification(..) => eprintln!("AcpTestSessionRead::read_from_agent(): Received SessionNotification"),
                    FromAgent::Stop(_) => eprintln!("AcpTestSessionRead::read_from_agent(): Received Stop"),
                    _ => eprintln!("AcpTestSessionRead::read_from_agent(): Received other message type"),
                }
            }
            Err(e) => eprintln!("AcpTestSessionRead::read_from_agent(): Error receiving from agent: {}", e),
        }
        result
    }
    
    /// Read session notifications (agent responses) until the turn stops
    pub async fn read_agent_response(&mut self) -> eyre::Result<String> {
        let mut response_text = String::new();
        
        loop {
            let result = self.read_from_agent().await;
            eprintln!("read_agent_response() = (is_ok={:?}, is_err={:?})", result.is_ok(), result.is_err());
            match result? {
                FromAgent::SessionNotification(notification, response_tx) => {
                    eprintln!("notification = {notification:?}");

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
                    eprintln!("stop");
                    result?; // Propagate any errors
                    break;
                }
                _ => {
                    eprintln!("ignoring other message type");
                } // Ignore other message types for now
            }
        }
        
        Ok(response_text)
    }
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
    #[expect(dead_code)] // <-- fields not currently used
    RequestPermission(acp::RequestPermissionRequest, tokio::sync::oneshot::Sender<acp::RequestPermissionResponse>),

    #[expect(dead_code)] // <-- fields not currently used
    WriteTextFile(acp::WriteTextFileRequest, tokio::sync::oneshot::Sender<acp::WriteTextFileResponse>),

    #[expect(dead_code)] // <-- fields not currently used
    ReadTextFile(acp::ReadTextFileRequest, tokio::sync::oneshot::Sender<acp::ReadTextFileResponse>),

    #[expect(dead_code)] // <-- fields not currently used
    CreateTerminal(acp::CreateTerminalRequest, tokio::sync::oneshot::Sender<acp::CreateTerminalResponse>),

    #[expect(dead_code)] // <-- fields not currently used
    TerminalOutput(acp::TerminalOutputRequest, tokio::sync::oneshot::Sender<acp::TerminalOutputResponse>),

    #[expect(dead_code)] // <-- fields not currently used
    ReleaseTerminal(acp::ReleaseTerminalRequest, tokio::sync::oneshot::Sender<acp::ReleaseTerminalResponse>),

    #[expect(dead_code)] // <-- fields not currently used
    WaitForTerminalExit(acp::WaitForTerminalExitRequest, tokio::sync::oneshot::Sender<acp::WaitForTerminalExitResponse>),

    #[expect(dead_code)] // <-- fields not currently used
    KillTerminalCommand(acp::KillTerminalCommandRequest, tokio::sync::oneshot::Sender<acp::KillTerminalCommandResponse>),

    SessionNotification(acp::SessionNotification, tokio::sync::oneshot::Sender<()>),

    Stop(Result<acp::PromptResponse, acp::Error>),
}

/// Map from active session-ids to the "sender" associated with that session-id.
type SessionsMap = Arc<Mutex<HashMap<acp::SessionId, tokio::sync::mpsc::Sender<FromAgent>>>>;

async fn spawn_test_client_actor(
    outgoing_bytes: impl Unpin + AsyncWrite + Send + 'static,
    incoming_bytes: impl Unpin + AsyncRead + Send + 'static,
) -> eyre::Result<tokio::sync::mpsc::Sender<ToAgent>> {
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
                    eprintln!("DEBUG: ClientActor received ToAgent::NewSession");
                    let closure = async || -> eyre::Result<NewSessionResponse>  {
                        let response = client_conn.new_session(request).await?;
                        sessions.lock().insert(response.session_id.clone(), event_tx);
                        Ok(response)
                    };
                    let result = closure().await;
                    eprintln!("DEBUG: NewSession result: {:?}", result.is_ok());
                    let _ = response_tx.send(result);
                }

                ToAgent::Prompt { request } => {
                    eprintln!("DEBUG: ClientActor received ToAgent::Prompt for session: {:?}", request.session_id);
                    let session_tx = sessions.lock().get(&request.session_id).cloned();
                    if let Some(session_tx) = session_tx {
                        eprintln!("DEBUG: Found session, calling client_conn.prompt()");
                        match client_conn.prompt(request).await {
                            Ok(result) => {
                                eprintln!("DEBUG: client_conn.prompt() succeeded {result:?}, sending Stop(Ok)");
                                let _ = session_tx.send(FromAgent::Stop(Ok(result))).await;
                            }
                            Err(e) => {
                                eprintln!("DEBUG: client_conn.prompt() failed: {:?}", e);
                                let _ = session_tx.send(FromAgent::Stop(Err(e))).await;
                            }
                        }
                    } else {
                        eprintln!("DEBUG: Session not found for ID: {:?}", request.session_id);
                    }
                }
            }
        }
    });

    Ok(client_tx)
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

    async fn send_and_await_reply<M: std::fmt::Debug, R: std::fmt::Debug>(&self, session_id: &acp::SessionId, message: impl FnOnce(M, tokio::sync::oneshot::Sender<R>) -> FromAgent, args: M) -> Result<R, acp::Error> {
        eprintln!("send_and_await_reply(session_id={session_id:?}, args={args:?})");
        let session_tx = self.session_tx(session_id)?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        session_tx.send(message(args, tx)).await.map_err(|e| acp::Error {
            code: 22,
            message: e.to_string(),
            data: None,
        })?;
        eprintln!("send_and_await_reply: awaiting response");
        let response = rx.await.map_err(|e| acp::Error {
            code: 22,
            message: e.to_string(),
            data: None,
        })?;
        eprintln!("send_and_await_reply: response={response:?}");
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
        dbg!(self.send_and_await_reply(&args.session_id.clone(), FromAgent::SessionNotification, args).await)
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
