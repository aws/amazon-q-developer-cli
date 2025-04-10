use std::process::ExitCode;

use clap::Subcommand;
use crossterm::style::Stylize;
use eyre::Result;
use serde_json::json;

use super::OutputFormat;

const TELEMETRY_ENABLED_KEY: &str = "telemetry.enabled";

#[derive(Debug, PartialEq, Eq, Subcommand)]
pub enum TelemetrySubcommand {
    Enable,
    Disable,
    Status {
        /// Format of the output
        #[arg(long, short, value_enum, default_value_t)]
        format: OutputFormat,
    },
}

impl TelemetrySubcommand {
    pub async fn execute(&self) -> Result<ExitCode> {
        match self {
            TelemetrySubcommand::Enable => {
                fig_settings::settings::set_value(TELEMETRY_ENABLED_KEY, true)?;
                Ok(ExitCode::SUCCESS)
            },
            TelemetrySubcommand::Disable => {
                fig_settings::settings::set_value(TELEMETRY_ENABLED_KEY, false)?;
                Ok(ExitCode::SUCCESS)
            },
            TelemetrySubcommand::Status { format } => {
                let status = fig_settings::settings::get_bool_or(TELEMETRY_ENABLED_KEY, true);
                format.print(
                    || {
                        format!(
                            "Telemetry status: {}",
                            if status { "enabled" } else { "disabled" }.bold()
                        )
                    },
                    || {
                        json!({
                            TELEMETRY_ENABLED_KEY: status,
                        })
                    },
                );
                Ok(ExitCode::SUCCESS)
            },
        }
    }
}
