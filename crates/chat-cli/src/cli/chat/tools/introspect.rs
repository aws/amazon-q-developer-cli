use std::io::Write;

use eyre::Result;
use serde::{Deserialize, Serialize};
use clap::CommandFactory;

use super::{InvokeOutput, OutputKind};
use crate::os::Os;
use crate::cli::chat::cli::SlashCommand;

#[derive(Debug, Clone, Deserialize)]
pub struct Introspect {
    #[serde(default)]
    query: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct IntrospectResponse {
    built_in_help: Option<String>,
    documentation: Option<String>,
    query_context: Option<String>,
    recommendations: Vec<ToolRecommendation>,
}

#[derive(Debug, Serialize)]
pub struct ToolRecommendation {
    tool_name: String,
    description: String,
    use_case: String,
    example: Option<String>,
}

impl Introspect {
    pub async fn invoke(&self, _os: &Os, _updates: impl Write) -> Result<InvokeOutput> {
        // Generate help from the actual SlashCommand definitions
        let mut cmd = SlashCommand::command();
        let help_content = cmd.render_help().to_string();

        // Embed documentation at compile time
        let mut documentation = String::new();
        
        documentation.push_str("\n\n--- README.md ---\n");
        documentation.push_str(include_str!("../../../../../../README.md"));
        
        documentation.push_str("\n\n--- docs/built-in-tools.md ---\n");
        documentation.push_str(include_str!("../../../../../../docs/built-in-tools.md"));
        
        documentation.push_str("\n\n--- CONTRIBUTING.md ---\n");
        documentation.push_str(include_str!("../../../../../../CONTRIBUTING.md"));

        let response = IntrospectResponse {
            built_in_help: Some(help_content),
            documentation: Some(documentation),
            query_context: self.query.clone(),
            recommendations: vec![],
        };
        
        Ok(InvokeOutput {
            output: OutputKind::Json(serde_json::to_value(&response)?),
        })
    }

    pub fn queue_description(&self, output: &mut impl Write) -> Result<()> {
        use crossterm::{queue, style};
        queue!(
            output,
            style::Print("Introspecting to get you the right information")
        )?;
        Ok(())
    }

    pub async fn validate(&self, _os: &Os) -> Result<()> {
        Ok(())
    }
}
