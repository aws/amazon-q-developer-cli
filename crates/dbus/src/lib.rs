#[cfg(target_os = "linux")]
mod dbus;
#[cfg(target_os = "linux")]
pub use crate::dbus::*;

#[cfg(not(target_os = "linux"))]
pub fn _dummy() {}
