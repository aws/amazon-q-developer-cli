//! Per-prompt tool-call budget.
//!
//! The agent loop can otherwise spiral on a hard question — too many tool
//! calls mean slow responses and unbounded Bedrock spend. Each prompt gets a
//! fresh `ToolBudget`; once the configured cap (default: 12 calls) is
//! exhausted, the engine aborts the prompt with a polite error.

use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};

/// Default per-prompt tool-call cap. Sized to allow a search → refine → cite
/// loop with a couple of follow-up clarifications, but cut off runaway agents
/// well before they hit Bedrock retry storms.
pub const DEFAULT_MAX_CALLS: usize = 12;

/// Tracks tool calls inside one prompt's run.
#[derive(Debug)]
pub struct ToolBudget {
    used: AtomicUsize,
    max: usize,
}

/// Outcome of `ToolBudget::charge`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetState {
    /// Call counted; calls still available.
    Within { used: usize, max: usize },
    /// Last available call just ran; subsequent calls will be rejected.
    Exhausted { used: usize, max: usize },
    /// Call rejected because the budget is exhausted.
    Over { used: usize, max: usize },
}

impl BudgetState {
    pub fn is_over(&self) -> bool {
        matches!(self, BudgetState::Over { .. })
    }
}

impl ToolBudget {
    pub fn new(max: usize) -> Self {
        Self {
            used: AtomicUsize::new(0),
            max,
        }
    }

    pub fn default_budget() -> Self {
        Self::new(DEFAULT_MAX_CALLS)
    }

    pub fn used(&self) -> usize {
        self.used.load(Ordering::SeqCst)
    }

    pub fn max(&self) -> usize {
        self.max
    }

    /// Increment the counter. Returns:
    ///
    /// - `Within { used, max }` for any call up to `max - 1`.
    /// - `Exhausted { used, max }` for the call that brings used to `max`.
    /// - `Over { used, max }` for any call after `max`. The counter is NOT incremented past `max`,
    ///   so repeated rejected calls all surface the same `used = max` value.
    pub fn charge(&self) -> BudgetState {
        let prev = self.used.fetch_add(1, Ordering::SeqCst);
        let used = prev + 1;
        if used < self.max {
            BudgetState::Within { used, max: self.max }
        } else if used == self.max {
            BudgetState::Exhausted { used, max: self.max }
        } else {
            // Roll back the increment — Over should not move `used` further.
            self.used.fetch_sub(1, Ordering::SeqCst);
            BudgetState::Over {
                used: self.max,
                max: self.max,
            }
        }
    }

    /// Convenience: returns `Err(message)` when the budget is over, otherwise
    /// `Ok(state)`. Message is suitable for surfacing to the user.
    pub fn charge_or_err(&self) -> Result<BudgetState, String> {
        match self.charge() {
            BudgetState::Over { used: _, max } => Err(format!(
                "Hit the per-prompt tool-call budget ({max} calls). Try a more specific question."
            )),
            other => Ok(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_budget_is_twelve_calls() {
        assert_eq!(ToolBudget::default_budget().max(), DEFAULT_MAX_CALLS);
    }

    #[test]
    fn charge_progresses_within_then_exhausted_then_over() {
        let budget = ToolBudget::new(3);
        assert_eq!(budget.charge(), BudgetState::Within { used: 1, max: 3 });
        assert_eq!(budget.charge(), BudgetState::Within { used: 2, max: 3 });
        assert_eq!(budget.charge(), BudgetState::Exhausted { used: 3, max: 3 });
        assert_eq!(budget.charge(), BudgetState::Over { used: 3, max: 3 });
        assert_eq!(budget.charge(), BudgetState::Over { used: 3, max: 3 });
        assert_eq!(budget.used(), 3, "over-budget calls must not bump used");
    }

    #[test]
    fn budget_zero_rejects_first_call() {
        let budget = ToolBudget::new(0);
        // First call: prev=0, used=1, max=0 → used > max → Over.
        let state = budget.charge();
        assert!(state.is_over(), "max=0 must reject the first call: {state:?}");
    }

    #[test]
    fn charge_or_err_returns_err_on_overflow() {
        let budget = ToolBudget::new(1);
        assert!(budget.charge_or_err().is_ok());
        let err = budget.charge_or_err().unwrap_err();
        assert!(err.contains("budget"));
        assert!(err.contains('1'));
    }

    #[tokio::test]
    async fn budget_is_safely_concurrent() {
        let budget = std::sync::Arc::new(ToolBudget::new(50));
        let mut handles = Vec::new();
        for _ in 0..100 {
            let b = budget.clone();
            handles.push(tokio::spawn(async move { b.charge() }));
        }
        let mut over = 0;
        let mut ok = 0;
        for h in handles {
            match h.await.unwrap() {
                BudgetState::Over { .. } => over += 1,
                _ => ok += 1,
            }
        }
        assert_eq!(ok, 50);
        assert_eq!(over, 50);
        assert_eq!(budget.used(), 50, "no race conditions on used count");
    }
}
