use futures::TryStreamExt;
use reqwest::Client;
use reqwest_eventsource::{Event, EventSource};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;

use super::base_protocol::JsonRpcMessage;
use super::{Listener, LogListener, Transport, TransportError};

pub struct JsonRpcSseTransport {
    http_client: Client,
    receiver: broadcast::Receiver<Result<JsonRpcMessage, TransportError>>,
    log_receiver: broadcast::Receiver<String>,
    server_url: String,
}

impl std::fmt::Debug for JsonRpcSseTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsonRpcSseTransport")
            .field("http_client", &"<reqwest::Client>")
            .field("server_url", &self.server_url)
            .finish()
    }
}

impl JsonRpcSseTransport {
    pub async fn new(server_url: String) -> Result<Self, TransportError> {
        let http_client = Client::new();
        let (tx, receiver) = broadcast::channel::<Result<JsonRpcMessage, TransportError>>(100);
        let (log_tx, log_receiver) = broadcast::channel::<String>(100);
        
        let event_source = EventSource::get(&server_url);
        
        let tx_clone = tx.clone();
        let mut stream = event_source.into_stream();
        tokio::spawn(async move {
            while let Some(event) = stream.next().await {
                match event {
                    Ok(Event::Message(message)) => {
                        match serde_json::from_str::<JsonRpcMessage>(&message.data) {
                            Ok(rpc_msg) => {
                                let _ = tx_clone.send(Ok(rpc_msg));
                            }
                            Err(e) => {
                                let _ = tx_clone.send(Err(TransportError::Serialization(e.to_string())));
                            }
                        }
                    }
                    Ok(Event::Open) => {
                        let _ = log_tx.send("SSE connection opened".to_string());
                    }
                    Err(e) => {
                        let _ = tx_clone.send(Err(TransportError::Custom(format!("SSE error: {}", e))));
                        break;
                    }
                }
            }
        });

        Ok(Self {
            http_client,
            receiver,
            log_receiver,
            server_url,
        })
    }
}

#[async_trait::async_trait]
impl Transport for JsonRpcSseTransport {
    async fn send(&self, msg: &JsonRpcMessage) -> Result<(), TransportError> {
        let json_data = serde_json::to_string(msg)
            .map_err(|e| TransportError::Serialization(e.to_string()))?;
        
        let response = self.http_client
            .post(&self.server_url)
            .header("Content-Type", "application/json")
            .body(json_data)
            .send()
            .await
            .map_err(|e| TransportError::Custom(format!("HTTP request failed: {}", e)))?;
            
        if !response.status().is_success() {
            return Err(TransportError::Custom(format!("HTTP error: {}", response.status())));
        }
        
        Ok(())
    }

    fn get_listener(&self) -> impl Listener {
        SseListener {
            receiver: self.receiver.resubscribe(),
        }
    }

    async fn shutdown(&self) -> Result<(), TransportError> {
        Ok(())
    }

    fn get_log_listener(&self) -> impl LogListener {
        SseLogListener {
            receiver: self.log_receiver.resubscribe(),
        }
    }
}

pub struct SseListener {
    pub receiver: broadcast::Receiver<Result<JsonRpcMessage, TransportError>>,
}

#[async_trait::async_trait]
impl Listener for SseListener {
    async fn recv(&mut self) -> Result<JsonRpcMessage, TransportError> {
        self.receiver.recv().await?
    }
}

pub struct SseLogListener {
    pub receiver: broadcast::Receiver<String>,
}

#[async_trait::async_trait]
impl LogListener for SseLogListener {
    async fn recv(&mut self) -> Result<String, TransportError> {
        Ok(self.receiver.recv().await?)
    }
} 