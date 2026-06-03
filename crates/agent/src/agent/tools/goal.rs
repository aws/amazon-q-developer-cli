use schemars::JsonSchema;
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
    ToolExecutionOutputItem,
    ToolExecutionResult,
};
use crate::protocol::AgentEvent;

/// Built-in `goal` tool. Lets the agent signal goal completion or check progress.
/// Only the user can create or clear goals (via the `/goal` slash command).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum GoalTool {
    /// Mark the current goal as complete — only after verifying success criteria
    Complete {
        /// Brief summary of what was accomplished and how it was verified
        summary: String,
    },
}

const DESCRIPTION: &str = "\
Signal goal completion. A goal is a binding completion contract — \
the system will continue re-prompting you until the contract is satisfied.

Commands:
- complete: Certify that ALL success criteria defined in the goal have been met, verified \
by concrete evidence you produced during this session (test output, file contents, command \
results). Your summary must cite the specific evidence that satisfies each criterion. \
A completion without cited verification will not satisfy the contract.

COMPLETION STANDARD (treat as binding):
1. Each criterion in the goal MUST be individually verified by tool output you can cite.
2. Belief, assumption, or narrative confidence is NOT evidence. Only tool results count.
3. If ANY criterion lacks verifiable evidence, you MUST NOT call complete. Continue working.
4. If a verification path is blocked, try alternative approaches before giving up. \
Only after exhausting alternatives should you end your turn without calling complete — \
the system will re-prompt you on the next iteration with a chance to try differently.
5. If across multiple iterations you identify a genuinely impossible impediment \
(same hard blocker confirmed repeatedly despite different approaches), you MAY call \
complete with a summary that documents: what was accomplished, what is blocked, \
the evidence of the blocker, and what would unblock it. This is the ONLY path to \
early completion without full success — and requires cited evidence of the blocker itself.

STRATEGY:
- Divide complex goals into independent sub-tasks. Use sub-agents to parallelize work \
when the goal has separable components (e.g., implement + test, frontend + backend).
- When available, delegate verification to a sub-agent acting as an independent auditor. \
An external reviewer that inspects the work product without sharing your context produces \
stronger evidence of completion than self-assessment.
- Prefer divide-and-conquer: decompose, delegate, verify independently, then synthesize.";

const SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "command": {
            "type": "string",
            "enum": ["complete", "status"],
            "description": "The goal action to perform"
        },
        "summary": {
            "type": "string",
            "description": "Brief summary of what was accomplished and how it was verified (required for 'complete')"
        }
    },
    "required": ["command"]
}"#;

impl BuiltInToolTrait for GoalTool {
    fn name() -> BuiltInToolName {
        BuiltInToolName::Goal
    }

    fn description() -> std::borrow::Cow<'static, str> {
        DESCRIPTION.into()
    }

    fn input_schema() -> std::borrow::Cow<'static, str> {
        SCHEMA.into()
    }
}

impl GoalTool {
    pub async fn execute(&self, result_tx: broadcast::Sender<AgentEvent>) -> ToolExecutionResult {
        let output = match self {
            GoalTool::Complete { .. } => "✓ Goal complete".into(),
        };
        result_tx
            .send(AgentEvent::GoalAction(self.clone()))
            .map(|_| ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(output)]))
            .map_err(|e| ToolExecutionError::Custom(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_complete() {
        let json = serde_json::json!({"command": "complete", "summary": "done"});
        let tool: GoalTool = serde_json::from_value(json).unwrap();
        assert!(matches!(tool, GoalTool::Complete { summary } if summary == "done"));
    }

    #[test]
    fn parse_invalid_command_fails() {
        let json = serde_json::json!({"command": "invalid"});
        assert!(serde_json::from_value::<GoalTool>(json).is_err());
    }

    #[test]
    fn parse_missing_command_fails() {
        let json = serde_json::json!({"summary": "done"});
        assert!(serde_json::from_value::<GoalTool>(json).is_err());
    }

    #[tokio::test]
    async fn execute_emits_goal_action_event() {
        let (tx, mut rx) = broadcast::channel(10);
        let tool = GoalTool::Complete {
            summary: "all done".into(),
        };
        let result = tool.execute(tx).await;
        assert!(result.is_ok());

        let event = rx.recv().await.unwrap();
        assert!(matches!(event, AgentEvent::GoalAction(GoalTool::Complete { summary }) if summary == "all done"));
    }

    #[tokio::test]
    async fn execute_returns_text_output() {
        let (tx, _rx) = broadcast::channel(10);
        let tool = GoalTool::Complete {
            summary: "all done".into(),
        };
        let result = tool.execute(tx).await.unwrap();
        assert_eq!(result.items.len(), 1);
        assert!(matches!(&result.items[0], ToolExecutionOutputItem::Text(t) if t.contains("Goal complete")));
    }
}
