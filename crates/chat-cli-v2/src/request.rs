use std::env::current_exe;
use std::sync::{
    Arc,
    LazyLock,
};
use std::time::Duration;

use reqwest::Client;
use rustls::{
    ClientConfig,
    RootCertStore,
};
use thiserror::Error;
use url::ParseError;

#[derive(Debug, Error)]
pub enum RequestError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Dir(#[from] crate::util::paths::DirectoryError),
    #[error(transparent)]
    Settings(#[from] crate::database::DatabaseError),
    #[error(transparent)]
    UrlParseError(#[from] ParseError),
}

/// Evict idle pooled connections after this. reqwest's 90s default outlives the ~60s idle timeout
/// common to NAT and firewall middleboxes, leaving a window where a pooled connection the network
/// has already dropped gets reused and the request fails as an unretryable "dispatch failure". A
/// value below that window forces a fresh connection instead. (TCP keepalives, which also help, are
/// already enabled by reqwest's defaults.)
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

// `pool_idle_timeout` is a parameter so the eviction test can drive this exact builder on a short
// timeout; production always passes `POOL_IDLE_TIMEOUT`.
fn client_builder(pool_idle_timeout: Duration) -> reqwest::ClientBuilder {
    Client::builder()
        .use_preconfigured_tls(client_config())
        .user_agent(USER_AGENT.chars().filter(|c| c.is_ascii_graphic()).collect::<String>())
        .cookie_store(true)
        .pool_idle_timeout(pool_idle_timeout)
}

pub fn new_client() -> Result<Client, RequestError> {
    Ok(client_builder(POOL_IDLE_TIMEOUT).build()?)
}

pub fn create_default_root_cert_store() -> RootCertStore {
    let mut root_cert_store: RootCertStore = webpki_roots::TLS_SERVER_ROOTS.iter().cloned().collect();

    // The errors are ignored because root certificates often include
    // ancient or syntactically invalid certificates
    let rustls_native_certs::CertificateResult { certs, errors: _, .. } = rustls_native_certs::load_native_certs();
    for cert in certs {
        let _ = root_cert_store.add(cert);
    }

    root_cert_store
}

fn client_config() -> ClientConfig {
    static TLS_CONFIG: LazyLock<ClientConfig> = LazyLock::new(|| {
        let provider = rustls::crypto::CryptoProvider::get_default()
            .cloned()
            .unwrap_or_else(|| Arc::new(rustls::crypto::aws_lc_rs::default_provider()));

        ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(rustls::DEFAULT_VERSIONS)
            .expect("Failed to set supported TLS versions")
            .with_root_certificates(create_default_root_cert_store())
            .with_no_client_auth()
    });

    TLS_CONFIG.clone()
}

static USER_AGENT: LazyLock<String> = LazyLock::new(|| {
    let name = current_exe()
        .ok()
        .and_then(|exe| exe.file_stem().and_then(|name| name.to_str().map(String::from)))
        .unwrap_or_else(|| "unknown-rust-client".into());

    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let version = env!("CARGO_PKG_VERSION");

    format!("{name}-{os}-{arch}-{version}")
});

#[cfg(test)]
mod tests {
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };
    use tokio::net::{
        TcpListener,
        TcpStream,
    };
    use tokio::sync::mpsc::{
        UnboundedReceiver,
        UnboundedSender,
    };

    use super::*;

    #[tokio::test]
    async fn get_client() {
        new_client().unwrap();
    }

    #[tokio::test]
    async fn request_test() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/hello")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("world")
            .create();
        let url = server.url();

        let client = new_client().unwrap();
        let res = client.get(format!("{url}/hello")).send().await.unwrap();
        assert_eq!(res.status(), 200);
        assert_eq!(res.headers()["content-type"], "text/plain");
        assert_eq!(res.text().await.unwrap(), "world");

        mock.expect(1).assert();
    }

    /// Reads one bodyless HTTP/1.1 request head off `stream`, stopping at the terminating empty
    /// line. Byte-at-a-time keeps the socket buffer clean so a keep-alive connection can serve the
    /// next request.
    async fn read_request_head(stream: &mut TcpStream) -> std::io::Result<()> {
        let mut head = Vec::new();
        let mut byte = [0_u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).await? == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "connection closed",
                ));
            }
            head.push(byte[0]);
        }
        Ok(())
    }

    /// Serves keep-alive responses on every accepted connection, reporting over `tx` the id of the
    /// connection that carried each request. Speculative connections that never send a request
    /// never report, so reuse (same id twice) is distinguishable from a fresh dial (two ids).
    async fn serve_tagging_connections(listener: TcpListener, tx: UnboundedSender<usize>) {
        let mut next_id = 0_usize;
        while let Ok((mut conn, _)) = listener.accept().await {
            let id = next_id;
            next_id += 1;
            let tx = tx.clone();
            tokio::spawn(async move {
                while read_request_head(&mut conn).await.is_ok() {
                    if tx.send(id).is_err() {
                        break;
                    }
                    if conn
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
    }

    async fn get_ok(client: &Client, url: &str) {
        let body = client.get(url).send().await.unwrap().bytes().await.unwrap();
        assert_eq!(body.as_ref(), b"ok");
    }

    async fn next_served_conn_id(rx: &mut UnboundedReceiver<usize>) -> usize {
        tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("server did not serve the request in time")
            .expect("server closed the channel")
    }

    /// A request issued after the pool idle timeout must open a fresh connection rather than reuse
    /// a pooled one — that is what stops a middlebox-dropped socket from being written to. The
    /// short timeout runs through the production `client_builder`, so dropping
    /// `pool_idle_timeout` there makes this fail.
    #[tokio::test]
    async fn idle_pooled_connection_is_evicted_not_reused() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(serve_tagging_connections(listener, tx));

        let idle = Duration::from_millis(50);
        let client = client_builder(idle).build().unwrap();
        let url = format!("http://{addr}/");

        get_ok(&client, &url).await;
        tokio::time::sleep(idle * 4).await;
        get_ok(&client, &url).await;

        let first = next_served_conn_id(&mut rx).await;
        let second = next_served_conn_id(&mut rx).await;
        assert_ne!(
            first, second,
            "a request after the idle timeout must use a fresh connection, not the evicted pooled one"
        );
    }

    /// Control: kept warm (no idle gap), the pooled connection is reused. Without this the eviction
    /// test could pass against a client that never pools connections at all.
    #[tokio::test]
    async fn warm_pooled_connection_is_reused() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(serve_tagging_connections(listener, tx));

        let client = client_builder(Duration::from_secs(30)).build().unwrap();
        let url = format!("http://{addr}/");

        get_ok(&client, &url).await;
        get_ok(&client, &url).await;

        let first = next_served_conn_id(&mut rx).await;
        let second = next_served_conn_id(&mut rx).await;
        assert_eq!(first, second, "a warm pooled connection must be reused");
    }

    #[test]
    fn pool_idle_timeout_is_below_common_middlebox_window() {
        assert!(
            POOL_IDLE_TIMEOUT < Duration::from_secs(60),
            "pool idle timeout must stay below the ~60s middlebox idle window"
        );
    }
}
