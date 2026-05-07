//! Background initialization helper for async operations that should not block startup

use std::sync::{
    Arc,
    Mutex,
};

use anyhow::Result;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// Helper for running expensive initialization in the background
pub struct BackgroundInit<T> {
    result: Arc<Mutex<Option<Result<T>>>>,
    notify: Arc<Notify>,
    _task: Option<JoinHandle<()>>,
}

impl<T: Send + 'static> BackgroundInit<T> {
    /// Start background initialization with CPU-bound work
    pub fn start_blocking<F>(init_fn: F) -> Self
    where
        F: FnOnce() -> Result<T> + Send + 'static,
    {
        let result = Arc::new(Mutex::new(None));
        let notify = Arc::new(Notify::new());

        let result_clone = result.clone();
        let notify_clone = notify.clone();

        let task = tokio::task::spawn_blocking(move || {
            let value = std::panic::catch_unwind(std::panic::AssertUnwindSafe(init_fn))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Background initialization panicked")));
            *result_clone.lock().unwrap() = Some(value);
            notify_clone.notify_waiters();
        });

        Self {
            result,
            notify,
            _task: Some(task),
        }
    }

    /// Wait for initialization to complete
    pub async fn wait(&self) -> Result<()> {
        loop {
            // Register for notification BEFORE checking result to avoid race
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            // Check if already complete
            {
                let guard = self.result.lock().unwrap();
                if let Some(ref r) = *guard {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(e) => Err(anyhow::anyhow!("Background initialization failed: {}", e)),
                    };
                }
            }

            // Wait for notification
            notified.await;
        }
    }

    /// Check if initialization is complete (non-blocking)
    pub fn is_ready(&self) -> bool {
        self.result.lock().unwrap().is_some()
    }
}

impl<T> Drop for BackgroundInit<T> {
    fn drop(&mut self) {
        if let Some(task) = self._task.take() {
            task.abort();
        }
    }
}
