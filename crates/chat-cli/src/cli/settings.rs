use std::process::ExitCode;

use anstream::println;
use clap::{
    ArgGroup,
    Args,
    Subcommand,
};
use eyre::{
    Result,
    WrapErr,
    bail,
};
use globset::Glob;
use serde_json::json;

use super::OutputFormat;
use crate::settings::JsonStore;
use crate::fig_util::{
    CliContext,
    directories,
};

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum SettingsSubcommands {
    /// Open the settings file
    Open,
    /// List all the settings
    All {
        /// Format of the output
        #[arg(long, short, value_enum, default_value_t)]
        format: OutputFormat,
    },
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(subcommand_negates_reqs = true)]
#[command(args_conflicts_with_subcommands = true)]
#[command(group(ArgGroup::new("vals").requires("key").args(&["value", "delete", "format"])))]
pub struct SettingsArgs {
    #[command(subcommand)]
    cmd: Option<SettingsSubcommands>,
    /// key
    key: Option<String>,
    /// value
    value: Option<String>,
    /// Delete a value
    #[arg(long, short)]
    delete: bool,
    /// Format of the output
    #[arg(long, short, value_enum, default_value_t)]
    format: OutputFormat,
}

impl SettingsArgs {
    pub async fn execute(&self, cli_context: &CliContext) -> Result<ExitCode> {
        match self.cmd {
            Some(SettingsSubcommands::Open) => {
                let file = directories::settings_path().context("Could not get settings path")?;
                if let Ok(editor) = cli_context.context().env().get("EDITOR") {
                    tokio::process::Command::new(editor).arg(file).spawn()?.wait().await?;
                    Ok(ExitCode::SUCCESS)
                } else {
                    bail!("The EDITOR environment variable is not set")
                }
            },
            Some(SettingsSubcommands::All { format }) => {
                let settings = crate::settings::OldSettings::load()?.map().clone();

                match format {
                    OutputFormat::Plain => {
                        for (key, value) in settings {
                            println!("{key} = {value}");
                        }
                    },
                    OutputFormat::Json => println!("{}", serde_json::to_string(&settings)?),
                    OutputFormat::JsonPretty => {
                        println!("{}", serde_json::to_string_pretty(&settings)?);
                    },
                }

                Ok(ExitCode::SUCCESS)
            },
            None => {
                let Some(key) = &self.key else {
                    return Ok(ExitCode::SUCCESS);
                };

                match (&self.value, self.delete) {
                    (None, false) => match crate::settings::settings::get_value(key)? {
                        Some(value) => {
                            match self.format {
                                OutputFormat::Plain => match value.as_str() {
                                    Some(value) => println!("{value}"),
                                    None => println!("{value:#}"),
                                },
                                OutputFormat::Json => println!("{value}"),
                                OutputFormat::JsonPretty => println!("{value:#}"),
                            }
                            Ok(ExitCode::SUCCESS)
                        },
                        None => match self.format {
                            OutputFormat::Plain => Err(eyre::eyre!("No value associated with {key}")),
                            OutputFormat::Json | OutputFormat::JsonPretty => {
                                println!("null");
                                Ok(ExitCode::SUCCESS)
                            },
                        },
                    },
                    (Some(value_str), false) => {
                        let value = serde_json::from_str(value_str).unwrap_or_else(|_| json!(value_str));
                        crate::settings::settings::set_value(key, value)?;
                        Ok(ExitCode::SUCCESS)
                    },
                    (None, true) => {
                        let glob = Glob::new(key).context("Could not create glob")?.compile_matcher();
                        let settings = crate::settings::OldSettings::load()?;
                        let map = settings.map();
                        let keys_to_remove = map.keys().filter(|key| glob.is_match(key)).collect::<Vec<_>>();

                        match keys_to_remove.len() {
                            0 => {
                                return Err(eyre::eyre!("No settings found matching {key}"));
                            },
                            1 => {
                                println!("Removing {:?}", keys_to_remove[0]);
                                crate::settings::settings::remove_value(keys_to_remove[0])?;
                            },
                            _ => {
                                println!("Removing:");
                                for key in &keys_to_remove {
                                    println!("  - {key}");
                                }

                                for key in &keys_to_remove {
                                    crate::settings::settings::remove_value(key)?;
                                }
                            },
                        }

                        Ok(ExitCode::SUCCESS)
                    },
                    _ => Ok(ExitCode::SUCCESS),
                }
            },
        }
    }
}
