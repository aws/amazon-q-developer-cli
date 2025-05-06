mod api_client;
mod auth;
mod cli;
mod diagnostics;
mod fig_aws_common;
mod fig_install;
mod fig_log;
mod fig_os_shim;
mod fig_settings;
mod fig_telemetry;
mod fig_telemetry_core;
mod fig_util;
mod mcp_client;
mod request;

use std::process::ExitCode;

use anstream::eprintln;
use clap::Parser;
use clap::error::{
    ContextKind,
    ErrorKind,
};
use crossterm::style::Stylize;
use eyre::Result;
use fig_log::get_log_level_max;
use tracing::metadata::LevelFilter;

use crate::fig_telemetry::finish_telemetry;
use crate::fig_util::{
    CHAT_BINARY_NAME,
    PRODUCT_NAME,
};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> Result<ExitCode> {
    color_eyre::install()?;

    let multithread = matches!(
        std::env::args().nth(1).as_deref(),
        Some("init" | "_" | "internal" | "completion" | "hook" | "chat")
    );

    let parsed = match cli::Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            let _ = err.print();

            let unknown_arg = matches!(err.kind(), ErrorKind::UnknownArgument | ErrorKind::InvalidSubcommand)
                && !err.context().any(|(context_kind, _)| {
                    matches!(
                        context_kind,
                        ContextKind::SuggestedSubcommand | ContextKind::SuggestedArg
                    )
                });

            if unknown_arg {
                eprintln!(
                    "\nThis command may be valid in newer versions of the {PRODUCT_NAME} CLI. Try running {} {}.",
                    CHAT_BINARY_NAME.magenta(),
                    "update".magenta()
                );
            }

            return Ok(ExitCode::from(err.exit_code().try_into().unwrap_or(2)));
        },
    };

    let verbose = parsed.verbose > 0;

    let runtime = if multithread {
        tokio::runtime::Builder::new_multi_thread()
    } else {
        tokio::runtime::Builder::new_current_thread()
    }
    .enable_all()
    .build()?;

    let result = runtime.block_on(async {
        let result = parsed.execute().await;
        finish_telemetry().await;
        result
    });

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
