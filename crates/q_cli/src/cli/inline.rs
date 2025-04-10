use std::fmt::Write;
use std::process::ExitCode;

use anstream::println;
use clap::Subcommand;
use crossterm::style::Stylize;
use eyre::{
    Result,
    bail,
};
use fig_api_client::{
    Client,
    Customization,
};
use fig_ipc::{
    BufferedUnixStream,
    SendMessage,
};
use fig_proto::figterm::figterm_request_message::Request;
use fig_proto::figterm::{
    FigtermRequestMessage,
    InlineShellCompletionSetEnabledRequest,
};
use fig_util::env_var::QTERM_SESSION_ID;
use tracing::error;

use super::OutputFormat;
use crate::util::CliContext;

const INLINE_ENABLED_SETTINGS_KEY: &str = "inline.enabled";

#[derive(Debug, Clone, PartialEq, Subcommand)]
pub enum InlineSubcommand {
    /// Enables inline
    Enable,
    /// Disables inline
    Disable,
    /// Shows the status of inline
    Status,
    /// Select a customization if you have any available
    SetCustomization {
        /// The arn of the customization to use
        arn: Option<String>,
    },
    /// Show the available customizations
    ShowCustomizations {
        #[arg(long, short, value_enum, default_value_t)]
        format: OutputFormat,
    },
}

impl InlineSubcommand {
    pub async fn execute(&self, cli_context: &CliContext) -> Result<ExitCode> {
        let settings = cli_context.settings();
        let state = cli_context.state();

        match self {
            InlineSubcommand::Enable => {
                settings.set_value(INLINE_ENABLED_SETTINGS_KEY, true)?;
                if let Err(err) = send_set_enabled(true).await {
                    error!("Failed to send set enabled message: {err}");
                }
                println!("{}", "Inline enabled".magenta());
            },
            InlineSubcommand::Disable => {
                settings.set_value(INLINE_ENABLED_SETTINGS_KEY, false)?;
                if let Err(err) = send_set_enabled(false).await {
                    error!("Failed to send set enabled message: {err}");
                }
                println!("{}", "Inline disabled".magenta());
            },
            InlineSubcommand::Status => {
                let enabled = settings.get_bool(INLINE_ENABLED_SETTINGS_KEY)?.unwrap_or(true);
                println!("Inline is {}", if enabled { "enabled" } else { "disabled" }.bold());
            },
            InlineSubcommand::SetCustomization { arn } => {
                let customizations = Client::new().await?.list_customizations().await?;
                if customizations.is_empty() {
                    println!("No customizations found");
                    return Ok(ExitCode::FAILURE);
                }

                // if the user has specified an arn, use it
                if let Some(arn) = arn {
                    let Some(customization) = customizations.iter().find(|c| c.arn == *arn) else {
                        println!("Customization not found");
                        return Ok(ExitCode::FAILURE);
                    };

                    customization.save_selected(state)?;
                    println!(
                        "Customization {} selected",
                        customization.name.as_deref().unwrap_or_default().bold()
                    );
                    return Ok(ExitCode::SUCCESS);
                }

                let names = customizations
                    .iter()
                    .map(|c| {
                        format!(
                            "{} - {}",
                            c.name.as_deref().unwrap_or_default().bold(),
                            c.description.as_deref().unwrap_or_default()
                        )
                    })
                    .chain(["None".bold().to_string()])
                    .collect::<Vec<_>>();

                let select = match crate::util::choose("Select a customization", &names)? {
                    Some(select) => select,
                    None => bail!("No customization selected"),
                };

                if select == customizations.len() {
                    Customization::delete_selected(state)?;
                    println!("Customization unset");
                } else {
                    customizations[select].save_selected(state)?;
                    println!(
                        "Customization {} selected",
                        customizations[select].name.as_deref().unwrap_or_default().bold()
                    );
                }
            },
            InlineSubcommand::ShowCustomizations { format } => {
                let customizations = Client::new().await?.list_customizations().await?;
                format.print(
                    || {
                        if customizations.is_empty() {
                            "No customizations found".into()
                        } else {
                            let mut s = String::new();
                            for customization in &customizations {
                                writeln!(s, "{}", customization.name.as_deref().unwrap_or_default().bold()).unwrap();
                                if let Some(description) = &customization.description {
                                    s.push_str(description);
                                }
                                s.push('\n');
                            }
                            s
                        }
                    },
                    || &customizations,
                );
            },
        }
        Ok(ExitCode::SUCCESS)
    }
}

async fn send_set_enabled(enabled: bool) -> Result<()> {
    let session_id = std::env::var(QTERM_SESSION_ID)?;
    let figterm_socket_path = fig_util::directories::figterm_socket_path(&session_id)?;
    let mut conn = BufferedUnixStream::connect(figterm_socket_path).await?;
    conn.send_message(FigtermRequestMessage {
        request: Some(Request::InlineShellCompletionSetEnabled(
            InlineShellCompletionSetEnabledRequest { enabled },
        )),
    })
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]

    async fn test_subcommands() {
        let cli_context = CliContext::new_fake();
        let settings = cli_context.settings();

        InlineSubcommand::Enable.execute(&cli_context).await.unwrap();
        assert!(settings.get_bool(INLINE_ENABLED_SETTINGS_KEY).unwrap().unwrap());
        InlineSubcommand::Status.execute(&cli_context).await.unwrap();

        InlineSubcommand::Disable.execute(&cli_context).await.unwrap();
        assert!(!settings.get_bool(INLINE_ENABLED_SETTINGS_KEY).unwrap().unwrap());
        InlineSubcommand::Status.execute(&cli_context).await.unwrap();
    }
}
