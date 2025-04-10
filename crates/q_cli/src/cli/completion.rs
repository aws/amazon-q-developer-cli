use std::io::{
    Write,
    stdout,
};
use std::process::ExitCode;

use clap::{
    Args,
    CommandFactory,
    ValueEnum,
};
use eyre::Result;
use fig_util::CLI_BINARY_NAME;

use crate::cli::Cli;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Shells {
    /// Bash shell completions
    Bash,
    /// Fish shell completions
    Fish,
    /// Zsh shell completions
    Zsh,
    /// Fig completion spec
    Fig,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct CompletionArgs {
    /// Shell to generate the completion spec for
    #[arg(value_enum, default_value_t = Shells::Zsh)]
    shell: Shells,
}

impl CompletionArgs {
    pub fn execute(&self) -> Result<ExitCode> {
        writeln!(stdout(), "{}", match self.shell {
            Shells::Bash => generation_completions(clap_complete::shells::Bash),
            Shells::Fish => generation_completions(clap_complete::shells::Fish),
            Shells::Zsh => generation_completions(clap_complete::shells::Zsh),
            Shells::Fig => generation_completions(clap_complete_fig::Fig),
        })
        .ok();
        Ok(ExitCode::SUCCESS)
    }
}

fn generation_completions(gen: impl clap_complete::Generator) -> String {
    let mut cli = Cli::command();
    let mut buffer = Vec::new();

    clap_complete::generate(gen, &mut cli, CLI_BINARY_NAME, &mut buffer);

    String::from_utf8_lossy(&buffer).into()
}
