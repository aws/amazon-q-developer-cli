/// Async stdin reader that doesn't block tokio runtime shutdown.
///
/// `tokio::io::stdin()` uses a blocking read on a separate thread that is
/// impossible to cancel, which can hang runtime shutdown
/// (see <https://docs.rs/tokio/latest/tokio/io/struct.Stdin.html>
/// and <https://github.com/tokio-rs/tokio/issues/2466>).
///
/// Following tokio's own recommendation, this module spawns a dedicated OS
/// thread for blocking stdin reads and forwards bytes through a channel.
/// The async side implements `futures::AsyncRead` over the channel receiver.
/// Because the reader thread lives outside the tokio runtime, it cannot
/// prevent runtime shutdown.
use std::io::Read;
use std::pin::Pin;
use std::task::{
    Context,
    Poll,
};

use futures::AsyncRead;
use tokio::sync::mpsc;

const BUF_SIZE: usize = 8192;

pub struct StdinReader {
    rx: mpsc::Receiver<Vec<u8>>,
    /// Leftover bytes from the last channel recv that haven't been consumed yet.
    buf: Vec<u8>,
    pos: usize,
    /// Fires when all buffered data has been drained and EOF delivered to the consumer.
    eof_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl StdinReader {
    /// Spawn a stdin reader thread. Returns the async reader and a oneshot that
    /// fires after all buffered data is drained and EOF is delivered.
    pub fn new() -> (Self, tokio::sync::oneshot::Receiver<()>) {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(2);
        let (eof_tx, eof_rx) = tokio::sync::oneshot::channel();

        std::thread::Builder::new()
            .name("stdin-reader".into())
            .spawn(move || {
                let stdin = std::io::stdin();
                let mut buf = [0u8; BUF_SIZE];
                loop {
                    match stdin.lock().read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tx.blocking_send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                        },
                    }
                }
                // tx dropped — rx yields None after buffered chunks drain.
            })
            .expect("failed to spawn stdin reader thread");

        (
            Self {
                rx,
                buf: Vec::new(),
                pos: 0,
                eof_tx: Some(eof_tx),
            },
            eof_rx,
        )
    }
}

impl AsyncRead for StdinReader {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<std::io::Result<usize>> {
        // Drain leftover bytes from previous recv first.
        if self.pos < self.buf.len() {
            let remaining = &self.buf[self.pos..];
            let n = remaining.len().min(buf.len());
            buf[..n].copy_from_slice(&remaining[..n]);
            self.pos += n;
            return Poll::Ready(Ok(n));
        }

        // Poll the channel for the next chunk.
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(data)) => {
                let n = data.len().min(buf.len());
                buf[..n].copy_from_slice(&data[..n]);
                if n < data.len() {
                    self.buf = data;
                    self.pos = n;
                } else {
                    self.buf.clear();
                    self.pos = 0;
                }
                Poll::Ready(Ok(n))
            },
            Poll::Ready(None) => {
                // Channel drained — notify that EOF has been delivered.
                if let Some(tx) = self.eof_tx.take() {
                    let _ = tx.send(());
                }
                Poll::Ready(Ok(0))
            },
            Poll::Pending => Poll::Pending,
        }
    }
}
