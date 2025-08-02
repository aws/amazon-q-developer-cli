use std::sync::Arc;
use eyre::Result;
use crate::cli::chat::ChatSession;
use crate::cli::chat::api::{SocketLifecycleManager, MessageRouter, SocketType};
use crate::cli::agent::Agents;
use crate::os::Os;

/// ApiChatSession extends ChatSession with socket support for hybrid API mode
pub struct ApiChatSession {
    /// The underlying chat session that handles all terminal interaction
    chat_session: ChatSession,
    /// Socket lifecycle manager for creating and managing Unix domain sockets
    lifecycle_manager: SocketLifecycleManager,
    /// Message router for distributing messages to socket clients
    message_router: Arc<MessageRouter>,
    /// Session ID for this API session
    session_id: String,
}

impl ApiChatSession {
    /// Create a new ApiChatSession with socket support
    pub async fn new(
        os: &mut Os,
        stdout: std::io::Stdout,
        stderr: std::io::Stderr,
        conversation_id: &str,
        agents: Agents,
        input: Option<String>,
        input_source: crate::cli::chat::input_source::InputSource,
        resume_conversation: bool,
        terminal_width_provider: fn() -> Option<usize>,
        tool_manager: crate::cli::chat::tool_manager::ToolManager,
        model_id: Option<String>,
        tool_config: std::collections::HashMap<String, crate::cli::chat::tools::ToolSpec>,
        interactive: bool,
        working_directory: &std::path::Path,
    ) -> Result<Self> {
        // Create socket lifecycle manager
        let mut lifecycle_manager = SocketLifecycleManager::new(working_directory.to_path_buf())?;
        
        // Initialize sockets
        lifecycle_manager.initialize().await?;
        
        // Get socket manager and create message router
        let socket_manager = lifecycle_manager.socket_manager();
        let session_id = lifecycle_manager.session_id().to_string();
        
        // Create shared broadcast channels for all socket types
        let mut shared_broadcasters = std::collections::HashMap::new();
        for socket_type in SocketType::all() {
            let (sender, _) = tokio::sync::broadcast::channel(1000);
            shared_broadcasters.insert(socket_type, sender);
        }
        
        // Create connection handler and message router with shared broadcasters
        let mut connection_handler = crate::cli::chat::api::ConnectionHandler::new(
            Arc::clone(&socket_manager),
            shared_broadcasters.clone(),
        );
        
        let message_router = Arc::new(MessageRouter::new(
            socket_manager,
            shared_broadcasters,
            session_id.clone(),
        ));
        
        // Start connection handler in background
        tokio::spawn(async move {
            if let Err(e) = connection_handler.start_accepting_connections().await {
                eprintln!("Connection handler error: {}", e);
            }
        });
        
        // Create ChatSession with message router integration
        let chat_session = ChatSession::new(
            os,
            stdout,
            stderr,
            conversation_id,
            agents,
            input,
            input_source,
            resume_conversation,
            terminal_width_provider,
            tool_manager,
            model_id,
            tool_config,
            interactive,
            None, // No lifecycle manager for ChatSession
            Some(Arc::clone(&message_router)),
        ).await?;
        
        Ok(Self {
            chat_session,
            lifecycle_manager,
            message_router,
            session_id,
        })
    }
    
    /// Get the session ID for this API session
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    
    /// Start the API chat session
    pub async fn run(&mut self, os: &mut Os) -> Result<()> {
        // Run the main chat session (this handles terminal interaction)
        self.chat_session.spawn(os).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::cli::agent::Agents;
    use crate::cli::chat::{
        input_source::InputSource,
        tool_manager::ToolManager,
        tools::ToolSpec,
    };
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_api_chat_session_creation() {
        let temp_dir = TempDir::new().unwrap();
        let mut os = Os::new().await.unwrap();
        
        let agents = Agents::default();
        let tool_manager = ToolManager::default();
        let tool_config: HashMap<String, ToolSpec> = HashMap::new();
        
        let api_session = ApiChatSession::new(
            &mut os,
            std::io::stdout(),
            std::io::stderr(),
            "test_conversation",
            agents,
            None,
            InputSource::new_mock(vec!["exit".to_string()]),
            false,
            || Some(80),
            tool_manager,
            None,
            tool_config,
            true,
            temp_dir.path(),
        ).await.unwrap();
        
        // Verify session was created successfully
        assert!(!api_session.session_id().is_empty());
    }
}
