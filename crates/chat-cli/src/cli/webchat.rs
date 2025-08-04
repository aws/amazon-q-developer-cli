use std::process::ExitCode;

use clap::Args;
use eyre::Result;
use tracing::info;

use crate::os::Os;

#[derive(Debug, Clone, PartialEq, Eq, Default, Args)]
pub struct WebchatArgs {
    /// Port to run the web server on
    #[arg(short, long, default_value = "8080")]
    pub port: u16,
    /// Context profile to use
    #[arg(long = "agent", alias = "profile")]
    pub agent: Option<String>,
    /// Current model to use
    #[arg(long = "model")]
    pub model: Option<String>,
    /// Allows the model to use any tool to run commands without asking for confirmation.
    #[arg(short = 'a', long)]
    pub trust_all_tools: bool,
    /// Trust only this set of tools. Example: trust some tools:
    /// '--trust-tools=fs_read,fs_write', trust no tools: '--trust-tools='
    #[arg(long, value_delimiter = ',', value_name = "TOOL_NAMES")]
    pub trust_tools: Option<Vec<String>>,
}

impl WebchatArgs {
    pub async fn execute(self, _os: &mut Os) -> Result<ExitCode> {
        info!("Starting webchat on port {}", self.port);
        
        // Convert webchat args to chat args for compatibility
        let chat_args = vec![
            "q".to_string(),
            "chat".to_string(),
        ];
        
        // Start the web terminal server
        web_terminal::start_web_server(self.port, chat_args).await?;
        
        Ok(ExitCode::SUCCESS)
    }
}
