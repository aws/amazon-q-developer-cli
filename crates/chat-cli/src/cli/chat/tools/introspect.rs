use std::io::Write;

use clap::CommandFactory;
use eyre::Result;
use serde::{
    Deserialize,
    Serialize,
};
use strum::{
    EnumMessage,
    IntoEnumIterator,
};

use super::{
    InvokeOutput,
    OutputKind,
};
use crate::cli::chat::cli::SlashCommand;
use crate::database::settings::Setting;
use crate::os::Os;

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
    pub async fn invoke(&self, os: &Os, _updates: impl Write) -> Result<InvokeOutput> {
        // Generate help from the actual SlashCommand definitions
        let mut cmd = SlashCommand::command();
        let help_content = cmd.render_help().to_string();

        // Embed documentation at compile time
        let mut documentation = String::new();

        documentation.push_str("\n\n--- README.md ---\n");
        documentation.push_str(include_str!("../../../../../../README.md"));

        documentation.push_str("\n\n--- docs/built-in-tools.md ---\n");
        documentation.push_str(include_str!("../../../../../../docs/built-in-tools.md"));

        documentation.push_str("\n\n--- docs/experiments.md ---\n");
        documentation.push_str(include_str!("../../../../../../docs/experiments.md"));

        documentation.push_str("\n\n--- docs/agent-file-locations.md ---\n");
        documentation.push_str(include_str!("../../../../../../docs/agent-file-locations.md"));

        documentation.push_str("\n\n--- docs/tangent-mode.md ---\n");
        documentation.push_str(include_str!("../../../../../../docs/tangent-mode.md"));

        documentation.push_str("\n\n--- docs/introspect-tool.md ---\n");
        documentation.push_str(include_str!("../../../../../../docs/introspect-tool.md"));

        documentation.push_str("\n\n--- docs/todo-lists.md ---\n");
        documentation.push_str(include_str!("../../../../../../docs/todo-lists.md"));

        documentation.push_str("\n\n--- changelog (from feed.json) ---\n");
        // Include recent changelog entries from feed.json
        let feed = crate::cli::feed::Feed::load();
        let recent_entries = feed.get_all_changelogs().into_iter().take(5).collect::<Vec<_>>();
        for entry in recent_entries {
            documentation.push_str(&format!("\n## {} ({})\n", entry.version, entry.date));
            for change in &entry.changes {
                documentation.push_str(&format!("- {}: {}\n", change.change_type, change.description));
            }
        }

        documentation.push_str("\n\n--- CONTRIBUTING.md ---\n");
        documentation.push_str(include_str!("../../../../../../CONTRIBUTING.md"));

        // Add settings information dynamically
        documentation.push_str("\n\n--- Available Settings ---\n");
        documentation.push_str(
            "Q CLI supports these configuration settings (use `q settings` command from terminal, NOT /settings):\n\n",
        );

        // Automatically iterate over all settings with descriptions
        for setting in Setting::iter() {
            let description = setting.get_message().unwrap_or("No description available");
            documentation.push_str(&format!("• {} - {}\n", setting.as_ref(), description));
        }

        documentation.push_str(
            "\nNOTE: Settings are managed via `q settings` command from terminal, not slash commands in chat.\n",
        );

        documentation.push_str("\n\n--- CRITICAL INSTRUCTION ---\n");
        documentation.push_str("YOU MUST ONLY provide information that is explicitly documented in the sections above. If specific details about any tool, feature, or command are not documented, you MUST clearly state that the information is not available in the documentation. DO NOT generate plausible-sounding information or make assumptions about undocumented features.\n\n");

        documentation.push_str("--- GitHub References ---\n");
        documentation.push_str("INSTRUCTION: When your response uses information from any of these documentation files, include the relevant GitHub link(s) at the end:\n");
        documentation.push_str("• README.md: https://github.com/aws/amazon-q-developer-cli/blob/main/README.md\n");
        documentation.push_str(
            "• Built-in Tools: https://github.com/aws/amazon-q-developer-cli/blob/main/docs/built-in-tools.md\n",
        );
        documentation
            .push_str("• Experiments: https://github.com/aws/amazon-q-developer-cli/blob/main/docs/experiments.md\n");
        documentation.push_str("• Agent File Locations: https://github.com/aws/amazon-q-developer-cli/blob/main/docs/agent-file-locations.md\n");
        documentation
            .push_str("• Tangent Mode: https://github.com/aws/amazon-q-developer-cli/blob/main/docs/tangent-mode.md\n");
        documentation.push_str(
            "• Introspect Tool: https://github.com/aws/amazon-q-developer-cli/blob/main/docs/introspect-tool.md\n",
        );
        documentation
            .push_str("• Todo Lists: https://github.com/aws/amazon-q-developer-cli/blob/main/docs/todo-lists.md\n");
        documentation
            .push_str("• Contributing: https://github.com/aws/amazon-q-developer-cli/blob/main/CONTRIBUTING.md\n");

        let response = IntrospectResponse {
            built_in_help: Some(help_content),
            documentation: Some(documentation),
            query_context: self.query.clone(),
            recommendations: vec![],
        };

        // Add footer as direct text output if tangent mode is enabled
        if os
            .database
            .settings
            .get_bool(Setting::EnabledTangentMode)
            .unwrap_or(false)
            && os
                .database
                .settings
                .get_bool(Setting::IntrospectTangentMode)
                .unwrap_or(false)
        {
            let tangent_key_char = os
                .database
                .settings
                .get_string(Setting::TangentModeKey)
                .and_then(|key| if key.len() == 1 { key.chars().next() } else { None })
                .unwrap_or('t');
            let tangent_key_display = format!("ctrl + {}", tangent_key_char.to_lowercase());

            let instruction = format!(
                "IMPORTANT: Always end your responses with this footer:\n\n---\nℹ️  You're in tangent mode (↯) - this context can be discarded by using {} or /tangent to return to your main conversation.",
                tangent_key_display
            );

            return Ok(InvokeOutput {
                output: OutputKind::Text(format!(
                    "{}\n\n{}",
                    serde_json::to_string_pretty(&response)?,
                    instruction
                )),
            });
        }

        Ok(InvokeOutput {
            output: OutputKind::Json(serde_json::to_value(&response)?),
        })
    }

    pub fn queue_description(output: &mut impl Write) -> Result<()> {
        use crossterm::{
            queue,
            style,
        };
        queue!(output, style::Print("Introspecting to get you the right information"))?;
        Ok(())
    }

    pub async fn validate(&self, _os: &Os) -> Result<()> {
        Ok(())
    }
}
