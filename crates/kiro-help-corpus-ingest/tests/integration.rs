//! Integration tests for the live HTTP source. Spins up a tiny TCP server
//! that speaks just enough HTTP/1.1 to satisfy reqwest, and points
//! `GithubIssuesSource::with_base_url` at it.

use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::sync::Arc;

use kiro_help_corpus_ingest::Source;
use kiro_help_corpus_ingest::github_http::GithubIssuesSource;
use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::net::TcpListener;

const PAGE_1_BODY: &str = r#"[
  {"number":42,"title":"Login hangs","body":"...","state":"open","updated_at":"2026-05-01T12:00:00Z"},
  {"number":43,"title":"PR foo","body":"...","state":"open","updated_at":"2026-05-02T12:00:00Z","pull_request":{"url":"x"}}
]"#;

const PAGE_2_BODY: &str = r#"[
  {"number":44,"title":"Another bug","body":"...","state":"closed","updated_at":"2026-05-03T12:00:00Z"}
]"#;

async fn serve(listener: TcpListener, base_for_link: String) -> Arc<AtomicUsize> {
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_for_task = counter.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let counter = counter_for_task.clone();
            let base = base_for_link.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let n = socket.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();

                let n_calls = counter.fetch_add(1, Ordering::SeqCst);
                let (body, link) = match n_calls {
                    0 => (
                        PAGE_1_BODY,
                        format!("<{base}/repos/foo/bar/issues?page=2>; rel=\"next\""),
                    ),
                    _ => (PAGE_2_BODY, String::new()),
                };
                let mut resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
                    body.len()
                );
                if !link.is_empty() {
                    resp.push_str(&format!("Link: {link}\r\n"));
                }
                resp.push_str("\r\n");
                resp.push_str(body);
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.shutdown().await;
                let _ = req; // silence unused
            });
        }
    });
    counter
}

#[tokio::test]
async fn github_issues_source_paginates_via_link_header() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{addr}");
    let counter = serve(listener, base_url.clone()).await;

    let src = GithubIssuesSource::new("foo/bar", None).with_base_url(base_url.clone());
    let chunks = src.fetch().await.unwrap();

    // 3 issues across both pages, but #43 is a PR and is dropped.
    assert_eq!(chunks.len(), 2, "expected #42 and #44, got {chunks:?}");
    let paths: Vec<&str> = chunks.iter().map(|c| c.source_path.as_str()).collect();
    assert!(paths.contains(&"github_issue:foo/bar#42"));
    assert!(paths.contains(&"github_issue:foo/bar#44"));
    // Server saw two requests (page 1 + page 2 from Link rel=next).
    assert_eq!(counter.load(Ordering::SeqCst), 2);
}
