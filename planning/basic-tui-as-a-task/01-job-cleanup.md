# Job Cleanup Design

## Problem
As the TUI runs multiple jobs sequentially, the job list in Session will grow unbounded. We need to keep only recent inactive jobs for debugging/history while removing old ones.

## Requirements
- Keep maximum 3 inactive jobs (completed, cancelled, or failed)
- Active jobs are never removed
- Cleanup happens when spawning new jobs
- Oldest inactive jobs are removed first

## Constants

```rust
// In crates/chat-cli/src/agent_env/session.rs or config module
pub const MAX_INACTIVE_JOBS: usize = 3;
```

## Session Changes

### Current State
```rust
pub struct Session {
    model_providers: Vec<BedrockConverseStreamModelProvider>,
    workers: Arc<Mutex<Vec<Arc<Worker>>>>,
    jobs: Arc<Mutex<Vec<Arc<WorkerJob>>>>,  // All jobs, no cleanup
}
```

### Enhanced State
```rust
pub struct Session {
    model_providers: Vec<BedrockConverseStreamModelProvider>,
    workers: Arc<Mutex<Vec<Arc<Worker>>>>,
    jobs: Arc<Mutex<Vec<Arc<WorkerJob>>>>,
}

impl Session {
    // NEW: Cleanup old inactive jobs
    pub fn cleanup_inactive_jobs(&self) {
        let mut jobs = self.jobs.lock().unwrap();
        
        // Separate active and inactive jobs
        let (active, mut inactive): (Vec<_>, Vec<_>) = jobs
            .iter()
            .cloned()
            .partition(|job| job.is_active());
        
        // Keep only last MAX_INACTIVE_JOBS inactive jobs
        if inactive.len() > MAX_INACTIVE_JOBS {
            let keep_from = inactive.len() - MAX_INACTIVE_JOBS;
            inactive.drain(0..keep_from);
        }
        
        // Rebuild jobs list: active + recent inactive
        *jobs = active;
        jobs.extend(inactive);
    }
    
    // NEW: Query job state
    pub fn get_job_counts(&self) -> (usize, usize) {
        let jobs = self.jobs.lock().unwrap();
        let active = jobs.iter().filter(|j| j.is_active()).count();
        let inactive = jobs.len() - active;
        (active, inactive)
    }
}
```

## WorkerJob Changes

### Add State Query
```rust
impl WorkerJob {
    // NEW: Check if job is still active
    pub fn is_active(&self) -> bool {
        // Job is active if:
        // 1. Task handle exists and is not finished
        // 2. Cancellation token is not cancelled
        
        if self.cancellation_token.is_cancelled() {
            return false;
        }
DM: while cancellation_token is triggered, the job can take some time to actually gracefully stop
        
        match &self.task_handle {
            Some(handle) => !handle.is_finished(),
            None => false,
        }
    }
    
    // NEW: Get job completion state
    pub fn get_state(&self) -> JobState {
        if self.is_active() {
            JobState::Active
        } else if self.cancellation_token.is_cancelled() {
            JobState::Cancelled
        } else {
            // Check worker state for completed/failed
            match self.worker.get_state() {
                WorkerStates::InactiveFailed => JobState::Failed,
                _ => JobState::Completed,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Active,
    Completed,
    Cancelled,
    Failed,
}
```

## Cleanup Trigger Points

### 1. Before Spawning New Job
```rust
// In AgentEnvUi main loop
loop {
    let user_input = input_handler.read_prompt().await?;
    
    // Cleanup before spawning new job
    session.cleanup_inactive_jobs();
    
    let job = session.run_agent_loop(worker, input, ui_interface)?;
    job.wait().await?;
}
```

### 2. Optional: Periodic Cleanup
```rust
// Could add periodic cleanup in background, but not needed for basic TUI
// The "before spawn" cleanup is sufficient
```

## Testing Considerations

### Test Scenarios
1. **No cleanup needed**: Spawn 3 jobs, verify all kept
2. **Cleanup triggered**: Spawn 5 jobs, verify only last 3 inactive kept
3. **Active jobs preserved**: Have 1 active + 4 inactive, verify active + 3 inactive kept
4. **Multiple active jobs**: Have 2 active + 4 inactive, verify 2 active + 3 inactive kept

### Test Implementation
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_cleanup_keeps_max_inactive() {
        let session = Session::new(vec![/* providers */]);
        
        // Spawn 5 jobs and wait for completion
        for i in 0..5 {
            let job = session.run_agent_loop(/* ... */)?;
            job.wait().await?;
        }
        
        // Cleanup
        session.cleanup_inactive_jobs();
        
        // Verify only 3 inactive jobs remain
        let (active, inactive) = session.get_job_counts();
        assert_eq!(active, 0);
        assert_eq!(inactive, 3);
    }
}
```

## Edge Cases

### 1. All Jobs Active
- Cleanup does nothing
- All jobs preserved

### 2. Job Completes During Cleanup
- Race condition: job transitions from active to inactive
- Solution: Lock is held during entire cleanup operation
- Job will be included in next cleanup cycle

### 3. Zero Inactive Jobs
- Cleanup does nothing
- No errors

### 4. Exactly MAX_INACTIVE_JOBS
- Cleanup does nothing
- All jobs preserved

## Performance Considerations

- Cleanup is O(n) where n = number of jobs
- Lock is held briefly during cleanup
- No async operations during cleanup
- Cleanup happens infrequently (once per user prompt)

## Alternative Approaches Considered

### 1. Time-based Cleanup
- Remove jobs older than X minutes
- **Rejected**: More complex, less predictable

### 2. Memory-based Cleanup
- Remove jobs when memory usage exceeds threshold
- **Rejected**: Overkill for this use case

### 3. Manual Cleanup Command
- User triggers cleanup with `/cleanup` command
- **Rejected**: Should be automatic

## Implementation Order

1. Add `MAX_INACTIVE_JOBS` constant
2. Add `is_active()` and `get_state()` to WorkerJob
3. Add `cleanup_inactive_jobs()` to Session
4. Add `get_job_counts()` for debugging/testing
5. Integrate cleanup call in TUI loop
6. Add tests
