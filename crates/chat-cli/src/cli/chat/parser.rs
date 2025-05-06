use std::time::{
    Duration,
    Instant,
};

use eyre::Result;
use rand::distr::{
    Alphanumeric,
    SampleString,
};
use thiserror::Error;
use tracing::{
    error,
    info,
    trace,
};

use super::message::{
    AssistantMessage,
    AssistantToolUse,
};
use crate::fig_api_client::clients::SendMessageOutput;
use crate::fig_api_client::model::ChatResponseStream;

#[derive(Debug, Error)]
pub struct RecvError {
    /// The request id associated with the [SendMessageOutput] stream.
    pub request_id: Option<String>,
    #[source]
    pub source: RecvErrorKind,
}

impl std::fmt::Display for RecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to receive the next message: ")?;
        if let Some(request_id) = self.request_id.as_ref() {
            write!(f, "request_id: {}, error: ", request_id)?;
        }
        write!(f, "{}", self.source)?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum RecvErrorKind {
    #[error("{0}")]
    Client(#[from] crate::fig_api_client::ApiClientError),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    /// An error was encountered while waiting for the next event in the stream after a noticeably
    /// long wait time.
    ///
    /// *Context*: the client can throw an error after ~100s of waiting with no response, likely due
    /// to an exceptionally complex tool use taking too long to generate.
    #[error("The stream ended after {}s: {source}", .duration.as_secs())]
    StreamTimeout {
        source: crate::fig_api_client::ApiClientError,
        duration: std::time::Duration,
    },
    /// Unexpected end of stream while receiving a tool use.
    ///
    /// *Context*: the stream can unexpectedly end with `Ok(None)` while waiting for an
    /// exceptionally complex tool use. This is due to some proxy server dropping idle
    /// connections after some timeout is reached.
    ///
    /// TODO: should this be removed?
    #[error("Unexpected end of stream for tool: {} with id: {}", .name, .tool_use_id)]
    UnexpectedToolUseEos {
        tool_use_id: String,
        name: String,
        message: Box<AssistantMessage>,
        time_elapsed: Duration,
    },
}

/// State associated with parsing a [ChatResponseStream] into a [Message].
///
/// # Usage
///
/// You should repeatedly call [Self::recv] to receive [ResponseEvent]'s until a
/// [ResponseEvent::EndStream] value is returned.
#[derive(Debug)]
pub struct ResponseParser {
    /// The response to consume and parse into a sequence of [Ev].
    response: SendMessageOutput,
    /// Buffer to hold the next event in [SendMessageOutput].
    peek: Option<ChatResponseStream>,
    /// Message identifier for the assistant's response. Randomly generated on creation.
    message_id: String,
    /// Buffer for holding the accumulated assistant response.
    assistant_text: String,
    /// Tool uses requested by the model.
    tool_uses: Vec<AssistantToolUse>,
    /// Whether or not we are currently receiving tool use delta events. Tuple of
    /// `Some((tool_use_id, name))` if true, [None] otherwise.
    parsing_tool_use: Option<(String, String)>,
}

impl ResponseParser {
    pub fn new(response: SendMessageOutput) -> Self {
        let message_id = Alphanumeric.sample_string(&mut rand::rng(), 9);
        info!(?message_id, "Generated new message id");
        Self {
            response,
            peek: None,
            message_id,
            assistant_text: String::new(),
            tool_uses: Vec::new(),
            parsing_tool_use: None,
        }
    }

    /// Consumes the associated [ConverseStreamResponse] until a valid [ResponseEvent] is parsed.
    pub async fn recv(&mut self) -> Result<ResponseEvent, RecvError> {
        if let Some((id, name)) = self.parsing_tool_use.take() {
            let tool_use = self.parse_tool_use(id, name).await?;
            self.tool_uses.push(tool_use.clone());
            return Ok(ResponseEvent::ToolUse(tool_use));
        }

        // First, handle discarding AssistantResponseEvent's that immediately precede a
        // CodeReferenceEvent.
        let peek = self.peek().await?;
        if let Some(ChatResponseStream::AssistantResponseEvent { content }) = peek {
            // Cloning to bypass borrowchecker stuff.
            let content = content.clone();
            self.next().await?;
            match self.peek().await? {
                Some(ChatResponseStream::CodeReferenceEvent(_)) => (),
                _ => {
                    self.assistant_text.push_str(&content);
                    return Ok(ResponseEvent::AssistantText(content));
                },
            }
        }

        loop {
            match self.next().await {
                Ok(Some(output)) => match output {
                    ChatResponseStream::AssistantResponseEvent { content } => {
                        self.assistant_text.push_str(&content);
                        return Ok(ResponseEvent::AssistantText(content));
                    },
                    ChatResponseStream::InvalidStateEvent { reason, message } => {
                        error!(%reason, %message, "invalid state event");
                    },
                    ChatResponseStream::ToolUseEvent {
                        tool_use_id,
                        name,
                        input,
                        stop,
                    } => {
                        debug_assert!(input.is_none(), "Unexpected initial content in first tool use event");
                        debug_assert!(
                            stop.is_none_or(|v| !v),
                            "Unexpected immediate stop in first tool use event"
                        );
                        self.parsing_tool_use = Some((tool_use_id.clone(), name.clone()));
                        return Ok(ResponseEvent::ToolUseStart { name });
                    },
                    _ => {},
                },
                Ok(None) => {
                    let message_id = Some(self.message_id.clone());
                    let content = std::mem::take(&mut self.assistant_text);
                    let message = if self.tool_uses.is_empty() {
                        AssistantMessage::new_response(message_id, content)
                    } else {
                        AssistantMessage::new_tool_use(
                            message_id,
                            content,
                            self.tool_uses.clone().into_iter().collect(),
                        )
                    };
                    return Ok(ResponseEvent::EndStream { message });
                },
                Err(err) => return Err(err),
            }
        }
    }

    /// Consumes the response stream until a valid [ToolUse] is parsed.
    ///
    /// The arguments are the fields from the first [ChatResponseStream::ToolUseEvent] consumed.
    async fn parse_tool_use(&mut self, id: String, name: String) -> Result<AssistantToolUse, RecvError> {
        let mut tool_string = String::new();
        let start = Instant::now();
        while let Some(ChatResponseStream::ToolUseEvent { .. }) = self.peek().await? {
            if let Some(ChatResponseStream::ToolUseEvent { input, stop, .. }) = self.next().await? {
                if let Some(i) = input {
                    tool_string.push_str(&i);
                }
                if let Some(true) = stop {
                    break;
                }
            }
        }

        let args = match serde_json::from_str(&tool_string) {
            Ok(args) => args,
            Err(err) if !tool_string.is_empty() => {
                // If we failed deserializing after waiting for a long time, then this is most
                // likely bedrock responding with a stop event for some reason without actually
                // including the tool contents. Essentially, the tool was too large.
                // Timeouts have been seen as short as ~1 minute, so setting the time to 30.
                let time_elapsed = start.elapsed();
                if self.peek().await?.is_none() && time_elapsed > Duration::from_secs(30) {
                    error!(
                        "Received an unexpected end of stream after spending ~{}s receiving tool events",
                        time_elapsed.as_secs_f64()
                    );
                    self.tool_uses.push(AssistantToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        args: serde_json::Value::Object(
                            [(
                                "key".to_string(),
                                serde_json::Value::String(
                                    "WARNING: the actual tool use arguments were too complicated to be generated"
                                        .to_string(),
                                ),
                            )]
                            .into_iter()
                            .collect(),
                        ),
                    });
                    let message = Box::new(AssistantMessage::new_tool_use(
                        Some(self.message_id.clone()),
                        std::mem::take(&mut self.assistant_text),
                        self.tool_uses.clone().into_iter().collect(),
                    ));
                    return Err(self.error(RecvErrorKind::UnexpectedToolUseEos {
                        tool_use_id: id,
                        name,
                        message,
                        time_elapsed,
                    }));
                } else {
                    return Err(self.error(err));
                }
            },
            // if the tool just does not need any input
            _ => serde_json::json!({}),
        };
        Ok(AssistantToolUse { id, name, args })
    }

    /// Returns the next event in the [SendMessageOutput] without consuming it.
    async fn peek(&mut self) -> Result<Option<&ChatResponseStream>, RecvError> {
        if self.peek.is_some() {
            return Ok(self.peek.as_ref());
        }
        match self.next().await? {
            Some(v) => {
                self.peek = Some(v);
                Ok(self.peek.as_ref())
            },
            None => Ok(None),
        }
    }

    /// Consumes the next [SendMessageOutput] event.
    async fn next(&mut self) -> Result<Option<ChatResponseStream>, RecvError> {
        if let Some(ev) = self.peek.take() {
            return Ok(Some(ev));
        }
        trace!("Attempting to recv next event");
        let start = std::time::Instant::now();
        let result = self.response.recv().await;
        let duration = std::time::Instant::now().duration_since(start);
        match result {
            Ok(r) => {
                trace!(?r, "Received new event");
                Ok(r)
            },
            Err(err) => {
                if duration.as_secs() >= 59 {
                    Err(self.error(RecvErrorKind::StreamTimeout { source: err, duration }))
                } else {
                    Err(self.error(err))
                }
            },
        }
    }

    fn request_id(&self) -> Option<&str> {
        self.response.request_id()
    }

    /// Helper to create a new [RecvError] populated with the associated request id for the stream.
    fn error(&self, source: impl Into<RecvErrorKind>) -> RecvError {
        RecvError {
            request_id: self.request_id().map(str::to_string),
            source: source.into(),
        }
    }
}

#[derive(Debug)]
pub enum ResponseEvent {
    /// Text returned by the assistant. This should be displayed to the user as it is received.
    AssistantText(String),
    /// Notification that a tool use is being received.
    ToolUseStart { name: String },
    /// A tool use requested by the assistant. This should be displayed to the user as it is
    /// received.
    ToolUse(AssistantToolUse),
    /// Represents the end of the response. No more events will be returned.
    EndStream {
        /// The completed message containing all of the assistant text and tool use events
        /// previously emitted. This should be stored in the conversation history and sent in
        /// subsequent requests.
        message: AssistantMessage,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse() {
        let _ = tracing_subscriber::fmt::try_init();
        let tool_use_id = "TEST_ID".to_string();
        let tool_name = "execute_bash".to_string();
        let tool_args = serde_json::json!({
            "command": "echo hello"
        })
        .to_string();
        let tool_use_split_at = 5;
        let mut events = vec![
            ChatResponseStream::AssistantResponseEvent {
                content: "hi".to_string(),
            },
            ChatResponseStream::AssistantResponseEvent {
                content: " there".to_string(),
            },
            ChatResponseStream::AssistantResponseEvent {
                content: "IGNORE ME PLEASE".to_string(),
            },
            ChatResponseStream::CodeReferenceEvent(()),
            ChatResponseStream::ToolUseEvent {
                tool_use_id: tool_use_id.clone(),
                name: tool_name.clone(),
                input: None,
                stop: None,
            },
            ChatResponseStream::ToolUseEvent {
                tool_use_id: tool_use_id.clone(),
                name: tool_name.clone(),
                input: Some(tool_args.as_str().split_at(tool_use_split_at).0.to_string()),
                stop: None,
            },
            ChatResponseStream::ToolUseEvent {
                tool_use_id: tool_use_id.clone(),
                name: tool_name.clone(),
                input: Some(tool_args.as_str().split_at(tool_use_split_at).1.to_string()),
                stop: None,
            },
            ChatResponseStream::ToolUseEvent {
                tool_use_id: tool_use_id.clone(),
                name: tool_name.clone(),
                input: None,
                stop: Some(true),
            },
        ];
        events.reverse();
        let mock = SendMessageOutput::Mock(events);
        let mut parser = ResponseParser::new(mock);

        for _ in 0..5 {
            println!("{:?}", parser.recv().await.unwrap());
        }
    }
}
