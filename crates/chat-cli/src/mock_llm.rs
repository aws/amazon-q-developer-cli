//! Mock LLM architecture for testing
//! 
//! This provides a stateless per-turn mock system that matches real LLM behavior.
//! Each user message spawns a fresh mock context with full conversation history.

use std::future::Future;
use std::pin::Pin;
use std::collections::HashMap;
use tokio::sync::mpsc;
use eyre::Result;
use regex::Regex;

use crate::api_client::model::ChatMessage;
use crate::cli::chat::{ResponseEvent, RequestMetadata, AssistantMessage, AssistantToolUse};

/// Handle to a running mock LLM instance. 
/// Dropping this handle will cancel the mock by closing the response channel.
#[derive(Debug)]
pub struct MockLLMHandle {
    pub rx: mpsc::Receiver<Result<ResponseEvent, RecvError>>,
}

impl MockLLMHandle {
    pub async fn recv(&mut self) -> Option<Result<ResponseEvent, RecvError>> {
        self.rx.recv().await
    }
}

/// Captures from regex matching against conversation
pub type ConversationMatches = HashMap<String, String>;

/// Context for per-turn mock LLM execution with conversation history and streaming response
/// This is the main interface that test scripts interact with.
pub struct MockLLMContext {
    conversation_history: Vec<ChatMessage>,
    current_user_message: String,
    tx: mpsc::Sender<Result<ResponseEvent, RecvError>>,
}

/// Individual turn within a mock LLM context - represents one user message
pub struct MockTurn {
    user_message: String,
    tx: mpsc::Sender<Result<ResponseEvent, RecvError>>,
}

impl MockLLMContext {
    pub fn new(
        conversation_history: Vec<ChatMessage>,
        current_user_message: String,
        tx: mpsc::Sender<Result<ResponseEvent, RecvError>>,
    ) -> Self {
        Self {
            conversation_history,
            current_user_message,
            tx,
        }
    }

    /// Read the current user message as a turn - this is the main API for test scripts
    /// Returns Some(turn) for the current user message, None if no message
    pub async fn read_user_message(&mut self) -> Option<MockTurn> {
        if !self.current_user_message.is_empty() {
            let turn = MockTurn {
                user_message: std::mem::take(&mut self.current_user_message),
                tx: self.tx.clone(),
            };
            Some(turn)
        } else {
            None
        }
    }

    /// Get access to conversation history for pattern matching
    pub fn conversation_history(&self) -> &[ChatMessage] {
        &self.conversation_history
    }

    /// Match conversation against regex patterns and return captured groups
    /// 
    /// # Arguments
    /// * `history_patterns` - Patterns to match against conversation history messages
    /// * `current_pattern` - Pattern to match against current user message  
    /// 
    /// # Returns
    /// - `Ok(Some(captures))` if all patterns match, where captures contains all named groups (?P<name>...)
    /// - `Ok(None)` if patterns are valid but don't match the conversation
    /// - `Err(...)` if regex compilation fails or other internal errors occur
    /// 
    /// # Example
    /// ```ignore
    /// let captures = ctx.match_conversation(
    ///     &["assistant said (?P<previous>.+)", "user.*(?P<topic>\\w+)"],
    ///     "tell me about (?P<query>.+)"
    /// )?;  // Propagate regex compilation errors
    /// 
    /// if let Some(caps) = captures {
    ///     let query = caps.get("query").unwrap();
    ///     // Use captured values...
    /// }
    /// ```
    pub fn match_conversation(&self, history_patterns: &[&str], current_pattern: &str) -> Result<Option<ConversationMatches>> {
        let mut all_captures = HashMap::new();

        // Compile current message pattern
        let current_regex = Regex::new(current_pattern)
            .map_err(|e| eyre::eyre!("Failed to compile current message pattern '{}': {}", current_pattern, e))?;
        
        // Match against current user message
        if let Some(caps) = current_regex.captures(&self.current_user_message) {
            // Extract named captures from current message
            for name in current_regex.capture_names().flatten() {
                if let Some(m) = caps.name(name) {
                    all_captures.insert(name.to_string(), m.as_str().to_string());
                }
            }
        } else {
            return Ok(None); // Current message doesn't match
        }

        // Match history patterns against conversation history
        // We need to find a subsequence in the history that matches all patterns
        if history_patterns.is_empty() {
            return Ok(Some(all_captures));
        }

        // Compile all history patterns
        let history_regexes: Result<Vec<Regex>, regex::Error> = history_patterns
            .iter()
            .map(|p| Regex::new(p))
            .collect();
        let history_regexes = history_regexes
            .map_err(|e| eyre::eyre!("Failed to compile history pattern: {}", e))?;

        // Convert history to strings for matching
        let history_strings: Vec<String> = self.conversation_history
            .iter()
            .map(|msg| match msg {
                ChatMessage::UserInputMessage(user_msg) => format!("user: {}", user_msg.content),
                ChatMessage::AssistantResponseMessage(assistant_msg) => format!("assistant: {}", assistant_msg.content),
            })
            .collect();

        // Try to match all history patterns as a sequence
        if self.match_history_sequence(&history_strings, &history_regexes, &mut all_captures) {
            Ok(Some(all_captures))
        } else {
            Ok(None)
        }
    }

    /// Helper to match history patterns as a subsequence
    fn match_history_sequence(&self, history: &[String], patterns: &[Regex], captures: &mut ConversationMatches) -> bool {
        if patterns.is_empty() {
            return true;
        }

        // Try to find starting positions where we can match the full sequence
        for start_idx in 0..=(history.len().saturating_sub(patterns.len())) {
            let mut temp_captures = HashMap::new();
            let mut matched = true;

            // Try to match each pattern in sequence starting from start_idx
            for (pattern_idx, pattern) in patterns.iter().enumerate() {
                let history_idx = start_idx + pattern_idx;
                if history_idx >= history.len() {
                    matched = false;
                    break;
                }

                if let Some(caps) = pattern.captures(&history[history_idx]) {
                    // Extract named captures
                    for name in pattern.capture_names().flatten() {
                        if let Some(m) = caps.name(name) {
                            temp_captures.insert(name.to_string(), m.as_str().to_string());
                        }
                    }
                } else {
                    matched = false;
                    break;
                }
            }

            if matched {
                // Merge temp_captures into main captures
                captures.extend(temp_captures);
                return true;
            }
        }

        false
    }

    /// Convenience method to match and respond in one call
    /// 
    /// # Returns
    /// - `Ok(true)` if pattern matched and response was sent
    /// - `Ok(false)` if pattern didn't match (no response sent)
    /// - `Err(...)` if regex compilation failed or response channel closed
    pub async fn match_and_respond(&mut self, history_patterns: &[&str], current_pattern: &str, response: &str) -> Result<bool> {
        match self.match_conversation(history_patterns, current_pattern)? {
            Some(_captures) => {
                self.tx.send(Ok(ResponseEvent::AssistantText(response.to_string())))
                    .await
                    .map_err(|_| eyre::eyre!("Response channel closed"))?;
                Ok(true)
            }
            None => Ok(false)
        }
    }

    /// Convenience method to match and respond with capture substitution
    /// Substitutes {capture_name} in the response with captured values
    /// 
    /// # Returns  
    /// - `Ok(true)` if pattern matched and response was sent
    /// - `Ok(false)` if pattern didn't match (no response sent)
    /// - `Err(...)` if regex compilation failed or response channel closed
    pub async fn match_and_respond_with_captures(&mut self, history_patterns: &[&str], current_pattern: &str, response_template: &str) -> Result<bool> {
        match self.match_conversation(history_patterns, current_pattern)? {
            Some(captures) => {
                let mut response = response_template.to_string();
                
                // Replace {capture_name} with captured values
                for (name, value) in &captures {
                    response = response.replace(&format!("{{{}}}", name), value);
                }
                
                self.tx.send(Ok(ResponseEvent::AssistantText(response)))
                    .await
                    .map_err(|_| eyre::eyre!("Response channel closed"))?;
                Ok(true)
            }
            None => Ok(false)
        }
    }

    /// Declarative pattern matching with automatic regex substitution
    /// 
    /// Tries each pattern tuple in order until one matches, then sends response with proper
    /// regex substitution using `$name` syntax for captured groups.
    /// 
    /// # Arguments
    /// * `patterns` - Array of (history_patterns, current_pattern, response_template) tuples
    /// 
    /// # Returns
    /// - `Ok(())` if any pattern matched and response was sent
    /// - `Err("unexpected input")` if no patterns matched
    /// - `Err(...)` if regex compilation failed or response channel closed
    /// 
    /// # Example
    /// ```ignore
    /// ctx.try_patterns(&[
    ///     (&[], r"(?i)hi,?\s+claude", "Hi, you! What's your name?"),
    ///     (&[r"assistant.*What's your name"], r"(?i)(?:i'm|my name is|call me)\s+(?P<name>\w+)", "Hi $name, I'm Q!"),
    ///     (&[], r".*", "I didn't understand that."),  // Fallback
    /// ]).await?;
    /// ```
    pub async fn try_patterns(&mut self, patterns: &[(&[&str], &str, &str)]) -> Result<()> {
        for (history_patterns, current_pattern, response_template) in patterns {
            // Try to match this pattern
            match self.match_conversation(history_patterns, current_pattern)? {
                Some(captures) => {
                    // Pattern matched! Do regex substitution on response template
                    let response = self.substitute_captures(current_pattern, &captures, response_template)?;
                    
                    // Send the response
                    self.tx.send(Ok(ResponseEvent::AssistantText(response)))
                        .await
                        .map_err(|_| eyre::eyre!("Response channel closed"))?;
                    
                    return Ok(()); // Success - matched and responded
                }
                None => {
                    // This pattern didn't match, try the next one
                    continue;
                }
            }
        }
        
        // No patterns matched
        Err(eyre::eyre!("unexpected input"))
    }

    /// Helper to perform regex substitution using captured groups
    /// Uses proper regex substitution with $name syntax
    fn substitute_captures(&self, pattern: &str, captures: &ConversationMatches, template: &str) -> Result<String> {
        // Create a regex to re-capture the current message for proper substitution
        let regex = Regex::new(pattern)
            .map_err(|e| eyre::eyre!("Failed to recompile pattern for substitution '{}': {}", pattern, e))?;
        
        if let Some(caps) = regex.captures(&self.current_user_message) {
            // Use regex's built-in substitution which handles $name syntax properly
            let mut result = String::new();
            caps.expand(template, &mut result);
            Ok(result)
        } else {
            // Fallback to manual substitution if regex doesn't match current message
            // This handles cases where captures came from history patterns
            let mut result = template.to_string();
            for (name, value) in captures {
                result = result.replace(&format!("${}", name), value);
                result = result.replace(&format!("${{{}}}", name), value); // Also support ${name} syntax
            }
            Ok(result)
        }
    }
}

impl MockTurn {
    /// Get the user's message content
    pub fn user_message(&self) -> &str {
        &self.user_message
    }

    /// Send a text response back to the user
    pub async fn respond_to_user(&mut self, text: &str) -> Result<()> {
        self.tx.send(Ok(ResponseEvent::AssistantText(text.to_string())))
            .await
            .map_err(|_| eyre::eyre!("Response channel closed"))?;
        Ok(())
    }

    /// Send a tool use start event 
    pub async fn tool_use_start(&mut self, name: &str) -> Result<()> {
        self.tx.send(Ok(ResponseEvent::ToolUseStart { name: name.to_string() }))
            .await
            .map_err(|_| eyre::eyre!("Response channel closed"))?;
        Ok(())
    }

    /// Send a tool use event with arguments
    pub async fn tool_use(&mut self, tool_use: AssistantToolUse) -> Result<()> {
        self.tx.send(Ok(ResponseEvent::ToolUse(tool_use)))
            .await
            .map_err(|_| eyre::eyre!("Response channel closed"))?;
        Ok(())
    }
}


/// Trait for creating mock LLM instances - called once per turn
pub trait MockLLM: Send + Sync + std::fmt::Debug {
    fn spawn_mock_llm(&self, context: MockLLMContext) -> MockLLMHandle;
}

/// Concrete implementation that wraps a closure for per-turn mock execution
pub struct MockLLMInstance<F> 
where 
    F: Fn(MockLLMContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync + 'static,
{
    closure: F,
}

impl<F> std::fmt::Debug for MockLLMInstance<F> 
where 
    F: Fn(MockLLMContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockLLMInstance")
            .field("closure", &"<closure>")
            .finish()
    }
}

impl<F> MockLLMInstance<F>
where 
    F: Fn(MockLLMContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync + 'static,
{
    pub fn new(closure: F) -> Self {
        Self { closure }
    }
}

impl<F> MockLLM for MockLLMInstance<F>
where 
    F: Fn(MockLLMContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync + 'static,
{
    fn spawn_mock_llm(&self, context: MockLLMContext) -> MockLLMHandle {
        // Create a fresh channel for this mock turn
        // The mock script will send ResponseEvents via the context's tx
        // The consumer will receive them via mock_rx
        let (mock_tx, mock_rx) = mpsc::channel(32);
        let mock_tx_clone = mock_tx.clone();
        
        // Create context with the provided tx channel  
        let mock_context = MockLLMContext {
            conversation_history: context.conversation_history,
            current_user_message: context.current_user_message,
            tx: mock_tx,
        };
        
        let future = (self.closure)(mock_context);
        tokio::spawn(async move {
            match future.await {
                Ok(()) => {
                    // Send EndStream on successful completion
                    let message = AssistantMessage::Response { 
                        message_id: None, 
                        content: String::new() 
                    };
                    let request_metadata = RequestMetadata::default();
                    let _ = mock_tx_clone.send(Ok(ResponseEvent::EndStream { 
                        message,
                        request_metadata,
                    })).await;
                }
                Err(e) => {
                    // Send error on failure
                    let _ = mock_tx_clone.send(Err(RecvError::from(e))).await;
                }
            }
        });
        
        MockLLMHandle { rx: mock_rx }
    }
}

/// Helper function to create a mock LLM instance from a closure - provides the clean API for tests
pub fn spawn_mock_llm<F, Fut>(closure: F) -> MockLLMInstance<impl Fn(MockLLMContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync + 'static>
where
    F: Fn(MockLLMContext) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    MockLLMInstance::new(move |ctx| Box::pin(closure(ctx)))
}

// Error type to match existing RecvError from parser
#[derive(Debug)]
pub struct RecvError(eyre::Error);

impl From<eyre::Error> for RecvError {
    fn from(e: eyre::Error) -> Self {
        RecvError(e)
    }
}

impl std::fmt::Display for RecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for RecvError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

// Compatibility alias for existing code during migration
pub type MockConversationState = MockLLMContext;