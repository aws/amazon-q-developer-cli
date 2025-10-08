use eyre::Result;
use tokio::sync::{
    mpsc,
    oneshot,
};
use tracing::{
    error,
    trace,
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
            error!("request receiver has closed");
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

