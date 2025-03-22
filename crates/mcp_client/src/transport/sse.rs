use std::sync::Arc;

use futures_util::StreamExt;
use reqwest::Url;
use reqwest_eventsource::{Event, EventSource};
use tokio::sync::broadcast;
use tokio::sync::RwLock;
use super::{JsonRpcMessage, Transport, TransportError};

/// SSE (Server-Sent Events) transport implementation for the MCP client.
///
/// This transport allows for receiving server-sent events over HTTP and sending
/// messages back to the server via a separate HTTP request.
#[derive(Debug)]
pub struct JsonRpcSseTransport {
    /// Used for the monitor
    exclusive_receiver: Arc<tokio::sync::Mutex<broadcast::Receiver<Result<JsonRpcMessage, TransportError>>>>,
    /// Used for the listen
    shared_receiver: Arc<tokio::sync::Mutex<broadcast::Receiver<Result<JsonRpcMessage, TransportError>>>>,
    /// HTTP client for making requests
    client: reqwest::Client,
    /// The url to send messages to
    post_url: Arc<RwLock<Option<String>>>,
}

impl JsonRpcSseTransport {
    pub fn client(sse_url: String) -> Result<Self, TransportError> {
        let (tx, receiver) = broadcast::channel::<Result<JsonRpcMessage, TransportError>>(100);
        let exclusive_receiver = Arc::new(tokio::sync::Mutex::new(receiver));
        let shared_receiver = Arc::new(tokio::sync::Mutex::new(tx.subscribe()));

        let post_url = Arc::new(RwLock::new(None));
        let post_url_clone = Arc::clone(&post_url);
        let sse_url_clone = sse_url.clone();

        tracing::debug!("Connecting to: {}", sse_url);

        tokio::spawn(async move {
            match Self::connect_sse(&sse_url_clone).await {
                Ok(mut event_source) => {
                    tracing::debug!("Connected to SSE endpoint: {}", sse_url_clone);

                    // todo: correct protocol error handling
                    while let Some(event) = event_source.next().await {
                        match event {
                            Ok(Event::Open) => tracing::debug!("Connection opened to: {}", sse_url_clone),
                            Ok(Event::Message(message)) if message.event == "endpoint" => {
                                let base_url = Url::parse(&sse_url_clone).expect("SSE URL is invalid");
                                let maybe_post_url = base_url.join(&message.data).map(|url| url.to_string());
                                tracing::debug!("POST URL: {} ", maybe_post_url.clone().expect("Invalid POST URL"));
                                *post_url_clone.write().await = maybe_post_url.ok();
                            }
                            Ok(Event::Message(message)) if message.event == "message" => {
                                tracing::debug!("message {} {} {}", message.id, message.event, message.data);
                                match serde_json::from_str::<JsonRpcMessage>(&message.data) {
                                    Ok(msg) => {
                                        let _ = tx.send(Ok(msg));
                                    },
                                    Err(e) => {
                                        let _ = tx.send(Err(e.into()));
                                    }
                                }
                            }
                            Ok(Event::Message(message)) => {
                                // todo: error handling
                                tracing::error!("Unexpected message: {} {}", message.event, message.data);
                            }
                            Err(err) => {
                                // todo: error handling
                                tracing::error!("SSE event error: {}", err);
                                event_source.close();
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    // todo: error handling?
                    tracing::error!("Failed to connect to SSE endpoint: {}", err);
                }
            }
        });

        Ok(JsonRpcSseTransport{
            exclusive_receiver,
            shared_receiver,
            client: reqwest::Client::new(),
            post_url,
        })
    }

    async fn connect_sse(url: &str) -> Result<EventSource, TransportError> {
        let client = reqwest::Client::new();
        let event_source = EventSource::new(client.get(url))
            .map_err(|e| TransportError::Custom(format!("Failed to create EventSource: {}", e)))?;

        Ok(event_source)
    }
}

#[async_trait::async_trait]
impl Transport for JsonRpcSseTransport {
    async fn send(&self, msg: &JsonRpcMessage) -> Result<(), TransportError> {
        // Wait for the post_url to be set with a timeout
        const MAX_WAIT_SECONDS: u64 = 10;
        const CHECK_INTERVAL_MS: u64 = 100;
        let mut attempts = 0;
        let max_attempts = (MAX_WAIT_SECONDS * 1000) / CHECK_INTERVAL_MS;

        let url = loop {
            if let Some(url) = self.post_url.read().await.clone() {
                break url;
            }

            attempts += 1;
            if attempts >= max_attempts {
                return Err(TransportError::Custom(
                    "Timed out waiting for POST URL to be set".to_string()
                ));
            }

            tracing::debug!("Waiting for POST URL to be set... (attempt {}/{})", attempts, max_attempts);
            tokio::time::sleep(tokio::time::Duration::from_millis(CHECK_INTERVAL_MS)).await;
        };

        tracing::debug!("Sending message to: {}", url);

        let json_str = serde_json::to_string(msg)
            .map_err(|e| TransportError::Custom(format!("Failed to serialize message: {}", e)))?;
        tracing::debug!("Sending message via SSE transport: {}", json_str);

        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(json_str)
            .send()
            .await
            .map_err(|e| TransportError::Custom(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await
                .unwrap_or_else(|_| "Failed to read response body".to_string());

            return Err(TransportError::Custom(format!(
                "Server returned error status: {} - {}",
                status, body
            )));
        }

        Ok(())
    }

    async fn listen(&self) -> Result<JsonRpcMessage, TransportError> {
        // The STDIO impl uses a resubscribe but that approach misses messages in our case
        // So this uses the same Mutex approach as the monitor which maybe makes sense
        //   since the Receiver can't be used concurrently anyway
        self.shared_receiver.lock().await.recv().await?
    }

    async fn monitor(&self) -> Result<JsonRpcMessage, TransportError> {
        self.exclusive_receiver.lock().await.recv().await?
    }
}
