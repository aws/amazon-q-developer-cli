use std::process::ExitCode;

use anstream::println;
use clap::{
    ArgGroup,
    Args,
    Subcommand,
};
use crossterm::style::Stylize;
use eyre::{
    Result,
    WrapErr,
};
use globset::Glob;
use serde_json::json;
use strum::IntoEnumIterator;

use super::OutputFormat;
use crate::database::settings::Setting;
use crate::os::Os;
use crate::util::directories;

#[derive(Clone, Debug, Subcommand, PartialEq, Eq)]
pub enum SettingsSubcommands {
    /// Open the settings file
    Open,
    /// List settings
    List {
        /// Show all available settings
        #[arg(long)]
        all: bool,
        /// Format of the output
        #[arg(long, short, value_enum, default_value_t)]
        format: OutputFormat,
        /// Whether or not we want to modify state instead
        #[arg(long, short, hide = true)]
        state: bool,
    },
    /// List configured  settings
    #[command(hide = true)]
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
#[command(group(ArgGroup::new("vals").requires("key").args(&["value", "format"])))]
pub struct SettingsArgs {
    #[command(subcommand)]
    cmd: Option<SettingsSubcommands>,
    /// key
    key: Option<String>,
    /// value
    value: Option<String>,
    /// Delete a key (No value needed)
    #[arg(long, short)]
    delete: bool,
    /// Format of the output
    #[arg(long, short, value_enum, default_value_t)]
    format: OutputFormat,
}

#[derive(Debug)]
struct SettingInfo {
    /// Setting key
    key: String,
    /// Setting description
    description: String,
    /// Current setting value
    current_value: Option<serde_json::Value>,
}

/// Print configured settings
fn print_configured_settings(os: &Os, format: OutputFormat) -> Result<()> {
    let settings = os.database.settings.map().clone();
    match format {
        OutputFormat::Plain => {
            for (key, value) in settings {
                println!("{key} = {value}");
            }
        },
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(&settings)?);
        },
        OutputFormat::JsonPretty => {
            println!("{}", serde_json::to_string_pretty(&settings)?);
        },
    }
    Ok(())
}

/// Print internal state table dump (hidden debug feature)
fn print_state_dump(os: &Os, format: OutputFormat) -> Result<()> {
    let settings = os.database.get_all_entries()?;
    match format {
        OutputFormat::Plain => {
            for (key, value) in settings {
                println!("{key} = {value}");
            }
        },
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(&settings)?);
        },
        OutputFormat::JsonPretty => {
            println!("{}", serde_json::to_string_pretty(&settings)?);
        },
    }
    Ok(())
}

/// Collect all settings with their metadata and current values
fn collect_settings(os: &Os) -> Vec<SettingInfo> {
    use strum::EnumMessage;

    Setting::iter()
        .map(|setting| {
            let key = setting.as_ref().to_string();
            let description = setting.get_message().unwrap_or("No description").to_string();
            let current_value = os.database.settings.get(setting).cloned();

            SettingInfo {
                key,
                description,
                current_value,
            }
        })
        .collect()
}

/// Print settings list in plain text format with colors
fn print_settings_plain(settings: &[SettingInfo]) {
    for setting in settings {
        println!("{}", setting.key.as_str().cyan().bold());
        println!("  Description: {}", setting.description);
        match &setting.current_value {
            Some(value) => println!("  Current: {}", value.to_string().green()),
            None => println!("  Current: {}", "not set".dim()),
        }
        println!();
    }
}

/// Print settings list in JSON or JSON Pretty format
fn print_settings_json(settings: &[SettingInfo], pretty: bool) -> Result<()> {
    let settings_list: Vec<_> = settings
        .iter()
        .map(|s| {
            json!({
                "key": s.key,
                "description": s.description,
                "current_value": s.current_value,
            })
        })
        .collect();

    let output = if pretty {
        serde_json::to_string_pretty(&settings_list)?
    } else {
        serde_json::to_string(&settings_list)?
    };

    println!("{}", output);
    Ok(())
}

/// Print all available settings
fn print_all_settings(os: &Os, format: OutputFormat) -> Result<()> {
    let settings = collect_settings(os);

    match format {
        OutputFormat::Plain => {
            print_settings_plain(&settings);
            Ok(())
        },
        OutputFormat::Json => print_settings_json(&settings, false),
        OutputFormat::JsonPretty => print_settings_json(&settings, true),
    }
}

impl SettingsArgs {
    pub async fn execute(&self, os: &mut Os) -> Result<ExitCode> {
        match self.cmd {
            Some(SettingsSubcommands::Open) => {
                let file = directories::settings_path().context("Could not get settings path")?;

                let editor =
                    crate::util::env_var::try_get_editor().context("The EDITOR environment variable is not set")?;
                tokio::process::Command::new(editor).arg(file).spawn()?.wait().await?;
                Ok(ExitCode::SUCCESS)
            },
            Some(SettingsSubcommands::List { all, format, state }) => {
                if state {
                    print_state_dump(os, format)?;
                } else if all {
                    print_all_settings(os, format)?;
                } else {
                    print_configured_settings(os, format)?;
                }
                Ok(ExitCode::SUCCESS)
            },
            Some(SettingsSubcommands::All { format, state }) => {
                // Deprecated: redirect to List behavior for backward compatibility
                if state {
                    print_state_dump(os, format)?;
                } else {
                    print_configured_settings(os, format)?;
                }
                Ok(ExitCode::SUCCESS)
            },
            None => {
                let Some(key) = &self.key else {
                    if self.delete {
                        return Err(eyre::eyre!(
                            "the argument {} requires a {}\n Usage: q settings {} {}",
                            "'--delete'".yellow(),
                            "<KEY>".green(),
                            "--delete".yellow(),
                            "<KEY>".green()
                        ));
                    }
                    return Ok(ExitCode::SUCCESS);
                };

                let key = Setting::try_from(key.as_str())?;
                match (&self.value, self.delete) {
                    (Some(_), true) => Err(eyre::eyre!(
                        "the argument {} cannot be used with {}\n Usage: q settings {} {key}",
                        "'--delete'".yellow(),
                        "'[VALUE]'".yellow(),
                        "--delete".yellow()
                    )),
                    (None, false) => match os.database.settings.get(key) {
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
                        os.database.settings.set(key, value).await?;
                        Ok(ExitCode::SUCCESS)
                    },
                    (None, true) => {
                        let glob = Glob::new(key.as_ref())
                            .context("Could not create glob")?
                            .compile_matcher();
                        let map = os.database.settings.map();
                        let keys_to_remove = map.keys().filter(|key| glob.is_match(key)).cloned().collect::<Vec<_>>();

                        match keys_to_remove.len() {
                            0 => {
                                return Err(eyre::eyre!("No settings found matching {key}"));
                            },
                            1 => {
                                println!("Removing {:?}", keys_to_remove[0]);
                                os.database
                                    .settings
                                    .remove(Setting::try_from(keys_to_remove[0].as_str())?)
                                    .await?;
                            },
                            _ => {
                                for key in &keys_to_remove {
                                    if let Ok(key) = Setting::try_from(key.as_str()) {
                                        println!("Removing `{key}`");
                                        os.database.settings.remove(key).await?;
                                    }
                                }
                            },
                        }

                        Ok(ExitCode::SUCCESS)
                    },
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_delete_with_value_error() {
        let mut os = Os::new().await.unwrap();

        let settings_args = SettingsArgs {
            cmd: None,
            key: Some("chat.defaultAgent".to_string()),
            value: Some("test_value".to_string()),
            delete: true,
            format: OutputFormat::Plain,
        };

        let result = settings_args.execute(&mut os).await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("the argument"));
        assert!(error_msg.contains("--delete"));
        assert!(error_msg.contains("Usage:"));
    }
}
