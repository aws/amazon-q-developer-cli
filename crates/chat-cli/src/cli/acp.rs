use std::process::ExitCode;

use clap::Parser;
use eyre::Result;

use crate::database::settings::Setting;
use crate::os::Os;

#[derive(Debug, Parser, PartialEq)]
pub struct AcpArgs {
    /// Agent to use for ACP sessions
    #[arg(long)]
    pub agent: Option<String>,
}

impl AcpArgs {
    pub async fn run(self, os: &mut Os) -> Result<ExitCode> {
        // Check feature flag
        if !os.database.settings.get_bool(Setting::EnabledAcp).unwrap_or(false) {
            eprintln!("ACP is disabled. Enable with: q settings acp.enabled true");
            return Ok(ExitCode::FAILURE);
        }

        // For now, just print status and exit
        println!("ACP server starting...");
        println!("Agent: {:?}", self.agent.unwrap_or_else(|| "default".to_string()));
        println!("ACP server functionality not yet implemented");
        
        Ok(ExitCode::SUCCESS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_acp_command_disabled() {
        let mut os = Os::new().await.unwrap();
        
        // Explicitly disable the feature flag
        os.database.settings.set(Setting::EnabledAcp, false).await.unwrap();
        
        let args = AcpArgs { agent: None };
        let result = args.run(&mut os).await.unwrap();
        assert_eq!(result, ExitCode::FAILURE);
    }

    #[tokio::test]
    async fn test_acp_command_enabled() {
        let mut os = Os::new().await.unwrap();
        
        // Enable the feature flag
        os.database.settings.set(Setting::EnabledAcp, true).await.unwrap();
        
        let args = AcpArgs { agent: None };
        let result = args.run(&mut os).await.unwrap();
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[tokio::test]
    async fn test_acp_command_with_agent() {
        let mut os = Os::new().await.unwrap();
        
        // Enable the feature flag
        os.database.settings.set(Setting::EnabledAcp, true).await.unwrap();
        
        let args = AcpArgs { agent: Some("my-agent".to_string()) };
        let result = args.run(&mut os).await.unwrap();
        assert_eq!(result, ExitCode::SUCCESS);
    }
}
