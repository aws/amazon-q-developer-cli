use std::sync::Arc;
use std::collections::VecDeque;
use tokio::sync::{Mutex, Notify};

use crate::agent_env::Worker;

pub struct PromptRequest {
    pub worker: Arc<Worker>,
}

pub struct PromptQueue {
    queue: Mutex<VecDeque<PromptRequest>>,
    notify_enqueue: Notify,
}

impl PromptQueue {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            notify_enqueue: Notify::new(),
        }
    }
    
    /// Add prompt request to queue
    pub async fn enqueue(&self, worker: Arc<Worker>) {
        tracing::debug!("Enqueuing prompt request for worker {}", worker.id);
        let request = PromptRequest { worker };
        self.queue.lock().await.push_back(request);
        self.notify_enqueue.notify_one();
    }
    
    /// Get next prompt request (FIFO)
    pub async fn dequeue(&self) -> Option<PromptRequest> {
        let result = self.queue.lock().await.pop_front();
        if let Some(ref req) = result {
            tracing::debug!("Dequeued prompt request for worker {}", req.worker.id);
        }
        result
    }
    
    /// Wait until items are available in the queue
    pub async fn wait_for_items(&self) {
        loop {
            if !self.is_empty().await {
                return;
            }
            self.notify_enqueue.notified().await;
        }
    }
    
    /// Check if queue is empty
    pub async fn is_empty(&self) -> bool {
        self.queue.lock().await.is_empty()
    }
    
    /// Get current queue length
    pub async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }
}
