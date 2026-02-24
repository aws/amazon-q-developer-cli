use std::borrow::Cow;

use serde::{
    Deserialize,
    Serialize,
};

use super::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionOutput,
    ToolExecutionOutputItem,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchToExecution {
    pub plan: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SwitchToExecutionResult {
    pub approved: bool,
    pub plan: String,
}

impl BuiltInToolTrait for SwitchToExecution {
    fn name() -> BuiltInToolName {
        BuiltInToolName::SwitchToExecution
    }

    fn description() -> Cow<'static, str> {
        Cow::Borrowed(
            "Signal that planning is complete and hand off the plan to the execution agent. \
             Only call this after the user has confirmed the plan looks good.",
        )
    }

    fn input_schema() -> Cow<'static, str> {
        Cow::Borrowed(
            r#"{
            "type": "object",
            "properties": {
                "plan": {
                    "type": "string",
                    "description": "The complete implementation plan to pass to the execution agent."
                }
            },
            "required": ["plan"]
        }"#,
        )
    }

    fn aliases() -> Option<&'static [&'static str]> {
        Some(&["switch_to_execution"])
    }
}

impl SwitchToExecution {
    pub fn execute(&self) -> ToolExecutionOutput {
        let result = SwitchToExecutionResult {
            approved: true,
            plan: self.plan.clone(),
        };
        ToolExecutionOutput {
            items: vec![ToolExecutionOutputItem::Text(
                serde_json::to_string(&result).unwrap_or_default(),
            )],
        }
    }
}
