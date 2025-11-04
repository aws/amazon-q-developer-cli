use std::pin::Pin;
use std::sync::{
    Arc,
    Mutex,
};
use std::time::Duration;

use futures::Stream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{
    debug,
    error,
    trace,
};

use super::protocol::{
    SendRequestArgs,
    StreamResult,
};
use super::types::{
    Message,
    ToolSpec,
};

/// Represents a backend implementation for a converse stream compatible API.
///
/// **Important** - implementations should be cancel safe
pub trait Model: std::fmt::Debug + Send + Sync + 'static {
    /// Sends a conversation to a model, returning a stream of events as the response.
    fn stream(
        &self,
        messages: Vec<Message>,
        tool_specs: Option<Vec<ToolSpec>>,
        system_prompt: Option<String>,
        cancel_token: CancellationToken,
    ) -> Pin<Box<dyn Stream<Item = StreamResult> + Send + 'static>>;

    /// Dump serializable state required by the model implementation.
    ///
    /// This is intended to provide the ability to save and restore state
    /// associated with an implementation, useful for restoring a previous conversation.
    fn state(&self) -> Option<serde_json::Value> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct MockModel {
    inner: Arc<Mutex<mock::Inner>>,
}

impl MockModel {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(mock::Inner::new())),
        }
    }

    pub fn with_response(self, response: impl Into<MockResponse>) -> Self {
        self.inner.lock().unwrap().mock_responses.push(response.into());
        self
    }
}

impl Default for MockModel {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Default)]
pub struct MockResponse {
    items: Vec<StreamResult>,
    /// Delay before sending the first stream result.
    time_to_first_chunk_delay: Option<Duration>,
}

impl MockResponse {
    async fn stream(self, tx: mpsc::Sender<StreamResult>) {
        trace!(?self.items, "beginning stream for mock response");
        if let Some(delay) = self.time_to_first_chunk_delay {
            debug!(?self.time_to_first_chunk_delay, "sleeping before sending first event");
            tokio::time::sleep(delay).await;
        }
        for item in self.items {
            let _ = tx.send(item).await;
        }
    }
}

impl From<Vec<StreamResult>> for MockResponse {
    fn from(value: Vec<StreamResult>) -> Self {
        Self {
            items: value,
            ..Default::default()
        }
    }
}

impl Model for MockModel {
    fn stream(
        &self,
        messages: Vec<Message>,
        tool_specs: Option<Vec<ToolSpec>>,
        system_prompt: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Pin<Box<dyn Stream<Item = StreamResult> + Send + 'static>> {
        let req = SendRequestArgs {
            messages: messages.clone(),
            tool_specs: tool_specs.clone(),
            system_prompt: system_prompt.clone(),
        };
        let mut r = self.inner.lock().unwrap();
        let Some(mock_response) = r.mock_responses.get(r.response_index).cloned() else {
            error!("received an unexpected request: {:?}", req);
            panic!("received an unexpected request: {req:?}");
        };
        r.received_requests.push(req);
        r.response_index += 1;

        let (tx, rx) = mpsc::channel(32);
        tokio::spawn(async move {
            mock_response.stream(tx).await;
        });
        Box::pin(ReceiverStream::new(rx))
    }
}

mod mock {
    use super::*;

    #[derive(Debug, Clone)]
    pub(super) struct Inner {
        /// Current index into [Self::mock_responses].
        pub response_index: usize,
        pub mock_responses: Vec<MockResponse>,
        pub received_requests: Vec<SendRequestArgs>,
    }

    impl Inner {
        pub(super) fn new() -> Self {
            Self {
                response_index: 0,
                mock_responses: Vec::new(),
                received_requests: Vec::new(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_loop::types::{
        ContentBlockDelta,
        ContentBlockDeltaEvent,
        MessageStartEvent,
        MessageStopEvent,
        Role,
        StopReason,
        StreamEvent,
    };

    fn make_mock_response(input: &str) -> Vec<StreamResult> {
        vec![
            StreamResult::Ok(StreamEvent::MessageStart(MessageStartEvent { role: Role::Assistant })),
            StreamResult::Ok(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                delta: ContentBlockDelta::Text(input.to_string()),
                content_block_index: None,
            })),
            StreamResult::Ok(StreamEvent::MessageStop(MessageStopEvent {
                stop_reason: StopReason::EndTurn,
            })),
        ]
    }

    async fn consume_response(
        mut response: Pin<Box<dyn Stream<Item = StreamResult> + Send + 'static>>,
    ) -> Vec<StreamResult> {
        use futures::StreamExt;
        let mut events = Vec::new();
        while let Some(evt) = response.next().await {
            events.push(evt);
        }
        events
    }

    fn assert_contains_text(events: &[StreamResult], expected: &str) {
        assert!(events.iter().any(
            |evt| matches!(evt, StreamResult::Ok(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                delta: ContentBlockDelta::Text(text),
                ..
            })) if text.contains(expected))
        ));
    }

    #[tokio::test]
    async fn test_mock_model() {
        let model = MockModel::new()
            .with_response(make_mock_response("first"))
            .with_response(make_mock_response("second"));

        let result = model.stream(vec![], None, None, CancellationToken::new());
        let events = consume_response(result).await;
        assert_contains_text(&events, "first");

        let result = model.stream(vec![], None, None, CancellationToken::new());
        let events = consume_response(result).await;
        assert_contains_text(&events, "second");
    }
}
