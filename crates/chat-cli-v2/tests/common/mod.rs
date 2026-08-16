//! Common test utilities for ACP integration tests.
#![allow(dead_code, unused)]

mod acp_client;
#[cfg(unix)]
mod harness;
mod paths;

pub use acp_client::{
    AcpTestClient,
    CapturedNotifications,
    PermissionResponse,
    text_content,
};
#[cfg(unix)]
pub use harness::{
    AcpTestHarness,
    AcpTestHarnessBuilder,
    parse_mock_response_streams,
};
pub use paths::{
    TestPaths,
    checkout_hash,
};

/// Minimal HTTP endpoint that returns a fixed JSON body for any GET. Stands in
/// for the MCP registry the agent fetches via `KIRO_MCP_REGISTRY_URL_OVERRIDE`.
/// Returns the URL and a task handle (aborted on drop by the caller).
pub async fn spawn_mock_registry(body: String) -> (String, tokio::task::JoinHandle<()>) {
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind registry listener");
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}/registry");

    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let body = body.clone();
            tokio::spawn(async move {
                // Drain the request (a single GET fits in one read); we serve the
                // same body regardless of path.
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes()).await;
                let _ = stream.flush().await;
            });
        }
    });

    (url, task)
}
