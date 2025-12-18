use std::io::{
    Write,
    stdin,
};

use crossterm::{
    cursor,
    execute,
    style,
};
use eyre::Result;
use serde::{
    Deserialize,
    Serialize,
};

use super::{
    InvokeOutput,
    OutputKind,
    ToolInfo,
};
use crate::theme::StyledText;

#[derive(Debug, Clone, Deserialize)]
pub struct SwitchToExecution {
    pub plan: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SwitchResponse {
    approved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    plan: String,
}

impl SwitchToExecution {
    pub const INFO: ToolInfo = ToolInfo {
        spec_name: "switch_to_execution",
        preferred_alias: "switch_to_execution",
        aliases: &["switch_to_execution"],
    };

    pub async fn invoke(&self, _stdout: &mut impl Write) -> Result<InvokeOutput> {
        execute!(
            std::io::stdout(),
            StyledText::secondary_fg(),
            style::Print("\nPlanning complete!\nReady to exit "),
            style::Print(StyledText::brand("[plan]")),
            StyledText::secondary_fg(),
            style::Print(" agent to start your implementation? ["),
            StyledText::current_item_fg(),
            style::Print("y"),
            StyledText::secondary_fg(),
            style::Print("/"),
            StyledText::current_item_fg(),
            style::Print("n"),
            StyledText::secondary_fg(),
            style::Print("]:\n\n"),
            StyledText::reset(),
            cursor::Show,
        )?;

        execute!(std::io::stdout(), style::Print("> "),)?;
        std::io::stdout().flush()?;

        let mut input = String::new();
        stdin().read_line(&mut input)?;
        let response = input.trim().to_lowercase();

        match response.as_str() {
            "y" | "yes" => Ok(InvokeOutput {
                output: OutputKind::Json(serde_json::to_value(SwitchResponse {
                    approved: true,
                    message: None,
                    plan: self.plan.clone(),
                })?),
            }),
            _ => Ok(InvokeOutput {
                output: OutputKind::Json(serde_json::to_value(SwitchResponse {
                    approved: false,
                    message: Some("User wants to continue planning. Ask the user what changes or additions they'd like to make to the plan.".to_string()),
                    plan: self.plan.clone(),
                })?),
            }),
        }
    }

    pub async fn post_process(
        &self,
        result: &super::InvokeOutput,
        session: &mut crate::cli::chat::ChatSession,
        _os: &crate::os::Os,
    ) -> eyre::Result<()> {
        if let super::OutputKind::Json(json) = &result.output {
            let response: SwitchResponse = serde_json::from_value(json.clone())?;
            if response.approved {
                let prompt = format!("Implement this plan:\n{}", response.plan);
                session.input_source.agent_swap_state().planner_toggle(Some(prompt));
            }
        }
        Ok(())
    }
}
