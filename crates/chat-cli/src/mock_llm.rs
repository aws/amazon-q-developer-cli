#![cfg_attr(not(test), allow(dead_code))] // outside of test code, parts of Mock LLM are unused

use serde_json::Value;
use std::future::Future;
use tokio::sync::mpsc;
use crate::api_client::model::ChatResponseStream;

/// Spawn a mock LLM that executes a custom Rust script for testing.
///
/// Creates a mock LLM that runs the provided script in a separate tokio task. The script
/// receives a [`MockLLMContext`] for bidirectional communication with the Q CLI system.
/// This allows testing complex conversation flows with full programmatic control.
///
/// ## Usage
///
/// ```rust,ignore
/// let mock_llm = spawn_mock_llm(|mut context| async move {
///     while let Some(user_msg) = context.read_user_message().await {
///         if user_msg.contains("Greece") {
///             context.respond_to_user("The capital is Athens".to_string()).await.unwrap();
///         } else {
///             context.call_tool("1".to_string(), "search".to_string(), 
///                              Some(json!({"query": user_msg})), None).await.unwrap();
///         }
///     }
/// });
/// ```
///
/// ## Communication Model
///
/// - **Input**: User messages sent via [`MockLLM::send_user_message`]
/// - **Output**: LLM responses read via [`MockLLM::read_llm_response`]
/// - **Script**: Runs asynchronously, processes messages and generates responses
///
/// The script continues running until the context channels are dropped or the script completes.
pub fn spawn_mock_llm<F>(script: impl FnOnce(MockLLMContext) -> F) -> MockLLM
where
    F: Future<Output = ()> + Send + 'static,
{
    let (user_input_tx, user_input_rx) = mpsc::channel(1);
    let (llm_response_tx, llm_response_rx) = mpsc::unbounded_channel();

    let context = MockLLMContext {
        user_input_rx,
        llm_response_tx,
    };

    tokio::spawn(script(context));

    tracing::debug!(actor="mock_llm", event="spawned");

    MockLLM {
        user_input_tx,
        llm_response_rx,
    }
}

/// Handle for communicating with a spawned mock LLM script.
///
/// Provides the interface for sending user messages to the mock LLM script and receiving
/// its responses. The actual LLM logic runs in a separate tokio task created by
/// [`spawn_mock_llm`].
///
/// ## Lifecycle
///
/// 1. Send user messages via [`send_user_message`](Self::send_user_message)
/// 2. Read LLM responses via [`read_llm_response`](Self::read_llm_response)
/// 3. Responses can be text or tool calls depending on the script logic
/// 4. Communication continues until channels are closed or script completes
///
/// ## Response Types
///
/// The mock LLM can generate:
/// - **Text responses**: Direct assistant messages
/// - **Tool calls**: Requests to execute tools with arguments
/// - **Mixed flows**: Combinations of text and tool usage
///
/// This matches the behavior of real LLM APIs for comprehensive testing.
#[derive(Debug)]
pub struct MockLLM {
    user_input_tx: mpsc::Sender<String>,
    llm_response_rx: mpsc::UnboundedReceiver<ChatResponseStreamOrEndTurn>,
}

enum ChatResponseStreamOrEndTurn {
    Message(ChatResponseStream),
    EndTurn,
}

impl MockLLM {
    /// Convey the user's message to the script.
    pub async fn send_user_message(&mut self, text: String) -> Result<(), mpsc::error::SendError<String>> {
        tracing::debug!(actor="mock_llm", event="send_user_message", message_len=text.len());
        let result = self.user_input_tx.send(text).await;
        if result.is_err() {
            tracing::debug!(actor="mock_llm", event="send_user_message failed");
        }
        result
    }

    /// Read the response from the LLM (could be text or tool call).
    /// If `None` is returned, that indicates that the end of the turn has been reached.
    pub async fn read_llm_response(&mut self) -> Option<ChatResponseStream> {
        match self.llm_response_rx.recv().await {
            // The closure has fully terminated, dropping the tx side, so that implies the turn is over
            None => {
                tracing::debug!(actor="mock_llm", event="read_llm_response", result="channel_closed");
                None
            }

            // Closure ended the turn.
            Some(ChatResponseStreamOrEndTurn::EndTurn) => {
                tracing::debug!(actor="mock_llm", event="read_llm_response", result="end_turn");
                None
            }

            // Closure generates a concrete message.
            Some(ChatResponseStreamOrEndTurn::Message(m)) => {
                tracing::debug!(actor="mock_llm", event="read_llm_response", result="message", ?m);
                Some(m)
            }
        }
    }
}

/// Context provided to mock LLM scripts for bidirectional communication.
///
/// This is the primary interface that mock LLM scripts use to interact with the Q CLI
/// system. Scripts receive this context and use it to:
///
/// - **Read user input**: Via [`read_user_message`](Self::read_user_message)
/// - **Send responses**: Via [`respond_to_user`](Self::respond_to_user)
/// - **Make tool calls**: Via [`call_tool`](Self::call_tool)
/// - **Internal tool logic**: Via [`invoke_tool`](Self::invoke_tool)
///
/// ## Communication Pattern
///
/// ```rust,ignore
/// async move {
///     while let Some(user_msg) = context.read_user_message().await {
///         // Process user message
///         if needs_tool_call(&user_msg) {
///             context.call_tool("1", "search", Some(args), None).await?;
///         } else {
///             context.respond_to_user(generate_response(&user_msg)).await?;
///         }
///     }
/// }
/// ```
///
/// ## Channel-Based Architecture
///
/// Uses tokio mpsc channels for async communication between the script task and the
/// main Q CLI process. This allows scripts to run independently while maintaining
/// responsive communication.
pub struct MockLLMContext {
    user_input_rx: mpsc::Receiver<String>,
    llm_response_tx: mpsc::UnboundedSender<ChatResponseStreamOrEndTurn>,
}

impl MockLLMContext {
    /// Read the next user message from the channel.
    pub async fn read_user_message(&mut self) -> Option<MockLLMContextTurn<'_>> {
        let user_message = self.user_input_rx.recv().await?;
        tracing::debug!(actor="mock_llm_context", event="turn_begins", message_len=user_message.len());
        Some(MockLLMContextTurn { user_message, cx: self })
    }
}

/// Indicates a "turn" of the conversation. A turn begins with a message from the user.
/// The LLM can send some number of messages back during the turn.
/// The turn ends when this struct is dropped. At that point, the user should response.
pub struct MockLLMContextTurn<'c> {
    user_message: String,
    cx: &'c mut MockLLMContext,
}

impl MockLLMContextTurn<'_> {
    /// The message the user wrote
    pub fn user_message(&self) -> &str {
        &self.user_message
    }

    /// Send a text response back to the user via channel
    pub async fn respond_to_user(&mut self, text: impl ToString) -> eyre::Result<()> {
        let text = text.to_string();
        tracing::debug!(actor="mock_llm_context", event="respond_to_user", text_len=text.len());
        Ok(self.cx.llm_response_tx.send(ChatResponseStream::AssistantResponseEvent {
            content: text,
        }.into())?)
    }

    /// Send a tool call via channel
    pub async fn call_tool(&mut self, tool_use_id: String, name: String, args: Option<Value>, stop: Option<bool>) -> eyre::Result<()> {
        tracing::debug!(actor="mock_llm_context", event="call_tool", %tool_use_id, %name, has_args=args.is_some(), ?stop);
        let input = args.map(|v| v.to_string());
        
        Ok(self.cx.llm_response_tx.send(ChatResponseStream::ToolUseEvent {
            tool_use_id,
            name,
            input,
            stop,
        }.into())?)
    }
}

impl Drop for MockLLMContextTurn<'_> {
    fn drop(&mut self) {
        tracing::debug!(actor="mock_llm_context", event="turn_ends");
        let _ = self.cx.llm_response_tx.send(ChatResponseStreamOrEndTurn::EndTurn);
    }
}

impl From<ChatResponseStream> for ChatResponseStreamOrEndTurn {
    fn from(value: ChatResponseStream) -> Self {
        ChatResponseStreamOrEndTurn::Message(value)
    }
}