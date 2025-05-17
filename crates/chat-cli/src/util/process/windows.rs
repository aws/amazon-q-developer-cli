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
