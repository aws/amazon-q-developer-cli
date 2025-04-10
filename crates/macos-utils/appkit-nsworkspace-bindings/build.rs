#[cfg(target_os = "macos")]
mod build_mac;
#[cfg(target_os = "macos")]
use crate::build_mac::build;

#[cfg(target_os = "macos")]
fn main() {
    build();
}

#[cfg(not(target_os = "macos"))]
fn main() {}
