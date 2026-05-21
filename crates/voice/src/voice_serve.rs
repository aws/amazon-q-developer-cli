use std::convert::Infallible;
use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

use eyre::Result;
use http_body_util::combinators::BoxBody;
use http_body_util::{
    BodyExt,
    StreamBody,
};
use hyper::body::{
    Bytes,
    Frame,
    Incoming,
};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{
    Method,
    Request,
    Response,
    StatusCode,
};
use hyper_util::rt::TokioIo;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::net::TcpListener;
use tokio::sync::{
    Mutex,
    mpsc,
};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{
    error,
    info,
};

use super::voice_handler::VoiceHandler;

type DynBody = BoxBody<Bytes, Infallible>;

fn json_response(status: StatusCode, body: impl Serialize) -> Response<DynBody> {
    let json = serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string());
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Connection", "close")
        .header("Access-Control-Allow-Origin", "http://localhost")
        .header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        .header("Access-Control-Allow-Headers", "Content-Type")
        .body(
            http_body_util::Full::new(Bytes::from(json))
                .map_err(|e| match e {})
                .boxed(),
        )
        .unwrap()
}

fn sse_response(rx: mpsc::Receiver<String>) -> Response<DynBody> {
    let stream = ReceiverStream::new(rx).map(|s| Ok::<Frame<Bytes>, Infallible>(Frame::data(Bytes::from(s))));
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Connection", "close")
        .header("Access-Control-Allow-Origin", "http://localhost")
        .body(StreamBody::new(stream).boxed())
        .unwrap()
}

fn sse_event(data: impl Serialize) -> String {
    format!("data: {}\n\n", serde_json::to_string(&data).unwrap_or_default())
}

#[derive(Deserialize)]
struct RecordRequest {
    #[serde(default)]
    context_hint: Option<String>,
    #[serde(default)]
    model_size: Option<String>,
    #[serde(default)]
    language: Option<String>,
}

#[derive(Serialize)]
struct RecordResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct StatusResponse {
    status: String,
    mic_available: bool,
    version: String,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SseEvent {
    Activity { level: u8 },
    Done { text: Option<String> },
    Error { message: String },
}

async fn parse_record_request(req: Request<Incoming>) -> Result<RecordRequest, Response<DynBody>> {
    let body_bytes = match http_body_util::BodyExt::collect(req.into_body()).await {
        Ok(c) => c.to_bytes(),
        Err(e) => {
            return Err(json_response(StatusCode::BAD_REQUEST, RecordResponse {
                text: None,
                error: Some(format!("Failed to read body: {e}")),
            }));
        },
    };
    if body_bytes.is_empty() {
        return Ok(RecordRequest {
            context_hint: None,
            model_size: None,
            language: None,
        });
    }
    let mut req: RecordRequest = serde_json::from_slice(&body_bytes).map_err(|e| {
        json_response(StatusCode::BAD_REQUEST, RecordResponse {
            text: None,
            error: Some(format!("Invalid JSON: {e}")),
        })
    })?;
    // Cap context_hint to 1KB to prevent oversized payloads
    if let Some(ref hint) = req.context_hint
        && hint.len() > 1024
    {
        let end = hint.floor_char_boundary(1024);
        req.context_hint = Some(hint[..end].to_string());
    }
    Ok(req)
}

async fn handle_request(req: Request<Incoming>, state: Arc<Mutex<()>>) -> Result<Response<DynBody>, hyper::Error> {
    if req.method() == Method::OPTIONS {
        return Ok(json_response(StatusCode::OK, serde_json::json!({})));
    }

    let path = req.uri().path().to_string();
    let method = req.method().clone();

    match (method, path.as_str()) {
        (Method::GET, "/voice/status") => {
            let mic_available = super::audio_capture::request_microphone_permission().is_ok();
            Ok(json_response(StatusCode::OK, StatusResponse {
                status: "ok".to_string(),
                mic_available,
                version: env!("CARGO_PKG_VERSION").to_string(),
            }))
        },

        (Method::POST, "/voice/record") => {
            // Serialize mic access — only one recording at a time
            let _guard = state.lock().await;

            let record_req = match parse_record_request(req).await {
                Ok(r) => r,
                Err(e) => return Ok(e),
            };

            info!("Starting voice recording (batch)...");
            let model_size = record_req.model_size.unwrap_or_else(|| "base".to_string());
            let context_hint = record_req.context_hint;
            let language = record_req.language;

            // VoiceHandler (cpal::Stream) is not Send — run on OS thread
            let rt = tokio::runtime::Handle::current();
            let (tx, rx) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                rt.block_on(async move {
                    let resp = match VoiceHandler::with_language(
                        super::transcription_provider::TranscriptionBackend::LocalWhisper,
                        context_hint,
                        Some(model_size),
                        language,
                        None,
                        None,
                    )
                    .await
                    {
                        Ok(mut handler) => match handler.listen_headless().await {
                            Ok(Some(text)) => {
                                info!("Transcribed: {}", text);
                                json_response(StatusCode::OK, RecordResponse {
                                    text: Some(text),
                                    error: None,
                                })
                            },
                            Ok(None) => json_response(StatusCode::OK, RecordResponse {
                                text: None,
                                error: None,
                            }),
                            Err(e) => {
                                error!("Recording failed: {}", e);
                                json_response(StatusCode::INTERNAL_SERVER_ERROR, RecordResponse {
                                    text: None,
                                    error: Some(format!("Recording failed: {e}")),
                                })
                            },
                        },
                        Err(e) => {
                            error!("Voice init failed: {}", e);
                            json_response(StatusCode::INTERNAL_SERVER_ERROR, RecordResponse {
                                text: None,
                                error: Some(format!("Voice init failed: {e}")),
                            })
                        },
                    };
                    let _ = tx.send(resp);
                });
            });

            match rx.await {
                Ok(resp) => Ok(resp),
                Err(_) => Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, RecordResponse {
                    text: None,
                    error: Some("Recording thread panicked".to_string()),
                })),
            }
        },

        // SSE streaming endpoint — sends activity levels then final transcript
        (Method::POST, "/voice/record/stream") => {
            // Serialize mic access — only one recording at a time.
            // We hold the guard through setup and drop before returning the SSE response;
            // the spawned OS thread inherits the active recording session.
            let guard = state.lock().await;

            let record_req = match parse_record_request(req).await {
                Ok(r) => r,
                Err(e) => return Ok(e),
            };

            info!("Starting voice recording (streaming)...");

            let (sse_tx, sse_rx) = mpsc::channel::<String>(200);
            let (activity_tx, mut activity_rx) = mpsc::channel::<u8>(200);

            // Forward activity levels to SSE channel (both are Send)
            let sse_activity = sse_tx.clone();
            tokio::spawn(async move {
                while let Some(level) = activity_rx.recv().await {
                    if sse_activity
                        .send(sse_event(SseEvent::Activity { level }))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });

            // Recording task — VoiceHandler (cpal::Stream) is not Send, use OS thread
            let model_size = record_req.model_size.unwrap_or_else(|| "base".to_string());
            let context_hint = record_req.context_hint;
            let language = record_req.language;
            let rt = tokio::runtime::Handle::current();

            std::thread::spawn(move || {
                rt.block_on(async move {
                    match VoiceHandler::with_language(
                        super::transcription_provider::TranscriptionBackend::LocalWhisper,
                        context_hint,
                        Some(model_size),
                        language,
                        None,
                        None,
                    )
                    .await
                    {
                        Ok(mut handler) => match handler.listen_headless_with_activity(activity_tx).await {
                            Ok(text) => {
                                let _ = sse_tx.send(sse_event(SseEvent::Done { text })).await;
                            },
                            Err(e) => {
                                let _ = sse_tx.send(sse_event(SseEvent::Error { message: e.to_string() })).await;
                            },
                        },
                        Err(e) => {
                            let _ = sse_tx
                                .send(sse_event(SseEvent::Error {
                                    message: format!("Voice init failed: {e}"),
                                }))
                                .await;
                        },
                    }
                });
            });

            // Release the lock — the OS thread owns the recording session now
            drop(guard);
            Ok(sse_response(sse_rx))
        },

        _ => Ok(json_response(
            StatusCode::NOT_FOUND,
            serde_json::json!({ "error": "Not found" }),
        )),
    }
}

pub async fn run_voice_server(bind: &str, port: u16) -> Result<ExitCode> {
    let addr: SocketAddr = format!("{bind}:{port}")
        .parse()
        .map_err(|e| eyre::eyre!("Invalid bind address: {e}"))?;

    if !addr.ip().is_loopback() {
        eprintln!(
            "WARNING: Binding to non-loopback address {addr}. The voice server has no authentication — only bind to localhost in production."
        );
    }

    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| eyre::eyre!("Failed to bind to {addr}: {e}"))?;

    eprintln!("Voice server listening on http://{addr}");
    eprintln!();
    eprintln!("Endpoints:");
    eprintln!("  GET  /voice/status         - Check server status");
    eprintln!("  POST /voice/record         - Record and transcribe");
    eprintln!("  POST /voice/record/stream  - Record with real-time activity (SSE)");
    eprintln!();
    eprintln!("Press Ctrl+C to stop.");

    let state = Arc::new(Mutex::new(()));

    loop {
        let (stream, remote_addr) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let state = state.clone();

        info!("Connection from {}", remote_addr);

        let service = service_fn(move |req| {
            let state = state.clone();
            handle_request(req, state)
        });

        tokio::spawn(async move {
            if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                error!("Connection error: {}", err);
            }
        });
    }
}
