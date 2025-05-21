pub use sysinfo::Pid;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
pub use unix::*;
