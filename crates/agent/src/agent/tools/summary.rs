use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::{
    broadcast,
    mpsc,
};

use super::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionResult,
};
use crate::agent_config::parse::CanonicalToolName;
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
    /// Result disposition: "terminal" (default) means the task is done;
    /// "changes_needed" signals that the target stage should re-run with this feedback.
    /// Only meaningful in crew pipelines with loop_to configured.
    #[serde(default)]
    pub result_type: Option<String>,
}

const SUMMARY_TOOL_DESCRIPTION: &str = r#"
MANDATORY tool for conveying task results from subagent to main agent. You MUST call this tool before ending your turn — do NOT end with a plain text response.

WHEN TO USE THIS TOOL: 
- ALWAYS call this tool when your task is done, before your turn ends
- This is the ONLY way to deliver results back to the main agent

HOW TO USE:
- Provide the description of the task given
- Optionally provide any context summary that compliments the consumer of the results. This is to aid subsequent actions to be performed with the result being sent
- Provide the result of the task performed
- If you are in a crew pipeline with a loop configured and you want the target stage to re-run with your feedback, set resultType to "changes_needed". Otherwise leave it unset or set to "terminal".
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
        },
        "resultType": {
            "type": "string",
            "enum": ["terminal", "changes_needed"],
            "description": "Result disposition. Use 'changes_needed' in crew pipelines to signal the target stage should re-run with your feedback. Defaults to 'terminal'."
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
    /// Deliver the summary to the parent. `summary_tx` is the lossless source of
    /// truth; `result_tx` is a best-effort broadcast for UI and is safe to drop.
    pub async fn execute(
        &self,
        summary_tx: mpsc::UnboundedSender<Summary>,
        result_tx: broadcast::Sender<AgentEvent>,
    ) -> ToolExecutionResult {
        // Best-effort UI broadcast; safe to drop.
        let _ = result_tx.send(self.into());
        // Lossless delivery to the waiting parent. This is the one that matters.
        summary_tx
            .send(self.clone())
            .map(|_res| ToolExecutionOutput::default())
            .map_err(|e| ToolExecutionError::Custom(e.to_string()))
    }

    pub fn get_canonical_name() -> CanonicalToolName {
        CanonicalToolName::BuiltIn(BuiltInToolName::Summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_tool_schema() {
        let schema = Summary::input_schema();
        println!("{schema:#?}");
    }

    #[tokio::test]
    async fn test_summary_tool_execute() {
        let (summary_tx, mut summary_rx) = mpsc::unbounded_channel();
        let (tx, mut rx) = broadcast::channel(10);
        let summary = Summary {
            task_description: "test task".to_string(),
            context_summary: Some("test context".to_string()),
            task_result: "test result".to_string(),
            result_type: None,
        };
        let result = summary.execute(summary_tx, tx).await;
        assert!(result.is_ok());

        // Lossless channel carries the typed Summary directly.
        let delivered = summary_rx.recv().await.unwrap();
        assert_eq!(delivered.task_description, "test task");
        assert_eq!(delivered.context_summary, Some("test context".to_string()));
        assert_eq!(delivered.task_result, "test result");

        // Broadcast still carries the SubagentSummary event for UI consumers.
        let event = rx.recv().await.unwrap();
        if let AgentEvent::SubagentSummary(Summary {
            task_description,
            context_summary,
            task_result,
            ..
        }) = event
        {
            assert_eq!(task_description, "test task");
            assert_eq!(context_summary, Some("test context".to_string()));
            assert_eq!(task_result, "test result");
        } else {
            panic!("Expected AgentEvent::Summary");
        }
    }

    /// Pins the hazard: when the broadcast overflows, a `SubagentSummary` sent
    /// first is evicted and lost — the reason delivery can't rely on it alone.
    #[tokio::test]
    async fn test_broadcast_drops_summary_event_when_lagged() {
        let (tx, mut rx) = broadcast::channel::<AgentEvent>(4);

        let summary = Summary {
            task_description: "count LOC".to_string(),
            context_summary: None,
            task_result: "42 LOC".to_string(),
            result_type: None,
        };

        // Summary is broadcast first, then a chatty turn overflows the buffer
        // before the consumer reads, evicting the older summary event.
        let _ = tx.send((&summary).into());
        for _ in 0..16 {
            let _ = tx.send(AgentEvent::Initialized);
        }

        let mut saw_summary = false;
        let mut saw_lag = false;
        loop {
            match rx.try_recv() {
                Ok(AgentEvent::SubagentSummary(_)) => saw_summary = true,
                Ok(_) => {},
                Err(broadcast::error::TryRecvError::Lagged(_)) => saw_lag = true,
                Err(broadcast::error::TryRecvError::Empty) | Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }

        assert!(saw_lag, "expected the broadcast to report Lagged after overflow");
        assert!(
            !saw_summary,
            "broadcast lag should have dropped the SubagentSummary event"
        );
    }

    /// The fix: with the broadcast saturated and never read, `execute` still
    /// delivers the summary over the lossless channel.
    #[tokio::test]
    async fn test_lossless_channel_survives_broadcast_overload() {
        let (summary_tx, mut summary_rx) = mpsc::unbounded_channel();
        // Tiny broadcast, deliberately never drained, to simulate a saturated
        // UI event stream under a verbose turn.
        let (tx, _rx) = broadcast::channel::<AgentEvent>(2);
        for _ in 0..32 {
            let _ = tx.send(AgentEvent::Initialized);
        }

        let summary = Summary {
            task_description: "analyze".to_string(),
            context_summary: None,
            task_result: "the long result that must not be lost".to_string(),
            result_type: None,
        };
        summary.execute(summary_tx, tx).await.unwrap();

        let delivered = summary_rx.recv().await.expect("summary must arrive losslessly");
        assert_eq!(delivered.task_result, "the long result that must not be lost");
    }
}
