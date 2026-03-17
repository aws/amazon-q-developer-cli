mod agent;
mod api_client;
mod auth;
mod aws_common;
mod cleanup;
mod cli;
mod constants;
mod database;
mod embedded_tui;
mod feature_flags;
mod logging;
mod mcp_client;
mod mcp_registry;
mod os;
mod request;
mod telemetry;
mod theme;
mod util;

use std::process::ExitCode;

use anstream::eprintln;
use clap::Parser;
use eyre::Result;
use logging::get_log_level_max;
use theme::StyledText;
use tracing::metadata::LevelFilter;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> Result<ExitCode> {
    color_eyre::install()?;

    // Spawn the main logic on a thread with a larger stack to prevent stack
    // overflows on Windows. Debug builds have much larger stack frames due to
    // unoptimized async state machines, and the default 1MB Windows stack is
    // insufficient for deep async call chains (e.g. hyper HTTP server handling
    // OAuth callbacks during login). 8MB matches the default on macOS/Linux.
    const STACK_SIZE: usize = 8 * 1024 * 1024; // 8 MB
    let builder = std::thread::Builder::new()
        .name("main-with-stack".into())
        .stack_size(STACK_SIZE);

    let handler = builder.spawn(main_inner)?;
    handler.join().unwrap_or_else(|panic_payload| {
        std::panic::resume_unwind(panic_payload);
    })
}

fn main_inner() -> Result<ExitCode> {
    let parsed = match cli::Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            err.print().ok();
            return Ok(ExitCode::from(err.exit_code().try_into().unwrap_or(2)));
        },
    };

    let verbose = parsed.verbose > 0;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let result = runtime.block_on(parsed.execute());

    match result {
        Ok(exit_code) => Ok(exit_code),
        Err(err) => {
            if verbose || get_log_level_max() > LevelFilter::INFO {
                eprintln!("{} {err:?}", StyledText::error("error:"));
            } else {
                eprintln!("{} {err}", StyledText::error("error:"));
            }

            Ok(ExitCode::FAILURE)
        },
    }
}
