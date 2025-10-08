use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::signal;
use tokio::sync::Notify;

use crate::agent_env::Session;

pub struct CtrlCHandler {
    last_interrupt_time: Arc<AtomicU64>,
    shutdown_signal: Arc<Notify>,
    session: Arc<Session>,
}

impl CtrlCHandler {
    pub fn new(shutdown_signal: Arc<Notify>, session: Arc<Session>) -> Self {
        Self {
            last_interrupt_time: Arc::new(AtomicU64::new(0)),
            shutdown_signal,
            session,
        }
    }
    
    /// Start listening for Ctrl+C signals
    pub fn start_listening(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                match signal::ctrl_c().await {
                    Ok(()) => {
                        self.handle_ctrl_c().await;
                    }
                    Err(e) => {
                        eprintln!("Error setting up Ctrl+C handler: {}", e);
                        break;
                    }
                }
            }
        });
    }
    
    /// Handle Ctrl+C signal
    async fn handle_ctrl_c(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let last_time = self.last_interrupt_time.swap(now, Ordering::SeqCst);
        let time_since_last = now.saturating_sub(last_time);
        
        if time_since_last < 1000 {
            // Double Ctrl+C (within 1 second) - force exit
            println!("\n^C (Force exit)");
            self.shutdown_signal.notify_one();
        } else {
            // First Ctrl+C - cancel all active jobs
            println!("\n^C (Cancelling... Press Ctrl+C again to force exit)");
            self.session.cancel_all_jobs();
        }
    }
}
