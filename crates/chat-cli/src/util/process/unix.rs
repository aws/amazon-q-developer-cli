use nix::libc::pid_t;
use nix::sys::signal::Signal;
use sysinfo::Pid;

impl From<nix::unistd::Pid> for Pid {
    fn from(pid: nix::unistd::Pid) -> Self {
        Pid(pid.as_raw())
    }
}

impl From<Pid> for nix::unistd::Pid {
    fn from(pid: Pid) -> Self {
        nix::unistd::Pid::from_raw(pid.0)
    }
}

/// Terminate a process on macOS
pub fn terminate_process(pid: Pid) -> Result<(), String> {
    let nix_pid = nix::unistd::Pid::from_raw(pid.0);
    nix::sys::signal::kill(nix_pid, Signal::SIGTERM).map_err(|e| format!("Failed to terminate process: {}", e))
}
