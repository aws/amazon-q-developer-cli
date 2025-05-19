use std::ops::Deref;

use sysinfo::Pid;
use windows::Win32::Foundation::{
    CloseHandle,
    HANDLE,
};
use windows::Win32::System::Threading::{
    OpenProcess,
    PROCESS_TERMINATE,
    TerminateProcess,
};

/// Terminate a process on Windows using the Windows API
pub fn terminate_process(pid: Pid) -> Result<(), String> {
    unsafe {
        // Open the process with termination rights
        let handle = OpenProcess(PROCESS_TERMINATE, false, pid.as_u32())
            .map_err(|e| format!("Failed to open process: {}", e))?;

        // Create a safe handle that will be closed automatically when dropped
        let safe_handle = SafeHandle::new(handle).ok_or_else(|| "Invalid process handle".to_string())?;

        // Terminate the process with exit code 1
        TerminateProcess(*safe_handle, 1).map_err(|e| format!("Failed to terminate process: {}", e))?;

        Ok(())
    }
}

struct SafeHandle(HANDLE);

impl SafeHandle {
    fn new(handle: HANDLE) -> Option<Self> {
        if !handle.is_invalid() { Some(Self(handle)) } else { None }
    }
}

impl Drop for SafeHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

impl Deref for SafeHandle {
    type Target = HANDLE;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use std::process::Command;
    use std::time::Duration;

    use super::*;

    // Helper to create a long-running process for testing
    fn spawn_test_process() -> std::process::Child {
        let mut command = Command::new("cmd");
        command.args(["/C", "ping 127.0.0.1 -n 30 > nul"]);
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

    #[test]
    fn test_safe_handle() {
        // Test creating a SafeHandle with an invalid handle
        let invalid_handle = HANDLE(0);
        let safe_handle = SafeHandle::new(invalid_handle);
        assert!(safe_handle.is_none(), "SafeHandle should be None for invalid handle");

        // We can't easily test a valid handle without actually opening a process,
        // which would require additional setup and teardown
    }
}
