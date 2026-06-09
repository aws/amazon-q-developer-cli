use agent::goal::GoalDefinition;
use serde::{
    Deserialize,
    Serialize,
};

/// Maximum consecutive dispatch failures before pausing the goal.
const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// State machine managing the goal loop lifecycle. Owned by AcpSession.
#[derive(Debug, Clone)]
pub struct GoalController {
    pub definition: GoalDefinition,
    pub state: GoalState,
    pub iteration: u32,
    pub history: Vec<GoalIterationResult>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Consecutive turn failures (dispatch errors, 5xx). Reset on successful turn.
    pub consecutive_failures: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum GoalState {
    #[default]
    WaitingForTurn,
    Completed,
    Exhausted {
        reason: String,
    },
}

/// Serializable snapshot of an active goal — persisted to session state and
/// reconstructed on session reload via [`GoalController::from_snapshot`].
///
/// `#[serde(flatten)]` on `definition` keeps the on-disk JSON shape backward
/// compatible with previously persisted bare `GoalDefinition` values: missing
/// runtime fields default cleanly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalSnapshot {
    #[serde(flatten)]
    pub definition: GoalDefinition,
    #[serde(default)]
    pub iteration: u32,
    #[serde(default)]
    pub state: GoalState,
    #[serde(default = "default_started_at")]
    pub started_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub history: Vec<GoalIterationResult>,
}

fn default_started_at() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoalIterationResult {
    pub iteration: u32,
    pub passed: bool,
    pub feedback: Option<String>,
}

impl GoalController {
    pub fn new(definition: GoalDefinition) -> Self {
        Self {
            definition,
            state: GoalState::WaitingForTurn,
            iteration: 0,
            history: Vec::new(),
            started_at: chrono::Utc::now(),
            consecutive_failures: 0,
        }
    }

    /// Capture the full controller state for persistence.
    pub fn to_snapshot(&self) -> GoalSnapshot {
        GoalSnapshot {
            definition: self.definition.clone(),
            iteration: self.iteration,
            state: self.state.clone(),
            started_at: self.started_at,
            history: self.history.clone(),
        }
    }

    /// Reconstruct from a persisted snapshot. Preserves iteration counter,
    /// state, started_at, and history so the loop can resume mid-stream.
    pub fn from_snapshot(snapshot: GoalSnapshot) -> Self {
        Self {
            definition: snapshot.definition,
            state: snapshot.state,
            iteration: snapshot.iteration,
            history: snapshot.history,
            started_at: snapshot.started_at,
            consecutive_failures: 0,
        }
    }

    pub fn should_continue(&self) -> bool {
        self.state == GoalState::WaitingForTurn
    }

    /// Record a successful turn — resets the failure counter.
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
    }

    /// Record a dispatch failure. Returns `true` if the goal should retry
    /// (after a backoff delay), or `false` if retries are exhausted and the
    /// goal has been paused.
    pub fn record_failure(&mut self) -> bool {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
            self.mark_exhausted(format!(
                "Paused after {} consecutive dispatch failures",
                self.consecutive_failures
            ));
            false
        } else {
            true
        }
    }

    /// Exponential backoff delay for the current failure count: 2s, 4s, 8s.
    pub fn backoff_delay(&self) -> std::time::Duration {
        std::time::Duration::from_secs(1 << self.consecutive_failures)
    }

    /// Transition the goal into the [`GoalState::Exhausted`] state with the given
    /// reason. Used both when `iteration >= max_iterations` and when re-injection
    /// fails after retries — keeping the transition in one place ensures the
    /// controller never gets stranded in `WaitingForTurn`.
    pub fn mark_exhausted(&mut self, reason: impl Into<String>) {
        self.state = GoalState::Exhausted { reason: reason.into() };
    }

    pub fn status_display(&self) -> String {
        let state_icon = match &self.state {
            GoalState::WaitingForTurn => "⟳",
            GoalState::Completed => "✓",
            GoalState::Exhausted { .. } => "✗",
        };
        let state_str = match &self.state {
            GoalState::WaitingForTurn => "Active",
            GoalState::Completed => "Completed",
            GoalState::Exhausted { .. } => "Exhausted",
        };
        let mut s = format!(
            "● Goal: {}\n  State: {} {} [{}/{}]",
            self.definition.description,
            state_icon,
            state_str,
            self.iteration + 1,
            self.definition.max_iterations,
        );
        if !self.history.is_empty() {
            s.push_str(&format!("\n  History: {} iterations completed", self.history.len()));
        }
        s
    }

    /// Build the goal prompt for a given iteration, using `goal_prompt.md` as the template.
    pub fn build_prompt(&self, iteration: u32) -> String {
        include_str!("commands/goal_prompt.md")
            .replace("{iteration}", &iteration.to_string())
            .replace("{max}", &self.definition.max_iterations.to_string())
            .replace("{goal}", &self.definition.description)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_definition() -> GoalDefinition {
        GoalDefinition {
            description: "Implement pagination".into(),
            max_iterations: 3,
        }
    }

    #[test]
    fn new_controller_starts_waiting() {
        let ctrl = GoalController::new(make_definition());
        assert_eq!(ctrl.state, GoalState::WaitingForTurn);
        assert_eq!(ctrl.iteration, 0);
    }

    #[test]
    fn should_continue_when_waiting() {
        let ctrl = GoalController::new(make_definition());
        assert!(ctrl.should_continue());
    }

    #[test]
    fn should_not_continue_when_completed() {
        let mut ctrl = GoalController::new(make_definition());
        ctrl.state = GoalState::Completed;
        assert!(!ctrl.should_continue());
    }

    #[test]
    fn should_not_continue_when_exhausted() {
        let mut ctrl = GoalController::new(make_definition());
        ctrl.state = GoalState::Exhausted { reason: "max".into() };
        assert!(!ctrl.should_continue());
    }

    #[test]
    fn mark_exhausted_transitions_state_with_reason() {
        let mut ctrl = GoalController::new(make_definition());
        ctrl.mark_exhausted("Re-injection failed: NotIdle");
        assert_eq!(ctrl.state, GoalState::Exhausted {
            reason: "Re-injection failed: NotIdle".into()
        });
    }

    #[test]
    fn mark_exhausted_breaks_should_continue_loop() {
        // Regression test for the Knight Rider finding: after re-injection
        // exhausted its retries, the controller must stop reporting
        // should_continue=true so the UI chip clears.
        let mut ctrl = GoalController::new(make_definition());
        assert!(ctrl.should_continue(), "fresh controller should be active");
        ctrl.iteration = 2; // simulating mid-loop failure
        ctrl.mark_exhausted("Re-injection failed: NotIdle");
        assert!(
            !ctrl.should_continue(),
            "after mark_exhausted, should_continue must be false"
        );
    }

    #[test]
    fn snapshot_roundtrip_preserves_full_state() {
        let mut ctrl = GoalController::new(make_definition());
        ctrl.iteration = 3;
        ctrl.state = GoalState::Exhausted {
            reason: "Max iterations reached".into(),
        };
        ctrl.history.push(GoalIterationResult {
            iteration: 1,
            passed: false,
            feedback: Some("attempt 1 failed".into()),
        });

        let snapshot = ctrl.to_snapshot();
        let restored = GoalController::from_snapshot(snapshot);

        assert_eq!(restored.iteration, 3);
        assert_eq!(restored.state, GoalState::Exhausted {
            reason: "Max iterations reached".into()
        });
        assert_eq!(restored.history.len(), 1);
        assert_eq!(restored.definition.description, "Implement pagination");
    }

    #[test]
    fn snapshot_serializes_with_flattened_definition_for_backcompat() {
        // Old session JSON had only the GoalDefinition fields directly.
        // GoalSnapshot must deserialize that shape so resuming pre-snapshot
        // sessions still works.
        let legacy_json = r#"{"description":"resume me","max_iterations":5}"#;
        let snapshot: GoalSnapshot = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(snapshot.definition.description, "resume me");
        assert_eq!(snapshot.iteration, 0);
        assert_eq!(snapshot.state, GoalState::WaitingForTurn);
        assert!(snapshot.history.is_empty());
    }

    #[test]
    fn snapshot_serializes_full_state_for_new_sessions() {
        let snapshot = GoalSnapshot {
            definition: make_definition(),
            iteration: 4,
            state: GoalState::Completed,
            started_at: chrono::Utc::now(),
            history: vec![],
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        // Definition fields are flattened to the top level
        assert!(json.contains("\"description\":\"Implement pagination\""));
        // Runtime fields are persisted
        assert!(json.contains("\"iteration\":4"));
        assert!(json.contains("\"state\":\"Completed\""));
    }

    #[test]
    fn status_display_shows_state() {
        let ctrl = GoalController::new(make_definition());
        let display = ctrl.status_display();
        assert!(display.contains("⟳ Active"));
        assert!(display.contains("Implement pagination"));
    }

    #[test]
    fn record_success_resets_failure_counter() {
        let mut ctrl = GoalController::new(make_definition());
        ctrl.consecutive_failures = 2;
        ctrl.record_success();
        assert_eq!(ctrl.consecutive_failures, 0);
    }

    #[test]
    fn record_failure_allows_retry_below_max() {
        let mut ctrl = GoalController::new(make_definition());
        assert!(ctrl.record_failure()); // 1st failure — retry
        assert!(ctrl.record_failure()); // 2nd failure — retry
        assert!(ctrl.should_continue());
    }

    #[test]
    fn record_failure_pauses_at_max() {
        let mut ctrl = GoalController::new(make_definition());
        ctrl.record_failure();
        ctrl.record_failure();
        let should_retry = ctrl.record_failure(); // 3rd — exhausted
        assert!(!should_retry);
        assert!(!ctrl.should_continue());
        assert!(matches!(ctrl.state, GoalState::Exhausted { .. }));
    }

    #[test]
    fn backoff_delay_grows_exponentially() {
        let mut ctrl = GoalController::new(make_definition());
        ctrl.consecutive_failures = 1;
        assert_eq!(ctrl.backoff_delay().as_secs(), 2);
        ctrl.consecutive_failures = 2;
        assert_eq!(ctrl.backoff_delay().as_secs(), 4);
        ctrl.consecutive_failures = 3;
        assert_eq!(ctrl.backoff_delay().as_secs(), 8);
    }

    #[test]
    fn success_after_failures_resets_and_allows_continuation() {
        let mut ctrl = GoalController::new(make_definition());
        ctrl.record_failure();
        ctrl.record_failure();
        // At 2 failures but not paused yet
        assert!(ctrl.should_continue());
        ctrl.record_success();
        assert_eq!(ctrl.consecutive_failures, 0);
        // Can now tolerate another 3 failures
        assert!(ctrl.record_failure());
        assert!(ctrl.record_failure());
    }
}
