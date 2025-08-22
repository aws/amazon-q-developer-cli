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
use crate::database::settings::Setting;
use crate::os::Os;
use crate::util::directories;

#[derive(Clone, Debug, Subcommand, PartialEq, Eq)]
pub enum SettingsSubcommands {
    /// Open the settings file
    Open,
    /// List all the settings
    All {
        /// Format of the output
        #[arg(long, short, value_enum, default_value_t)]
        format: OutputFormat,
        /// Whether or not we want to modify state instead
        #[arg(long, short, hide = true)]
        state: bool,
    },
}

#[derive(Clone, Debug, Args, PartialEq, Eq)]
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
    pub async fn execute(&self, os: &mut Os) -> Result<ExitCode> {
        match self.cmd {
            Some(SettingsSubcommands::Open) => {
                let file = directories::settings_path().context("Could not get settings path")?;
                if let Ok(editor) = os.env.get("EDITOR") {
                    tokio::process::Command::new(editor).arg(file).spawn()?.wait().await?;
                    Ok(ExitCode::SUCCESS)
                } else {
                    bail!("The EDITOR environment variable is not set")
                }
            },
            Some(SettingsSubcommands::All { format, state }) => {
                let settings = match state {
                    true => os.database.get_all_entries()?,
                    false => os.database.settings.map().clone(),
                };

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
                let Some(key_str) = &self.key else {
                    return Ok(ExitCode::SUCCESS);
                };

                let key = Setting::try_from(key_str.as_str())?;
                match (&self.value, self.delete) {
                    (None, false) => {
                        let value = os.database.settings.get(key);

                        match value {
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
                                OutputFormat::Plain => Err(eyre::eyre!("No value associated with {}", key.as_string())),
                                OutputFormat::Json | OutputFormat::JsonPretty => {
                                    println!("null");
                                    Ok(ExitCode::SUCCESS)
                                },
                            },
                        }
                    },
                    (Some(value_str), false) => {
                        let value = serde_json::from_str(value_str).unwrap_or_else(|_| json!(value_str));
                        os.database.settings.set(key, value).await?;
                        Ok(ExitCode::SUCCESS)
                    },
                    (None, true) => {
                        let key_string = key.as_string();
                        let glob = Glob::new(&key_string)
                            .context("Could not create glob")?
                            .compile_matcher();
                        let map = os.database.settings.map();
                        let keys_to_remove = map.keys().filter(|k| glob.is_match(k)).cloned().collect::<Vec<_>>();

                        match keys_to_remove.len() {
                            0 => {
                                return Err(eyre::eyre!("No settings found matching {}", key_string));
                            },
                            1 => {
                                println!("Removing {:?}", keys_to_remove[0]);
                                os.database.settings.remove(key).await?;
                            },
                            _ => {
                                for key_str in &keys_to_remove {
                                    if let Ok(parsed_key) = Setting::try_from(key_str.as_str()) {
                                        println!("Removing `{}`", parsed_key.as_string());
                                        os.database.settings.remove(parsed_key).await?;
                                    }
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
