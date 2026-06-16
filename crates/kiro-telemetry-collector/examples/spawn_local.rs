//! Tiny driver used by the workflow's e2e verification step.
//!
//! Calls `ensure_running` with a (deliberately non-routable) upstream
//! endpoint and prints the resulting handle as JSON. The collector's queue
//! will absorb any received OTLP records since upstream is unreachable, which
//! is exactly what the e2e step asserts.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example spawn_local --package kiro-telemetry-collector -- \
//!   --upstream-endpoint http://127.0.0.1:9999/v1/metrics
//! ```

use std::process::ExitCode;

use kiro_telemetry_collector::{
    StateDir,
    ensure_running,
};

#[tokio::main]
async fn main() -> ExitCode {
    let mut endpoint = "http://127.0.0.1:9999/v1/metrics".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--upstream-endpoint" => {
                if let Some(v) = args.next() {
                    endpoint = v;
                }
            },
            "-h" | "--help" => {
                println!("Usage: spawn_local [--upstream-endpoint URL]");
                return ExitCode::SUCCESS;
            },
            other => {
                eprintln!("unknown arg: {other}");
                return ExitCode::from(2);
            },
        }
    }

    let state = match StateDir::default() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("state dir error: {e}");
            return ExitCode::from(1);
        },
    };

    match ensure_running(&state, &endpoint).await {
        Ok(handle) => {
            match serde_json::to_string_pretty(&handle) {
                Ok(s) => println!("{s}"),
                Err(e) => {
                    eprintln!("serialize handle: {e}");
                    return ExitCode::from(1);
                },
            }
            ExitCode::SUCCESS
        },
        Err(e) => {
            eprintln!("ensure_running failed: {e}");
            ExitCode::from(1)
        },
    }
}
