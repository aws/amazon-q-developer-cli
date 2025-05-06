use std::io::{
    IsTerminal,
    stdout,
};
use std::process::ExitCode;

use anstream::println;
use clap::Args;
use color_eyre::Result;
use crossterm::terminal::{
    Clear,
    ClearType,
};
use crossterm::{
    cursor,
    execute,
};
use spinners::{
    Spinner,
    Spinners,
};

use super::OutputFormat;
use crate::platform::diagnostics::Diagnostics;

#[derive(Debug, Args, PartialEq, Eq)]
pub struct DiagnosticArgs {
    /// The format of the output
    #[arg(long, short, value_enum, default_value_t)]
    format: OutputFormat,
    /// Force limited diagnostic output
    #[arg(long)]
    force: bool,
}

impl DiagnosticArgs {
    pub async fn execute(&self) -> Result<ExitCode> {
        let spinner = if stdout().is_terminal() {
            Some(Spinner::new(Spinners::Dots, "Generating...".into()))
        } else {
            None
        };

        if spinner.is_some() {
            execute!(std::io::stdout(), cursor::Hide)?;

            ctrlc::set_handler(move || {
                execute!(std::io::stdout(), cursor::Show).ok();
                std::process::exit(1);
            })?;
        }

        let diagnostics = Diagnostics::new().await;

        if let Some(mut sp) = spinner {
            sp.stop();
            execute!(std::io::stdout(), Clear(ClearType::CurrentLine), cursor::Show)?;
            println!();
        }

        self.format.print(
            || diagnostics.user_readable().expect("Failed to run user_readable()"),
            || &diagnostics,
        );

        Ok(ExitCode::SUCCESS)
    }
}
