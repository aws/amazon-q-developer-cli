use nix::sys::signal::Signal;
use sysinfo::Pid;

pub fn terminate_process(pid: Pid) -> Result<(), String> {
    let nix_pid = nix::unistd::Pid::from_raw(pid.as_u32() as i32);
    nix::sys::signal::kill(nix_pid, Signal::SIGTERM).map_err(|e| format!("Failed to terminate process: {}", e))
}
