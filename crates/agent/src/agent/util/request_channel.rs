use eyre::Result;
use tokio::sync::{
    mpsc,
    oneshot,
};
use tracing::{
    error,
    trace,
    warn,
};

/// A request to a specific task
#[derive(Debug)]
pub struct Request<Req, Res, Err> {
    /// Request payload
    pub payload: Req,
    /// Response channel
    pub res_tx: oneshot::Sender<Result<Res, Err>>,
}

impl<Req, Res, Err> Request<Req, Res, Err>
where
    Req: std::fmt::Debug + Send + Sync + 'static,
    Res: std::fmt::Debug + Send + Sync + 'static,
    Err: std::fmt::Debug + std::error::Error + Send + Sync + 'static,
{
    pub async fn respond(self, response: Result<Res, Err>) {
        self.res_tx
            .send(response)
            .map_err(|err| tracing::error!(?err, "failed to send response"))
            .ok();
    }
}

/// Helper macro for responding to a request that has partially moved data (eg, the payload)
macro_rules! respond {
    ($res_tx:expr, $res:expr) => {
        $res_tx
            .res_tx
            .send($res)
            .map_err(|err| tracing::error!(?err, "failed to send response"))
            .ok();
    };
}

pub(crate) use respond;

#[derive(Debug)]
pub struct RequestSender<Req, Res, Err> {
    tx: mpsc::Sender<Request<Req, Res, Err>>,
}

impl<Req, Res, Err> Clone for RequestSender<Req, Res, Err> {
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone() }
    }
}

impl<Req, Res, Err> RequestSender<Req, Res, Err>
where
    Req: std::fmt::Debug + Send + Sync + 'static,
    Res: std::fmt::Debug + Send + Sync + 'static,
    Err: std::fmt::Debug + std::error::Error + Send + Sync + 'static,
{
    pub fn new(tx: mpsc::Sender<Request<Req, Res, Err>>) -> Self {
        Self { tx }
    }

    /// Returns [None] if one of the channels for sending and receiving messages fails. This
    /// should only happen if one end of the channels closes for whatever reason.
    pub async fn send_recv(&self, payload: Req) -> Option<Result<Res, Err>> {
        trace!(?payload, "sending payload");
        let (res_tx, res_rx) = oneshot::channel();
        let request = Request { payload, res_tx };

        // Errors if the request receiver has closed
        if (self.tx.send(request).await).is_err() {
            warn!("request receiver has closed");
            return None;
        }

        // Errors if the response tx is dropped before sending a result, indicates a bug with the
        // responder.
        match res_rx.await {
            Ok(res) => Some(res),
            Err(_) => {
                error!("response tx dropped before sending a result");
                None
            },
        }
    }

    pub fn try_blocking_send_recv(&self, payload: Req) -> Option<Result<Res, Err>> {
        trace!(?payload, "sending payload");
        let (res_tx, mut res_rx) = oneshot::channel();
        let request = Request { payload, res_tx };

        // Errors if the channel is full or the channel has closed
        if (self.tx.try_send(request)).is_err() {
            warn!("request receiver has closed");
            return None;
        }

        // Errors if the response tx is dropped before sending a result, indicates a bug with the
        // responder.
        match res_rx.try_recv() {
            Ok(res) => Some(res),
            Err(_) => {
                error!("response tx dropped before sending a result");
                None
            },
        }
    }

    /// Fire-and-forget: enqueue the request without waiting for a response.
    /// The oneshot receiver is intentionally dropped — the responder's send
    /// will fail silently, which is acceptable for termination signals where
    /// the caller doesn't need acknowledgment.
    pub fn try_send_no_recv(&self, payload: Req) {
        trace!(?payload, "fire-and-forget send");
        let (res_tx, _res_rx) = oneshot::channel();
        let request = Request { payload, res_tx };
        if self.tx.try_send(request).is_err() {
            warn!("request receiver has closed (fire-and-forget)");
        }
    }

    pub fn count(&self) -> usize {
        self.tx.strong_count()
    }
}

pub type RequestReceiver<Req, Res, Err> = mpsc::Receiver<Request<Req, Res, Err>>;

pub fn new_request_channel<Req, Res, Err>() -> (RequestSender<Req, Res, Err>, RequestReceiver<Req, Res, Err>)
where
    Req: std::fmt::Debug + Send + Sync + 'static,
    Res: std::fmt::Debug + Send + Sync + 'static,
    Err: std::fmt::Debug + std::error::Error + Send + Sync + 'static,
{
    let (tx, rx) = mpsc::channel(16);
    (RequestSender::new(tx), rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    #[error("test error: {0}")]
    struct TestErr(String);

    #[tokio::test]
    async fn test_new_request_channel() {
        let (tx, _rx): (RequestSender<u32, u32, TestErr>, _) = new_request_channel();
        assert_eq!(tx.count(), 1);
    }

    #[tokio::test]
    async fn test_send_recv_success() {
        let (tx, mut rx): (RequestSender<i32, i32, TestErr>, _) = new_request_channel();
        let handle = tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                let payload = req.payload;
                req.respond(Ok(payload * 2)).await;
            }
        });
        let result = tx.send_recv(21).await;
        assert!(matches!(result, Some(Ok(42))));
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_send_recv_error() {
        let (tx, mut rx): (RequestSender<(), i32, TestErr>, _) = new_request_channel();
        let handle = tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                req.respond(Err(TestErr("nope".into()))).await;
            }
        });
        let result = tx.send_recv(()).await;
        assert!(matches!(result, Some(Err(_))));
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_send_recv_receiver_closed() {
        let (tx, rx): (RequestSender<i32, i32, TestErr>, _) = new_request_channel();
        drop(rx);
        let result = tx.send_recv(1).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_send_recv_response_dropped() {
        let (tx, mut rx): (RequestSender<i32, i32, TestErr>, _) = new_request_channel();
        let handle = tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                drop(req); // drop without responding
            }
        });
        let result = tx.send_recv(1).await;
        assert!(result.is_none());
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_clone() {
        let (tx, _rx): (RequestSender<i32, i32, TestErr>, _) = new_request_channel();
        let _tx2 = tx.clone();
        assert_eq!(tx.count(), 2);
    }

    #[test]
    fn test_try_blocking_send_recv_closed() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (tx, rx): (RequestSender<i32, i32, TestErr>, _) = new_request_channel();
            drop(rx);
            let result = tx.try_blocking_send_recv(1);
            assert!(result.is_none());
        });
    }
}
