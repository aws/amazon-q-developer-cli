#[cfg(target_os = "macos")]
mod imk;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(not(target_os = "macos"))]
use std::process::ExitCode;

#[cfg(target_os = "macos")]
pub use macos::main;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("Fig input method is only supported on macOS");
    ExitCode::FAILURE
}
