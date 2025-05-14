pub mod cli;
pub mod util;

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
use fig_util::{
    CLI_BINARY_NAME,
    PRODUCT_NAME,
};
use tracing::metadata::LevelFilter;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> Result<ExitCode> {
    color_eyre::install()?;
    fig_telemetry::set_dispatch_mode(fig_telemetry::DispatchMode::On);
    fig_telemetry::init_global_telemetry_emitter();

    let mut args = std::env::args();
    let subcommand = args.nth(1);
    let multithread = matches!(
        subcommand.as_deref(),
        Some("init" | "_" | "internal" | "completion" | "hook" | "chat")
    );

    let runtime = if multithread {
        tokio::runtime::Builder::new_multi_thread()
    } else {
        tokio::runtime::Builder::new_current_thread()
    }
    .enable_all()
    .build()?;

    // Hack as clap doesn't expose a custom command help.
    if subcommand.as_deref() == Some("chat") && args.any(|arg| ["--help", "-h"].contains(&arg.as_str())) {
        runtime.block_on(cli::Cli::execute_chat(Some(vec!["--help".to_owned()]), true))?;
    }

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
                    CLI_BINARY_NAME.magenta(),
                    "update".magenta()
                );
            }

            return Ok(ExitCode::from(err.exit_code().try_into().unwrap_or(2)));
        },
    };

    let verbose = parsed.verbose > 0;

    let result = runtime.block_on(async {
        let result = parsed.execute().await;
        fig_telemetry::finish_telemetry().await;
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
