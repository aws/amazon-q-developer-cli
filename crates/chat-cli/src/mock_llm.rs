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
    let (llm_response_tx, llm_response_rx) = mpsc::channel(1);

    let context = MockLLMContext {
        user_input_rx,
        llm_response_tx,
    };

    tokio::spawn(script(context));

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
    llm_response_rx: mpsc::Receiver<ChatResponseStream>,
}

impl MockLLM {
    /// Convey the user's message to the script.
    pub async fn send_user_message(&mut self, text: String) -> Result<(), mpsc::error::SendError<String>> {
        self.user_input_tx.send(text).await
    }

    /// Read the response from the LLM (could be text or tool call).
    pub async fn read_llm_response(&mut self) -> Option<ChatResponseStream> {
        self.llm_response_rx.recv().await
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
    llm_response_tx: mpsc::Sender<ChatResponseStream>,
}

impl MockLLMContext {
    /// Read the next user message from the channel
    pub async fn read_user_message(&mut self) -> Option<String> {
        self.user_input_rx.recv().await
    }

    /// Send a text response back to the user via channel
    pub async fn respond_to_user(&mut self, text: String) -> Result<(), mpsc::error::SendError<ChatResponseStream>> {
        self.llm_response_tx.send(ChatResponseStream::AssistantResponseEvent {
            content: text,
        }).await
    }

    /// Send a tool call via channel
    pub async fn call_tool(&mut self, tool_use_id: String, name: String, args: Option<Value>, stop: Option<bool>) -> Result<(), mpsc::error::SendError<ChatResponseStream>> {
        let input = args.map(|v| v.to_string());
        
        self.llm_response_tx.send(ChatResponseStream::ToolUseEvent {
            tool_use_id,
            name,
            input,
            stop,
        }).await
    }


}
