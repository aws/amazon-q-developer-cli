#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use tokio::sync::broadcast;

    /// This test demonstrates that blocking operations prevent tokio::select! from processing
    /// Ctrl+C signals. This simulates the issue where rustyline::readline() blocks the runtime.
    #[tokio::test]
    async fn test_blocking_prevents_ctrlc_handling() {
        let (ctrlc_tx, mut ctrlc_rx) = broadcast::channel::<()>(1);
        let hang_detected = Arc::new(AtomicBool::new(false));
        let hang_detected_clone = hang_detected.clone();

        // Spawn a task that simulates the blocking readline call
        let blocking_task = tokio::spawn(async move {
            tokio::select! {
                // This simulates a blocking operation like rustyline::readline()
                _ = tokio::task::spawn_blocking(|| {
                    // Block for 2 seconds to simulate readline waiting for input
                    std::thread::sleep(Duration::from_secs(2));
                }) => {
                    // If we get here, the blocking operation completed
                },
                // This should fire when Ctrl+C is pressed
                Ok(_) = ctrlc_rx.recv() => {
                    // If we get here, Ctrl+C was processed successfully
                    hang_detected_clone.store(true, Ordering::SeqCst);
                },
            }
        });

        // Wait a bit for the task to start blocking
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Simulate Ctrl+C being pressed
        let _ = ctrlc_tx.send(());

        // Wait a bit to see if the signal was processed
        tokio::time::sleep(Duration::from_millis(200)).await;

        // The hang_detected flag should be true if Ctrl+C was handled
        // With spawn_blocking, this should work correctly
        assert!(
            hang_detected.load(Ordering::SeqCst),
            "Ctrl+C should be processed even during blocking operations when using spawn_blocking"
        );

        // Clean up
        blocking_task.abort();
    }

    /// This test simulates the run_hooks scenario where async operations
    /// complete without Ctrl+C handling - demonstrates the bug
    #[tokio::test(flavor = "multi_thread")]
    async fn test_hooks_without_ctrlc_handling() {
        let (ctrlc_tx, _ctrlc_rx) = broadcast::channel::<()>(1);
        let hook_completed = Arc::new(AtomicBool::new(false));
        let hook_completed_clone = hook_completed.clone();
        
        // Simulate hook execution that takes time but has NO Ctrl+C handling
        let hook_task = tokio::spawn(async move {
            // Simulate a hook that takes 1 second
            tokio::time::sleep(Duration::from_secs(1)).await;
            hook_completed_clone.store(true, Ordering::SeqCst);
        });
        
        // Simulate Ctrl+C being pressed immediately
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = ctrlc_tx.send(());
        
        // Wait a bit to see if hook responds to Ctrl+C (it won't)
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Hook should still be running because it doesn't check for Ctrl+C
        assert!(
            !hook_completed.load(Ordering::SeqCst),
            "Hook should still be running - doesn't respond to Ctrl+C"
        );
        
        // Wait for hook to actually complete
        let _ = hook_task.await;
        
        // Now it should be done
        assert!(
            hook_completed.load(Ordering::SeqCst),
            "Hook eventually completes but ignored Ctrl+C - this is the bug"
        );
    }

    /// This test shows the CORRECT pattern - hooks WITH Ctrl+C handling
    #[tokio::test(flavor = "multi_thread")]
    async fn test_hooks_with_ctrlc_handling() {
        let (ctrlc_tx, mut ctrlc_rx) = broadcast::channel::<()>(1);
        
        // Simulate hook execution WITH Ctrl+C handling
        let hook_task = tokio::spawn(async move {
            let mut futures = vec![];
            for _ in 0..3 {
                futures.push(tokio::spawn(async {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    "hook result"
                }));
            }
            
            // Wait for hooks WITH tokio::select! for Ctrl+C
            tokio::select! {
                _ = async {
                    for fut in futures {
                        let _ = fut.await;
                    }
                } => {},
                Ok(_) = ctrlc_rx.recv() => {
                    // Ctrl+C received, cancel hooks
                    return Err("Cancelled by Ctrl+C");
                }
            }
            Ok("Completed")
        });
        
        // Simulate Ctrl+C being pressed while hooks are running
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = ctrlc_tx.send(());
        
        // Hook task should complete quickly because it handles Ctrl+C
        let result = tokio::time::timeout(Duration::from_millis(500), hook_task).await;
        
        assert!(
            result.is_ok(),
            "Hook execution should complete quickly when Ctrl+C is handled"
        );
        assert!(
            result.unwrap().unwrap().is_err(),
            "Hook should return error indicating cancellation"
        );
    }
}
