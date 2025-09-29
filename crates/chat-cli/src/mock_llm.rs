#![cfg_attr(not(test), allow(dead_code))]

//! Mock LLM architecture for testing
//!
//! This provides a stateless per-turn mock system that matches real LLM behavior.
//! Each user message spawns a fresh mock context with full conversation history.

use eyre::Result;
use regex::Regex;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::mpsc;

use crate::api_client::model::{ChatMessage, ChatResponseStream};

/// Captures from regex matching against conversation
pub type ConversationMatches = HashMap<String, String>;

/// Context for per-turn mock LLM execution with conversation history and streaming response
/// This is the main interface that test scripts interact with.
pub struct MockLLMContext {
    conversation_history: Vec<ChatMessage>,
    current_user_message: String,
    tx: mpsc::Sender<Result<ChatResponseStream, RecvError>>,
}

impl MockLLMContext {
    async fn send_text(
        tx: &mut mpsc::Sender<Result<ChatResponseStream, RecvError>>,
        text: impl ToString,
    ) -> eyre::Result<()> {
        tx
                        .send(Ok(ChatResponseStream::AssistantResponseEvent {
                            content: text.to_string(),
                        }))
                        .await
                        .map_err(|_| eyre::eyre!("Response channel closed"))
    }

    /// Respond with text to the user.
    #[allow(dead_code)]
    pub async fn respond(&mut self, text: impl ToString) -> eyre::Result<()> {
        Self::send_text(&mut self.tx, text).await
    }

    /// Count the number of user messages in the conversation (including the current one).
    /// Useful for determining which predefined response to use in sequence.
    #[allow(dead_code)]
    pub fn count_user_messages(&self) -> usize {
        // Count user messages in history + current message
        let user_messages_in_history = self.conversation_history
            .iter()
            .filter(|msg| matches!(msg, ChatMessage::UserInputMessage(_)))
            .count();
        user_messages_in_history + 1 // +1 for current message
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
    pub fn match_conversation(
        &self,
        history_patterns: &[&str],
        current_pattern: &str,
    ) -> Result<Option<ConversationMatches>> {
        tracing::trace!("match_conversation: current_pattern={:?}, history_patterns={:?}", current_pattern, history_patterns);
        let mut all_captures = HashMap::new();

        // Compile current message pattern
        let current_regex = Regex::new(current_pattern)
            .map_err(|e| eyre::eyre!("Failed to compile current message pattern '{}': {}", current_pattern, e))?;

        // Match against current user message
        tracing::trace!("Matching current message {:?} against pattern {:?}", self.current_user_message, current_pattern);
        if let Some(caps) = current_regex.captures(&self.current_user_message) {
            tracing::trace!("Current message matched! Captures: {:?}", caps);
            // Extract named captures from current message
            for name in current_regex.capture_names().flatten() {
                if let Some(m) = caps.name(name) {
                    all_captures.insert(name.to_string(), m.as_str().to_string());
                    tracing::trace!("Captured from current message: {}={}", name, m.as_str());
                }
            }
        } else {
            tracing::trace!("Current message did not match pattern");
            return Ok(None); // Current message doesn't match
        }

        // Match history patterns against conversation history
        // We need to find a subsequence in the history that matches all patterns
        if history_patterns.is_empty() {
            tracing::trace!("No history patterns, returning current captures");
            return Ok(Some(all_captures));
        }

        // Compile all history patterns
        let history_regexes: Result<Vec<Regex>, regex::Error> =
            history_patterns.iter().map(|p| Regex::new(p)).collect();
        let history_regexes = history_regexes.map_err(|e| eyre::eyre!("Failed to compile history pattern: {}", e))?;

        // Convert history to strings for matching
        let history_strings: Vec<String> = self
            .conversation_history
            .iter()
            .map(|msg| match msg {
                ChatMessage::UserInputMessage(user_msg) => format!("user: {}", user_msg.content),
                ChatMessage::AssistantResponseMessage(assistant_msg) => format!("assistant: {}", assistant_msg.content),
            })
            .collect();

        tracing::trace!("Formatted history strings for pattern matching:");
        for (i, hist_str) in history_strings.iter().enumerate() {
            tracing::trace!("  [{}]: {:?}", i, hist_str);
        }

        // Try to match all history patterns as a sequence
        tracing::trace!("Attempting to match history patterns: {:?}", history_patterns);
        if self.match_history_sequence(&history_strings, &history_regexes, &mut all_captures) {
            tracing::trace!("History patterns matched! Final captures: {:?}", all_captures);
            Ok(Some(all_captures))
        } else {
            tracing::trace!("History patterns did not match");
            Ok(None)
        }
    }

    /// Helper to match history patterns as a subsequence
    fn match_history_sequence(
        &self,
        history: &[String],
        patterns: &[Regex],
        captures: &mut ConversationMatches,
    ) -> bool {
        if patterns.is_empty() {
            tracing::trace!("No patterns to match, returning true");
            return true;
        }

        tracing::trace!("Matching {} patterns against {} history items", patterns.len(), history.len());

        // Try to find starting positions where we can match the full sequence
        for start_idx in 0..=(history.len().saturating_sub(patterns.len())) {
            tracing::trace!("Trying to match patterns starting at history index {}", start_idx);
            let mut temp_captures = HashMap::new();
            let mut matched = true;

            // Try to match each pattern in sequence starting from start_idx
            for (pattern_idx, pattern) in patterns.iter().enumerate() {
                let history_idx = start_idx + pattern_idx;
                if history_idx >= history.len() {
                    tracing::trace!("History index {} out of bounds", history_idx);
                    matched = false;
                    break;
                }

                let history_item = &history[history_idx];
                tracing::trace!("Matching pattern {:?} against history[{}]: {:?}", pattern.as_str(), history_idx, history_item);
                
                if let Some(caps) = pattern.captures(history_item) {
                    tracing::trace!("Pattern matched! Captures: {:?}", caps);
                    // Extract named captures
                    for name in pattern.capture_names().flatten() {
                        if let Some(m) = caps.name(name) {
                            temp_captures.insert(name.to_string(), m.as_str().to_string());
                            tracing::trace!("Captured from history: {}={}", name, m.as_str());
                        }
                    }
                } else {
                    tracing::trace!("Pattern did not match history item");
                    matched = false;
                    break;
                }
            }

            if matched {
                tracing::trace!("All patterns matched starting at index {}! Merging captures: {:?}", start_idx, temp_captures);
                // Merge temp_captures into main captures
                captures.extend(temp_captures);
                return true;
            } else {
                tracing::trace!("Not all patterns matched starting at index {}", start_idx);
            }
        }

        tracing::trace!("No starting position worked, sequence match failed");
        false
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
        tracing::debug!("MockLLM try_patterns called with {} patterns", patterns.len());
        tracing::debug!("Current user message: {:?}", self.current_user_message);
        tracing::debug!("Conversation history length: {}", self.conversation_history.len());
        
        // Show full conversation history for debugging
        for (i, msg) in self.conversation_history.iter().enumerate() {
            match msg {
                ChatMessage::UserInputMessage(user_msg) => {
                    tracing::debug!("History[{}]: user: {:?}", i, user_msg.content);
                }
                ChatMessage::AssistantResponseMessage(assistant_msg) => {
                    tracing::debug!("History[{}]: assistant: {:?}", i, assistant_msg.content);
                }
            }
        }
        
        for (pattern_idx, (history_patterns, current_pattern, response_template)) in patterns.iter().enumerate() {
            tracing::debug!("Trying pattern {}: history_patterns={:?}, current_pattern={:?}", 
                           pattern_idx, history_patterns, current_pattern);
            
            // Try to match this pattern
            match self.match_conversation(history_patterns, current_pattern)? {
                Some(captures) => {
                    tracing::debug!("Pattern {} matched! Captures: {:?}", pattern_idx, captures);
                    // Pattern matched! Do regex substitution on response template
                    let response = self.substitute_captures(current_pattern, &captures, response_template)?;
                    tracing::debug!("Generated response: {:?}", response);

                    // Send the response
                    Self::send_text(&mut self.tx, response).await?;

                    return Ok(()); // Success - matched and responded
                },
                None => {
                    tracing::debug!("Pattern {} did not match", pattern_idx);
                    // This pattern didn't match, try the next one
                    continue;
                },
            }
        }

        tracing::debug!("No patterns matched, returning error");
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

/// Concrete implementation that wraps a closure for per-turn mock execution
pub struct MockLLM {
    closure: Box<dyn Fn(MockLLMContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync + 'static>,
}

impl std::fmt::Debug for MockLLM {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockLLMInstance").finish()
    }
}

impl MockLLM {
    pub fn new<F, Fut>(closure: F) -> Self
    where
        F: Fn(crate::mock_llm::MockLLMContext) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = eyre::Result<()>> + Send + 'static,
    {
        Self {
            closure: Box::new(move |ctx| Box::pin(closure(ctx))),
        }
    }

    /// Spawn a task for this turn. Returns a receiver which will receive events
    /// emitted by this task. If that receiver is dropped, the task will naturally
    /// terminate.
    pub fn spawn_turn(
        &self,
        conversation_history: Vec<ChatMessage>,
        current_user_message: String,
    ) -> mpsc::Receiver<Result<ChatResponseStream, RecvError>> {
        // Create a fresh channel for this mock turn
        // The mock script will send ResponseEvents via the context's tx
        // The consumer will receive them via mock_rx
        let (mock_tx, mock_rx) = mpsc::channel(32);
        let mock_tx_clone = mock_tx.clone();

        // Create context with the provided tx channel
        let mock_context = MockLLMContext {
            conversation_history,
            current_user_message,
            tx: mock_tx,
        };

        let future = (self.closure)(mock_context);
        tokio::spawn(async move {
            match future.await {
                Ok(()) => {
                    // Just return, this will close the channel.
                }
                Err(e) => {
                    // Send error on failure
                    let _ = mock_tx_clone.send(Err(RecvError::from(e))).await;
                }
            }
        });

        mock_rx
    }
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
