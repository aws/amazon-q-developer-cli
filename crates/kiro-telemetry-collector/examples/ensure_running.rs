//! Compatibility alias for the `spawn_local` example. Some scripts in the
//! design doc refer to `cargo run --example ensure_running ...`; this file
//! exists so either name works.

use std::process::ExitCode;

use kiro_telemetry_collector::{
    StateDir,
    ensure_running,
    status,
};

#[tokio::main]
async fn main() -> ExitCode {
    let endpoint = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:9999/v1/metrics".to_string());

    let state = match StateDir::default() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("state dir error: {e}");
            return ExitCode::from(1);
        },
    };

    match ensure_running(&state, &endpoint).await {
        Ok(handle) => {
            println!("ensure_running: {handle:?}");
        },
        Err(e) => {
            eprintln!("ensure_running failed: {e}");
            return ExitCode::from(1);
        },
    }

    match status(&state).await {
        Ok(st) => println!("status: {st:?}"),
        Err(e) => eprintln!("status failed: {e}"),
    }
    ExitCode::SUCCESS
}
