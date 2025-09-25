mod api_client;
mod auth;
mod aws_common;
mod cli;
mod database;
mod logging;
mod mcp_client;
mod os;
mod request;
mod telemetry;
mod util;

#[cfg(target_os = "macos")]
embed_plist::embed_info_plist!("Info.plist");

use std::process::ExitCode;

use anstream::eprintln;
use clap::Parser;
use crossterm::style::Stylize;
use eyre::Result;
use logging::get_log_level_max;
use tracing::metadata::LevelFilter;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> Result<ExitCode> {
    color_eyre::install()?;

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
                eprintln!("{} {err:?}", "error:".bold().red());
            } else {
                eprintln!("{} {err}", "error:".bold().red());
            }

            Ok(ExitCode::FAILURE)
        },
    }
}
