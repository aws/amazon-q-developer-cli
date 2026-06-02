//! End-to-end smoke for the SigV4 wrapper: stand up a tiny hyper server on
//! a loopback port, point [`SigV4HttpClient`] at it, and assert that the
//! request that arrived was signed in the way API Gateway expects (correct
//! `Authorization` shape, `X-Amz-Date`, `X-Amz-Content-Sha256`, and the
//! optional session token when one is present).
//!
//! This catches the class of bug where the wrapper "works" against an
//! unauthenticated mock but silently drops the signing instructions before
//! send.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{
    Arc,
    Mutex,
};
use std::time::{
    Duration,
    SystemTime,
};

use aws_credential_types::Credentials;
use aws_credential_types::provider::SharedCredentialsProvider;
use http_body_util::{
    BodyExt,
    Full,
};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{
    Request,
    Response,
};
use hyper_util::rt::TokioIo;
use kiro_mcp::sigv4_client::SigV4HttpClient;
use tokio::net::TcpListener;

#[derive(Default, Clone)]
struct CapturedHeaders {
    inner: Arc<Mutex<Vec<(String, String)>>>,
}

impl CapturedHeaders {
    fn snapshot(&self) -> Vec<(String, String)> {
        self.inner.lock().unwrap().clone()
    }

    fn record(&self, req: &Request<hyper::body::Incoming>) {
        let mut guard = self.inner.lock().unwrap();
        for (k, v) in req.headers() {
            guard.push((k.as_str().to_string(), v.to_str().unwrap_or("").to_string()));
        }
    }
}

async fn start_capturing_server(headers: CapturedHeaders) -> SocketAddr {
    // bind to an ephemeral port so the test is parallel-safe.
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(c) => c,
                Err(_) => break,
            };
            let captured = headers.clone();
            tokio::spawn(async move {
                let svc = service_fn(move |req: Request<hyper::body::Incoming>| {
                    let captured = captured.clone();
                    async move {
                        captured.record(&req);
                        // Drain the body so the client gets a clean response.
                        let _ = req.into_body().collect().await;
                        Ok::<_, Infallible>(Response::new(Full::new(Bytes::from_static(
                            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}",
                        ))))
                    }
                });
                let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), svc).await;
            });
        }
    });
    addr
}

#[tokio::test]
async fn signed_post_arrives_with_required_sigv4_headers() {
    let captured = CapturedHeaders::default();
    let addr = start_capturing_server(captured.clone()).await;
    let url = format!("http://{addr}/mcp");

    let creds = Credentials::new(
        "AKIDEXAMPLE",
        "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        Some("session-token-xyz".into()),
        Some(SystemTime::now() + Duration::from_secs(3600)),
        "sigv4-smoke-test",
    );
    let provider = SharedCredentialsProvider::new(creds);
    let client = SigV4HttpClient::for_test(reqwest::Client::new(), provider, "us-east-1".into());

    let resp = client
        .post_json(&url, serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
        .await
        .expect("signed post should succeed against the local mock");
    assert!(resp.status().is_success(), "status: {}", resp.status());

    let recorded = captured.snapshot();
    let names: Vec<String> = recorded.iter().map(|(k, _)| k.to_lowercase()).collect();

    // The four headers SigV4 over execute-api MUST emit. We assert presence
    // here; semantic correctness (e.g., signature validity) is the API
    // Gateway's job and is covered by the Phase 0 ECS smoke against the
    // real endpoint.
    for required in [
        "authorization",
        "x-amz-date",
        "x-amz-content-sha256",
        "x-amz-security-token", // present because the test creds include a session token
    ] {
        assert!(
            names.iter().any(|n| n == required),
            "missing required signed header `{required}`; got {names:?}"
        );
    }

    // Regression: we previously set Content-Type both on the http::Request
    // we signed AND again on the reqwest builder, which made the gateway
    // see `content-type: application/json,application/json` and reject the
    // signature. Make sure exactly one Content-Type goes out.
    let content_types: Vec<&str> = recorded
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.as_str())
        .collect();
    assert_eq!(
        content_types.len(),
        1,
        "Content-Type must appear exactly once; got {content_types:?}",
    );
    assert_eq!(content_types[0], "application/json");

    // Authorization must be a SigV4 credential and reference the right
    // service. Catches "we signed but for the wrong service" regressions.
    let auth = recorded
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
        .map(|(_, v)| v.clone())
        .unwrap();
    assert!(
        auth.starts_with("AWS4-HMAC-SHA256 "),
        "Authorization scheme should be SigV4: {auth}"
    );
    assert!(
        auth.contains("/execute-api/aws4_request"),
        "credential scope should reference execute-api: {auth}"
    );
    assert!(
        auth.contains("Credential=AKIDEXAMPLE/"),
        "credential should embed the test AKID: {auth}"
    );
}

#[tokio::test]
async fn omits_security_token_when_creds_have_none() {
    let captured = CapturedHeaders::default();
    let addr = start_capturing_server(captured.clone()).await;
    let url = format!("http://{addr}/mcp");

    let creds = Credentials::new(
        "AKIDLONGTERM",
        "secret",
        None, // long-term creds — no session token
        Some(SystemTime::now() + Duration::from_secs(3600)),
        "long-term",
    );
    let client = SigV4HttpClient::for_test(
        reqwest::Client::new(),
        SharedCredentialsProvider::new(creds),
        "us-east-1".into(),
    );

    let _ = client
        .post_json(&url, serde_json::json!({"x":1}))
        .await
        .expect("signed post should succeed");

    let names: Vec<String> = captured.snapshot().into_iter().map(|(k, _)| k.to_lowercase()).collect();
    assert!(
        !names.iter().any(|n| n == "x-amz-security-token"),
        "long-term creds must not produce X-Amz-Security-Token; got {names:?}"
    );
    assert!(names.iter().any(|n| n == "authorization"));
}
