use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;

use bytes::Bytes;
pub use http::Method;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::service::Service;
use hyper::{
    Request,
    Response,
};
use hyper_util::rt::TokioIo;
use tokio::net::{
    TcpListener,
    TcpStream,
};
use tokio::select;
use tokio_util::sync::CancellationToken;

/// A local HTTP server that can be used for returning mock responses to HTTP requests.
///
/// # Examples
///
/// ```rust
/// # use fig_test_utils::server::*;
/// # async fn run() -> Result<(), reqwest::Error> {
/// // Hosting a local server that responds to GET requests for "/my-file" with the
/// // body "some text".
/// let test_path = String::from("/my-file");
/// let mock_body = String::from("some text");
/// let test_server_addr = TestServer::new()
///     .await
///     .with_mock_response(Method::GET, test_path.clone(), mock_body.clone())
///     .spawn_listener();
///
/// let body = reqwest::get(format!("http://{}{}", &test_server_addr, &test_path))
///     .await?
///     .text()
///     .await?;
///
/// assert_eq!(body, mock_body);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct TestServer {
    listener: TcpListener,
    mock_responses: HashMap<(http::Method, String), String>,
}

impl TestServer {
    pub async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind socket to local host");
        Self {
            listener,
            mock_responses: HashMap::default(),
        }
    }

    pub fn with_mock_response(mut self, method: http::Method, path: String, response: String) -> Self {
        self.mock_responses.insert((method, path), response);
        self
    }

    /// Spawns a new task for accepting requests, returning the address of the listening socket.
    pub fn spawn_listener(self) -> TestAddress {
        let address = self
            .listener
            .local_addr()
            .expect("listener should be bound to an address");
        let cancellation_token = CancellationToken::new();
        let token_clone = cancellation_token.clone();
        tokio::task::spawn(async move {
            loop {
                select! {
                    stream = self.listener.accept() => {
                        let (stream, _) = stream.expect("failed to accept new connection");
                        self.handle_request(stream).await;
                    },
                    _ = token_clone.cancelled() => {
                        break;
                    }
                }
            }
        });
        TestAddress {
            address,
            cancellation_token,
        }
    }

    async fn handle_request(&self, stream: TcpStream) {
        let stream = TokioIo::new(stream);
        hyper::server::conn::http1::Builder::new()
            .serve_connection(stream, &self)
            .await
            .unwrap();
    }
}

#[derive(Debug)]
pub struct TestAddress {
    address: SocketAddr,
    cancellation_token: CancellationToken,
}

impl TestAddress {
    pub fn address(&self) -> SocketAddr {
        self.address
    }
}

impl std::fmt::Display for TestAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.address)
    }
}

impl Drop for TestAddress {
    fn drop(&mut self) {
        self.cancellation_token.cancel();
    }
}

type ServiceError = Box<dyn std::error::Error + Send + Sync + 'static>;
type ServiceResponse = Response<Full<Bytes>>;
type ServiceFuture = Pin<Box<dyn Future<Output = Result<ServiceResponse, ServiceError>> + Send>>;

impl Service<Request<Incoming>> for TestServer {
    type Error = ServiceError;
    type Future = ServiceFuture;
    type Response = ServiceResponse;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let method = req.method().clone();
        let path = req.uri().path().to_string();
        let result = self.mock_responses.get(&(method, path)).unwrap().clone();
        Box::pin(async move { Ok(Response::builder().status(200).body(result.into()).unwrap()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_mock_and_drop() {
        let test_path = String::from("/test-path");
        let test_response = String::from("test body");
        let test_server_addr = TestServer::new()
            .await
            .with_mock_response(Method::GET, test_path.clone(), test_response.clone())
            .spawn_listener();

        let response = reqwest::get(format!("http://{}{}", &test_server_addr, &test_path))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(response, test_response);

        // Test that dropping TestAddress stops the server
        let addr = test_server_addr.address();
        std::mem::drop(test_server_addr);
        // wait for the task to complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let response = reqwest::get(format!("http://{}{}", &addr, &test_path)).await;
        assert!(response.is_err());
    }
}
