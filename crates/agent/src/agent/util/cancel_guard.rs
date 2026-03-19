use std::sync::Arc;
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};

/// Drop guard that sets a cancellation flag when a future is dropped by `select!`.
///
/// When a tool's future is cancelled (e.g. by Ctrl+C via `tokio::select!`), code after
/// the `.await` point never runs. This guard ensures the `AtomicBool` flag is set on
/// drop, signalling any `spawn_blocking` thread to stop promptly.
pub struct CancelGuard(pub Arc<AtomicBool>);

impl Drop for CancelGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
