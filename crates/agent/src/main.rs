mod acp;
mod cli;

use std::process::ExitCode;

use clap::Parser;
use cli::CliArgs;
use eyre::Result;

fn main() -> Result<ExitCode> {
    color_eyre::install()?;

    let cli = CliArgs::parse();

    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;

    runtime.block_on(cli.execute())
}
