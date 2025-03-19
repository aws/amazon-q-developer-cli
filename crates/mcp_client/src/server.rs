use std::collections::HashMap;
use std::sync::atomic::{
    AtomicBool,
    AtomicU64,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
};

use tokio::io::{
    Stdin,
    Stdout,
};
use tokio::task::JoinHandle;

use crate::client::StdioTransport;
use crate::error::ErrorCode;
use crate::transport::base_protocol::{
    JsonRpcError,
    JsonRpcMessage,
    JsonRpcNotification,
    JsonRpcRequest,
    JsonRpcResponse,
};
use crate::transport::stdio::JsonRpcStdioTransport;
use crate::transport::{
    JsonRpcVersion,
    Transport,
    TransportError,
};

pub type Request = serde_json::Value;
pub type Response = Option<serde_json::Value>;
pub type InitializedServer = JoinHandle<Result<(), ServerError>>;

pub trait PreServerRequestHandler {
    fn register_pending_request_callback(&mut self, cb: impl Fn(u64) -> Option<JsonRpcRequest> + Send + Sync + 'static);
    fn register_send_request_callback(
        &mut self,
        cb: impl Fn(&str, Option<serde_json::Value>) -> Result<(), ServerError> + Send + Sync + 'static,
    );
}

#[async_trait::async_trait]
pub trait ServerRequestHandler: PreServerRequestHandler + Send + Sync + 'static {
    async fn handle_initialize(&self, params: Option<serde_json::Value>) -> Result<Response, ServerError>;
    async fn handle_incoming(&self, method: &str, params: Option<serde_json::Value>) -> Result<Response, ServerError>;
    async fn handle_response(&self, resp: JsonRpcResponse) -> Result<(), ServerError>;
    async fn handle_shutdown(&self) -> Result<(), ServerError>;
}

pub struct Server<T: Transport, H: ServerRequestHandler> {
    transport: Option<Arc<T>>,
    handler: Option<H>,
    #[allow(dead_code)]
    pending_requests: Arc<Mutex<HashMap<u64, JsonRpcRequest>>>,
    #[allow(dead_code)]
    current_id: Arc<AtomicU64>,
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error(transparent)]
    TransportError(#[from] TransportError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error("Unexpected msg type encountered")]
    UnexpectedMsgType,
    #[error("{0}")]
    NegotiationError(String),
    #[error(transparent)]
    TokioJoinError(#[from] tokio::task::JoinError),
    #[error("Failed to obtain mutex lock")]
    MutexError,
    #[error("Failed to obtain request method")]
    MissingMethod,
    #[error("Failed to obtain request id")]
    MissingId,
    #[error("Failed to initialize server. Missing transport")]
    MissingTransport,
    #[error("Failed to initialize server. Missing handler")]
    MissingHandler,
}

impl<H> Server<StdioTransport, H>
where
    H: ServerRequestHandler,
{
    pub fn new(mut handler: H, stdin: Stdin, stdout: Stdout) -> Result<Self, ServerError> {
        let transport = Arc::new(JsonRpcStdioTransport::server(stdin, stdout)?);
        let pending_requests = Arc::new(Mutex::new(HashMap::<u64, JsonRpcRequest>::new()));
        let pending_requests_clone_one = pending_requests.clone();
        let current_id = Arc::new(AtomicU64::new(0));
        let pending_request_getter = move |id: u64| -> Option<JsonRpcRequest> {
            match pending_requests_clone_one.lock() {
                Ok(mut p) => p.remove(&id),
                Err(_) => None,
            }
        };
        handler.register_pending_request_callback(pending_request_getter);
        let transport_clone = transport.clone();
        let pending_request_clone_two = pending_requests.clone();
        let current_id_clone = current_id.clone();
        let request_sender = move |method: &str, params: Option<serde_json::Value>| -> Result<(), ServerError> {
            let id = current_id_clone.fetch_add(1, Ordering::SeqCst);
            let request = JsonRpcRequest {
                jsonrpc: JsonRpcVersion::default(),
                id,
                method: method.to_owned(),
                params,
            };
            let msg = JsonRpcMessage::Request(request.clone());
            let transport = transport_clone.clone();
            tokio::task::spawn(async move {
                let _ = transport.send(&msg).await;
            });
            #[allow(clippy::map_err_ignore)]
            let mut pending_request = pending_request_clone_two.lock().map_err(|_| ServerError::MutexError)?;
            pending_request.insert(id, request);
            Ok(())
        };
        handler.register_send_request_callback(request_sender);
        let server = Self {
            transport: Some(transport),
            handler: Some(handler),
            pending_requests,
            current_id,
        };
        Ok(server)
    }
}

impl<T, H> Server<T, H>
where
    T: Transport,
    H: ServerRequestHandler,
{
    pub fn init(mut self) -> Result<InitializedServer, ServerError> {
        let transport = self.transport.take().ok_or(ServerError::MissingTransport)?;
        let handler = Arc::new(self.handler.take().ok_or(ServerError::MissingHandler)?);
        let has_initialized = Arc::new(AtomicBool::new(false));
        let listener = tokio::spawn(async move {
            loop {
                let request = transport.monitor().await;
                let transport_clone = transport.clone();
                let has_init_clone = has_initialized.clone();
                let handler_clone = handler.clone();
                tokio::task::spawn(async move {
                    process_request(has_init_clone, transport_clone, handler_clone, request).await;
                });
            }
        });
        Ok(listener)
    }
}

async fn process_request<T, H>(
    has_initialized: Arc<AtomicBool>,
    transport: Arc<T>,
    handler: Arc<H>,
    request: Result<JsonRpcMessage, TransportError>,
) where
    T: Transport,
    H: ServerRequestHandler,
{
    match request {
        Ok(msg) if msg.is_initialize() => {
            let id = msg.id().unwrap_or_default();
            if has_initialized.load(Ordering::SeqCst) {
                let resp = JsonRpcMessage::Response(JsonRpcResponse {
                    jsonrpc: JsonRpcVersion::default(),
                    id,
                    error: Some(JsonRpcError {
                        code: ErrorCode::InvalidRequest.into(),
                        message: "Server has already been initialized".to_owned(),
                        data: None,
                    }),
                    ..Default::default()
                });
                let _ = transport.send(&resp).await;
                return;
            }
            let JsonRpcMessage::Request(req) = msg else {
                let resp = JsonRpcMessage::Response(JsonRpcResponse {
                    jsonrpc: JsonRpcVersion::default(),
                    id,
                    error: Some(JsonRpcError {
                        code: ErrorCode::InvalidRequest.into(),
                        message: "Invalid method for initialization (use request)".to_owned(),
                        data: None,
                    }),
                    ..Default::default()
                });
                let _ = transport.send(&resp).await;
                return;
            };
            let JsonRpcRequest { params, .. } = req;
            match handler.handle_initialize(params).await {
                Ok(result) => {
                    let resp = JsonRpcMessage::Response(JsonRpcResponse {
                        id,
                        result,
                        ..Default::default()
                    });
                    let _ = transport.send(&resp).await;
                    has_initialized.store(true, Ordering::SeqCst);
                },
                Err(_e) => {
                    let resp = JsonRpcMessage::Response(JsonRpcResponse {
                        jsonrpc: JsonRpcVersion::default(),
                        id,
                        error: Some(JsonRpcError {
                            code: ErrorCode::InternalError.into(),
                            message: "Error producing initialization response".to_owned(),
                            data: None,
                        }),
                        ..Default::default()
                    });
                    let _ = transport.send(&resp).await;
                },
            }
        },
        Ok(msg) if msg.is_shutdown() => {
            // TODO: add shutdown routine
        },
        Ok(msg) if has_initialized.load(Ordering::SeqCst) => match msg {
            JsonRpcMessage::Request(req) => {
                let JsonRpcRequest {
                    id,
                    jsonrpc,
                    params,
                    ref method,
                } = req;
                let resp = handler.handle_incoming(method, params).await.map_or_else(
                    |error| {
                        let err = JsonRpcError {
                            code: ErrorCode::InternalError.into(),
                            message: error.to_string(),
                            data: None,
                        };
                        let resp = JsonRpcResponse {
                            jsonrpc: jsonrpc.clone(),
                            id,
                            result: None,
                            error: Some(err),
                        };
                        JsonRpcMessage::Response(resp)
                    },
                    |result| {
                        let resp = JsonRpcResponse {
                            jsonrpc: jsonrpc.clone(),
                            id,
                            result,
                            error: None,
                        };
                        JsonRpcMessage::Response(resp)
                    },
                );
                let _ = transport.send(&resp).await;
            },
            JsonRpcMessage::Notification(notif) => {
                let JsonRpcNotification { ref method, params, .. } = notif;
                let _ = handler.handle_incoming(method, params).await;
            },
            JsonRpcMessage::Response(resp) => {
                let _ = handler.handle_response(resp).await;
            },
        },
        Ok(msg) => {
            let id = msg.id().unwrap_or_default();
            let resp = JsonRpcMessage::Response(JsonRpcResponse {
                jsonrpc: JsonRpcVersion::default(),
                id,
                error: Some(JsonRpcError {
                    code: ErrorCode::ServerNotInitialized.into(),
                    message: "Server has not been initialized".to_owned(),
                    data: None,
                }),
                ..Default::default()
            });
            let _ = transport.send(&resp).await;
        },
        Err(_e) => {
            // TODO: error handling
        },
    }
}
