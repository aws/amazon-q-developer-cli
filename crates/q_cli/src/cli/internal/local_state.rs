use anstream::println;
use clap::{
    ArgGroup,
    Args,
    Subcommand,
};
use crossterm::style::Stylize;
use eyre::{
    Result,
    eyre,
};
use fig_ipc::local::restart_settings_listener;
use fig_util::PRODUCT_NAME;
use serde_json::json;

use crate::cli::OutputFormat;

#[derive(Debug, PartialEq, Eq, Subcommand)]
pub enum LocalStateSubcommand {
    /// Reload the state listener
    Init,
    /// List all the settings
    All {
        #[arg(long, short, value_enum, default_value_t)]
        format: OutputFormat,
    },
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(subcommand_negates_reqs = true)]
#[command(args_conflicts_with_subcommands = true)]
#[command(group(ArgGroup::new("vals").requires("key").args(&["value", "delete", "format"])))]
pub struct LocalStateArgs {
    #[command(subcommand)]
    cmd: Option<LocalStateSubcommand>,
    /// Key of the state
    key: Option<String>,
    /// Value of the state
    value: Option<String>,
    #[arg(long, short)]
    /// Delete the state
    delete: bool,
    #[arg(long, short, value_enum, default_value_t)]
    /// Format of the output
    format: OutputFormat,
}

impl LocalStateArgs {
    pub async fn execute(&self) -> Result<()> {
        macro_rules! print_connection_error {
            () => {
                println!(
                    "\n{}\n{PRODUCT_NAME} might not be running, to launch {PRODUCT_NAME} run: {}\n",
                    format!("Unable to connect to {PRODUCT_NAME}").bold(),
                    "fig launch".magenta()
                )
            };
        }

        match self.cmd {
            Some(LocalStateSubcommand::Init) => match restart_settings_listener().await {
                Ok(()) => {
                    println!("\nState listener restarted\n");
                    Ok(())
                },
                Err(err) => {
                    print_connection_error!();
                    Err(err.into())
                },
            },
            Some(LocalStateSubcommand::All { format }) => {
                let state = fig_settings::state::all()?;
                match format {
                    OutputFormat::Plain => {
                        for (key, value) in state.iter() {
                            println!("{key} = {value}");
                        }
                    },
                    OutputFormat::Json => println!("{}", serde_json::to_string(&state)?),
                    OutputFormat::JsonPretty => {
                        println!("{}", serde_json::to_string_pretty(&state)?);
                    },
                }

                Ok(())
            },
            None => match &self.key {
                Some(key) => match (&self.value, self.delete) {
                    (None, false) => match fig_settings::state::get_value(key)? {
                        Some(value) => {
                            match self.format {
                                OutputFormat::Plain => match value.as_str() {
                                    Some(value) => println!("{value}"),
                                    None => println!("{value:#}"),
                                },
                                OutputFormat::Json => println!("{value}"),
                                OutputFormat::JsonPretty => println!("{value:#}"),
                            }
                            Ok(())
                        },
                        None => match self.format {
                            OutputFormat::Plain => Err(eyre::eyre!("No value associated with {key}")),
                            OutputFormat::Json | OutputFormat::JsonPretty => {
                                println!("null");
                                Ok(())
                            },
                        },
                    },
                    (None, true) => {
                        fig_settings::state::remove_value(key)?;
                        println!("Successfully updated state");
                        Ok(())
                    },
                    (Some(value), false) => {
                        let value: serde_json::Value = serde_json::from_str(value).unwrap_or_else(|_| json!(value));
                        fig_settings::state::set_value(key, value)?;
                        println!("Successfully updated state");
                        Ok(())
                    },
                    (Some(_), true) => Err(eyre!("Cannot delete a value with a value")),
                },
                None => Err(eyre!("No key specified")),
            },
        }
    }
}
