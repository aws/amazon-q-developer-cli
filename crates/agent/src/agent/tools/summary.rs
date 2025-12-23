use schemars::{
    JsonSchema,
    schema_for,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::broadcast;

use super::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionResult,
};
use crate::protocol::AgentEvent;

/// A tool for conveying information from subagent to its main agent
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// Description of the task that was assigned to the subagent
    pub task_description: String,
    /// Relevant context and information gathered during task execution
    pub context_summary: Option<String>,
    /// The final result or outcome of the completed task
    pub task_result: String,
}

const SUMMARY_TOOL_DESCRIPTION: &str = r#"
A tool for conveying task summary and results from subagent to main agent. 

WHEN TO USE THIS TOOL: 
- As a subagent, when a task is completed, use this tool to send the findings / conclusions to the main agent

HOW TO USE:
- Provide the description of the task given
- Optionally provide any context summary that compliments the consumer of the results. This is to aid subsequent actions to be performed with the result being sent
- Provide the result of the task performed
"#;

const SUMMARY_TOOL_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "taskDescription": {
            "type": "string",
            "description": "Description of the task that was assigned to the subagent"
        },
        "contextSummary": {
            "type": "string",
            "description": "Relevant context and information gathered during task execution"
        },
        "taskResult": {
            "type": "string",
            "description": "The final result or outcome of the completed task"
        }
    },
    "required": [
        "taskDescription",
        "taskResult"
    ]
}
"#;

impl BuiltInToolTrait for Summary {
    fn name() -> super::BuiltInToolName {
        BuiltInToolName::Summary
    }

    fn description() -> std::borrow::Cow<'static, str> {
        SUMMARY_TOOL_DESCRIPTION.into()
    }

    fn input_schema() -> std::borrow::Cow<'static, str> {
        SUMMARY_TOOL_SCHEMA.into()
    }
}

impl Summary {
    pub fn tool_schema() -> serde_json::Value {
        let schema = schema_for!(Self);
        serde_json::to_value(schema).expect("creating tool schema should not fail")
    }

    pub async fn execute(&self, result_tx: broadcast::Sender<AgentEvent>) -> ToolExecutionResult {
        result_tx
            .send(self.into())
            .map(|_res| ToolExecutionOutput::default())
            .map_err(|e| ToolExecutionError::Custom(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_tool_schema() {
        let schema = Summary::input_schema();
        println!("{:#?}", schema);
    }

    #[tokio::test]
    async fn test_summary_tool_execute() {
        let (tx, mut rx) = broadcast::channel(10);
        let summary = Summary {
            task_description: "test task".to_string(),
            context_summary: Some("test context".to_string()),
            task_result: "test result".to_string(),
        };
        let result = summary.execute(tx).await;
        assert!(result.is_ok());

        let event = rx.recv().await.unwrap();

        if let AgentEvent::SubagentSummary(Summary {
            task_description,
            context_summary,
            task_result,
        }) = event
        {
            assert_eq!(task_description, "test task");
            assert_eq!(context_summary, Some("test context".to_string()));
            assert_eq!(task_result, "test result");
        } else {
            panic!("Expected AgentEvent::Summary");
        }
    }
}
