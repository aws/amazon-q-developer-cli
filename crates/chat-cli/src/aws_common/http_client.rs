use std::time::Duration;

use aws_smithy_runtime_api::client::http::{
    HttpClient,
    HttpConnector,
    HttpConnectorFuture,
    HttpConnectorSettings,
    SharedHttpConnector,
};
use aws_smithy_runtime_api::client::result::ConnectorError;
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_runtime_api::http::Request;
use aws_smithy_types::body::SdkBody;
use reqwest::Client as ReqwestClient;

/// Returns a wrapper around the global [fig_request::client] that implements
/// [HttpClient].
pub fn client() -> Client {
    let client = crate::request::new_client().expect("failed to create http client");
    Client::new(client.clone())
}

/// A wrapper around [reqwest::Client] that implements [HttpClient].
///
/// This is required to support using proxy servers with the AWS SDK.
#[derive(Debug, Clone)]
pub struct Client {
    inner: ReqwestClient,
}

impl Client {
    pub fn new(client: ReqwestClient) -> Self {
        Self { inner: client }
    }
}

#[derive(Debug)]
struct CallError {
    kind: CallErrorKind,
    message: &'static str,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl CallError {
    fn user(message: &'static str) -> Self {
        Self {
            kind: CallErrorKind::User,
            message,
            source: None,
        }
    }

    fn user_with_source<E>(message: &'static str, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            kind: CallErrorKind::User,
            message,
            source: Some(Box::new(source)),
        }
    }

    fn timeout<E>(source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            kind: CallErrorKind::Timeout,
            message: "request timed out",
            source: Some(Box::new(source)),
        }
    }

    fn io<E>(source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            kind: CallErrorKind::Io,
            message: "an i/o error occurred",
            source: Some(Box::new(source)),
        }
    }

    fn other<E>(message: &'static str, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            kind: CallErrorKind::Other,
            message,
            source: Some(Box::new(source)),
        }
    }
}

impl std::error::Error for CallError {}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(err) = self.source.as_ref() {
            write!(f, ": {err}")?;
        }
        Ok(())
    }
}

impl From<CallError> for ConnectorError {
    fn from(value: CallError) -> Self {
        match &value.kind {
            CallErrorKind::User => Self::user(Box::new(value)),
            CallErrorKind::Timeout => Self::timeout(Box::new(value)),
            CallErrorKind::Io => Self::io(Box::new(value)),
            CallErrorKind::Other => Self::other(Box::new(value), None),
        }
    }
}

impl From<reqwest::Error> for CallError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            CallError::timeout(err)
        } else if err.is_connect() || is_transient_transport_error(&err) {
            CallError::io(err)
        } else {
            CallError::other("an unknown error occurred", err)
        }
    }
}

/// Returns whether the failure means the transport died before any response headers were read,
/// which is the classification smithy's `TransientErrorClassifier` retries.
///
/// This is ambiguous with respect to the server: the request may have been fully processed before
/// the connection died, so a retry can duplicate a non-idempotent operation. That is accepted
/// because it is the only way to recover the far more common case where the peer tore down an idle
/// pooled connection before reading the request at all.
///
/// [`reqwest::Error::is_connect`] only reports failures to *establish* a connection, so it misses
/// the pooled-connection case entirely. Those failures surface deeper in the source chain as either
/// an [`std::io::Error`] whose kind means the transport is gone, or a [`hyper::Error`] reporting an
/// incomplete message (a clean FIN mid-request, which carries no [`std::io::Error`] at all).
///
/// The i/o kinds matched are the kinds that reliably mean transport death.
/// [`std::io::ErrorKind::TimedOut`] is excluded because the [`reqwest::Error::is_timeout`] arm
/// above already covers it, and [`std::io::ErrorKind::ConnectionRefused`] along with the DNS, TLS,
/// and host-unreachable establishment failures are already covered by
/// [`reqwest::Error::is_connect`]. [`std::io::ErrorKind::WriteZero`] is included because both hyper
/// and tokio-rustls construct it directly when a transport write makes no progress, the latter
/// being the likelier producer on the TLS path. The list cannot be exhaustive: on Windows
/// `WSAENETRESET` and `WSAESHUTDOWN` map to `ErrorKind::Uncategorized`, which is unmatchable on
/// stable, so those are missed.
fn is_transient_transport_error(err: &(dyn std::error::Error + 'static)) -> bool {
    // TODO: every connection is HTTP/1.1 today because `request.rs` builds the client with
    // `use_preconfigured_tls` and never sets `alpn_protocols`, so no `h2::Error` is reachable. If
    // ALPN is ever enabled, an equivalent arm for h2 stream/GOAWAY errors will be needed here.
    let mut current = Some(err);
    while let Some(err) = current {
        let transient_io = err.downcast_ref::<std::io::Error>().is_some_and(|err| {
            matches!(
                err.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::UnexpectedEof
                    | std::io::ErrorKind::NotConnected
                    | std::io::ErrorKind::WriteZero
            )
        });
        // Relies on reqwest and this crate resolving to the same `hyper` 1.x entry in the lock
        // file; if they ever diverge this downcast silently stops matching.
        let incomplete_message = err
            .downcast_ref::<hyper::Error>()
            .is_some_and(hyper::Error::is_incomplete_message);
        if transient_io || incomplete_message {
            return true;
        }
        current = err.source();
    }
    false
}

#[derive(Debug, Clone)]
enum CallErrorKind {
    User,
    Timeout,
    Io,
    Other,
}

#[derive(Debug)]
struct ReqwestConnector {
    client: ReqwestClient,
    timeout: Option<Duration>,
}

impl HttpConnector for ReqwestConnector {
    fn call(&self, request: Request) -> HttpConnectorFuture {
        let client = self.client.clone();
        let timeout = self.timeout;

        HttpConnectorFuture::new(async move {
            // Convert the aws_smithy_runtime_api request to a reqwest request.
            // TODO: There surely has to be a better way to convert an aws_smith_runtime_api
            // Request<SdkBody> to a reqwest Request<Body>.
            let mut req_builder = client.request(
                reqwest::Method::from_bytes(request.method().as_bytes())
                    .map_err(|err| CallError::user_with_source("failed to create method name", err))?,
                request.uri().to_owned(),
            );
            // Copy the header, body, and timeout.
            let parts = request.into_parts();
            for (name, value) in parts.headers.iter() {
                let name = name.to_owned();
                let value = value.as_bytes().to_owned();
                req_builder = req_builder.header(name, value);
            }
            let body_bytes = parts
                .body
                .bytes()
                .ok_or(CallError::user("streaming request body is not supported"))?
                .to_owned();
            req_builder = req_builder.body(body_bytes);
            if let Some(timeout) = timeout {
                req_builder = req_builder.timeout(timeout);
            }

            let reqwest_response = req_builder.send().await.map_err(CallError::from)?;

            // Converts from a reqwest Response into an http::Response<SdkBody>.
            let (parts, body) = http::Response::from(reqwest_response).into_parts();
            let http_response = http::Response::from_parts(parts, SdkBody::from_body_1_x(body));

            Ok(aws_smithy_runtime_api::http::Response::try_from(http_response)
                .map_err(|err| CallError::other("failed to convert to a proper response", err))?)
        })
    }
}

impl HttpClient for Client {
    fn http_connector(&self, settings: &HttpConnectorSettings, _components: &RuntimeComponents) -> SharedHttpConnector {
        let connector = ReqwestConnector {
            client: self.inner.clone(),
            timeout: settings.read_timeout(),
        };
        SharedHttpConnector::new(connector)
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };
    use tokio::net::{
        TcpListener,
        TcpStream,
    };
    use tokio::sync::oneshot;
    use tokio::time::timeout;

    use super::*;

    /// Upper bound on any single client or server step, so a hang fails fast instead of wedging CI.
    /// Where this bounds a server join it bounds the *join*, not the task: dropping a `JoinHandle`
    /// does not abort it, so anything still running is reaped when the per-test runtime drops.
    const STEP_TIMEOUT: Duration = Duration::from_secs(10);

    /// The client under test: no proxy so a CI proxy env var cannot interfere, HTTP/1.1 only so the
    /// hand-rolled server below is a valid peer.
    fn test_client() -> ReqwestClient {
        ReqwestClient::builder()
            .no_proxy()
            .http1_only()
            .build()
            .expect("failed to build test client")
    }

    /// Reads a single HTTP/1.1 request head off `stream`. The requests issued below are bodyless,
    /// so stopping at the terminating empty line consumes the whole request.
    ///
    /// The byte-at-a-time read is load-bearing: it consumes exactly through `\r\n\r\n` and leaves
    /// nothing in the server's receive buffer. Unread bytes at close make Linux and Windows emit an
    /// RST instead of a FIN, which would give the clean-close test the reset test's shape and break
    /// its `chain_contains_io_error` assertion.
    async fn read_request_head(stream: &mut TcpStream) -> io::Result<()> {
        let mut head = Vec::new();
        let mut byte = [0_u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).await? == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "connection closed"));
            }
            head.push(byte[0]);
        }
        Ok(())
    }

    /// Serves one keep-alive response so the connection lands back in reqwest's pool, then reads
    /// the next request off the *same* socket and hands it back.
    ///
    /// Reading request two off that one socket is what proves reuse, deterministically and by
    /// construction: exactly one socket is ever accepted, so a client that dialed fresh instead
    /// would leave this read hanging and fail the caller's step timeout. Do not add a "the listener
    /// saw no second connection" probe on top of this -- hyper-util races a background TCP connect
    /// against the pooled checkout and completes it even when the pool wins, so a stray socket can
    /// legally land on the backlog and such a probe flakes in both directions.
    async fn serve_then_capture_pooled_request(listener: &TcpListener) -> TcpStream {
        let (mut conn, _) = listener.accept().await.expect("accept failed");
        read_request_head(&mut conn)
            .await
            .expect("failed to read first request");
        conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .await
            .expect("failed to write first response");
        read_request_head(&mut conn)
            .await
            .expect("failed to read pooled request");
        conn
    }

    /// Issues the first request and drains its body, which is what returns the connection to the
    /// pool, then returns the error from the second (pooled) request.
    async fn first_ok_then_pooled_error(url: &str) -> reqwest::Error {
        let client = test_client();
        let response = timeout(STEP_TIMEOUT, client.get(url).send())
            .await
            .expect("first request timed out")
            .expect("first request failed");
        let body = timeout(STEP_TIMEOUT, response.bytes())
            .await
            .expect("first body timed out")
            .expect("first body failed");
        assert_eq!(body.as_ref(), b"ok");

        timeout(STEP_TIMEOUT, client.get(url).send())
            .await
            .expect("second request timed out")
            .expect_err("second request should have failed")
    }

    /// Whether any level of the source chain is an [`std::io::Error`]. Used only to prove that the
    /// reset and clean-close tests really do exercise different branches of the classifier: a reset
    /// carries an [`std::io::Error`], a clean close does not carry one at all. The specific kind is
    /// deliberately not asserted, since that varies by platform.
    fn chain_contains_io_error(err: &(dyn std::error::Error + 'static)) -> bool {
        std::iter::successors(Some(err), |err| err.source()).any(|err| err.is::<std::io::Error>())
    }

    /// A hard RST while a request is in flight on a pooled connection must classify as an i/o
    /// error, which is the classification smithy retries. `is_connect()` is false here, which
    /// is exactly why the source chain has to be inspected.
    #[tokio::test]
    async fn pooled_connection_reset_is_retryable() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind failed");
        let addr = listener.local_addr().expect("local_addr failed");

        let server = tokio::spawn(async move {
            let conn = serve_then_capture_pooled_request(&listener).await;
            // Zero linger turns the close into an RST rather than a FIN. `set_linger` is deprecated
            // because it can block the thread on drop, which is irrelevant here: a zero linger closes
            // immediately, and it is the only portable way to force a reset.
            #[allow(deprecated)]
            conn.set_linger(Some(Duration::ZERO)).expect("set_linger failed");
            drop(conn);
        });

        let err = first_ok_then_pooled_error(&format!("http://{addr}/")).await;
        assert!(!err.is_connect(), "expected a request-phase error, got {err:?}");
        assert!(
            chain_contains_io_error(&err),
            "expected a reset to surface an io error, got {err:?}"
        );
        let connector_error: ConnectorError = CallError::from(err).into();
        assert!(
            connector_error.is_io(),
            "a reset pooled connection must be retryable, got {connector_error:?}"
        );

        timeout(STEP_TIMEOUT, server)
            .await
            .expect("server timed out")
            .expect("server panicked");
    }

    /// A clean FIN while a request is in flight on a pooled connection surfaces as
    /// `hyper::Error(IncompleteMessage)` with no `io::Error` anywhere in the chain, so this covers
    /// the hyper branch of the classifier specifically.
    #[tokio::test]
    async fn pooled_connection_close_is_retryable() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind failed");
        let addr = listener.local_addr().expect("local_addr failed");

        let server = tokio::spawn(async move {
            drop(serve_then_capture_pooled_request(&listener).await);
        });

        let err = first_ok_then_pooled_error(&format!("http://{addr}/")).await;
        assert!(!err.is_connect(), "expected a request-phase error, got {err:?}");
        assert!(
            !chain_contains_io_error(&err),
            "a clean close should carry no io error, so this test must cover the hyper branch: {err:?}"
        );
        let connector_error: ConnectorError = CallError::from(err).into();
        assert!(
            connector_error.is_io(),
            "a closed pooled connection must be retryable, got {connector_error:?}"
        );

        timeout(STEP_TIMEOUT, server)
            .await
            .expect("server timed out")
            .expect("server panicked");
    }

    /// Negative control: a malformed response is a request-phase error too, but the transport is
    /// healthy, so it must not be classified as retryable i/o. Without this the suite could not
    /// tell a correct classifier from one that always returns true.
    #[tokio::test]
    async fn malformed_response_is_not_retryable() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind failed");
        let addr = listener.local_addr().expect("local_addr failed");
        let (release_tx, release_rx) = oneshot::channel::<()>();

        let server = tokio::spawn(async move {
            let (mut conn, _) = listener.accept().await.expect("accept failed");
            read_request_head(&mut conn).await.expect("failed to read request");
            conn.write_all(b"NOT-HTTP garbage\r\n\r\n")
                .await
                .expect("failed to write response");
            // Hold the socket open until the client is done, so the failure is purely a parse error
            // with no transport close anywhere in the source chain.
            let _ = release_rx.await;
        });

        let err = timeout(STEP_TIMEOUT, test_client().get(format!("http://{addr}/")).send())
            .await
            .expect("request timed out")
            .expect_err("request should have failed");
        assert!(
            !chain_contains_io_error(&err),
            "a healthy transport must carry no io error, got {err:?}"
        );
        let connector_error: ConnectorError = CallError::from(err).into();
        assert!(
            !connector_error.is_io(),
            "a malformed response is not a transport failure, got {connector_error:?}"
        );
        assert!(
            !connector_error.is_timeout(),
            "a malformed response is not a timeout, got {connector_error:?}"
        );

        let _ = release_tx.send(());
        timeout(STEP_TIMEOUT, server)
            .await
            .expect("server timed out")
            .expect("server panicked");
    }

    /// Wraps an arbitrary error so the classifier has to walk `source()` to find it, rather than
    /// matching at depth zero the way a bare `io::Error` would.
    #[derive(Debug)]
    struct Wrapped(Box<dyn std::error::Error + Send + Sync>);

    impl std::fmt::Display for Wrapped {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "wrapped: {}", self.0)
        }
    }

    impl std::error::Error for Wrapped {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(self.0.as_ref())
        }
    }

    /// Pins the exact set of i/o kinds treated as transport death. The socket tests above only
    /// reach `ConnectionReset`, so without this both dropping a kind from the match and adding a
    /// bogus one would go unnoticed.
    #[test]
    fn transient_transport_error_matches_only_transport_death() {
        let cases = [
            (io::ErrorKind::ConnectionReset, true),
            (io::ErrorKind::ConnectionAborted, true),
            (io::ErrorKind::BrokenPipe, true),
            (io::ErrorKind::UnexpectedEof, true),
            (io::ErrorKind::NotConnected, true),
            (io::ErrorKind::WriteZero, true),
            // Claimed by the `is_timeout()` and `is_connect()` arms of `From<reqwest::Error>`
            // instead, so this helper must leave them alone.
            (io::ErrorKind::TimedOut, false),
            (io::ErrorKind::ConnectionRefused, false),
            (io::ErrorKind::PermissionDenied, false),
        ];
        for (kind, expected) in cases {
            let err = Wrapped(Box::new(io::Error::from(kind)));
            assert_eq!(
                is_transient_transport_error(&err),
                expected,
                "wrong classification for {kind:?}"
            );
        }

        let err = Wrapped(Box::new(std::fmt::Error));
        assert!(
            !is_transient_transport_error(&err),
            "a chain with no io error must not be transient"
        );
    }
}
