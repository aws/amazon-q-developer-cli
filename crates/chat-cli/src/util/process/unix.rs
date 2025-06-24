use nix::sys::signal::Signal;
use sysinfo::Pid;

pub fn terminate_process(pid: Pid) -> Result<(), String> {
    let nix_pid = nix::unistd::Pid::from_raw(pid.as_u32() as i32);
    nix::sys::signal::kill(nix_pid, Signal::SIGTERM).map_err(|e| format!("Failed to terminate process: {}", e))
}

#[cfg(test)]
#[cfg(not(windows))]
mod tests {
    use std::process::Command;
    use std::time::Duration;

    use super::*;

    // Helper to create a long-running process for testing
    fn spawn_test_process() -> std::process::Child {
        let mut command = Command::new("sleep");
        command.arg("30");
        command.spawn().expect("Failed to spawn test process")
    }

    #[test]
    fn test_terminate_process() {
        // Spawn a test process
        let mut child = spawn_test_process();
        let pid = Pid::from_u32(child.id());

        // Terminate the process
        let result = terminate_process(pid);

        // Verify termination was successful
        assert!(result.is_ok(), "Process termination failed: {:?}", result.err());

        // Give it a moment to terminate
        std::thread::sleep(Duration::from_millis(100));

        // Verify the process is actually terminated
        match child.try_wait() {
            Ok(Some(_)) => {
                // Process exited, which is what we expect
            },
            Ok(None) => {
                panic!("Process is still running after termination");
            },
            Err(e) => {
                panic!("Error checking process status: {}", e);
            },
        }
    }

    #[test]
    fn test_terminate_nonexistent_process() {
        // Use a likely invalid PID
        let invalid_pid = Pid::from_u32(u32::MAX - 1);

        // Attempt to terminate a non-existent process
        let result = terminate_process(invalid_pid);

        // Should return an error
        assert!(result.is_err(), "Terminating non-existent process should fail");
    }
}
