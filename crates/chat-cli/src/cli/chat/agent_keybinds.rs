use eyre::Result;
use rustyline::history::FileHistory;
use rustyline::{
    Cmd,
    Editor,
    EventHandler,
    KeyCode,
    KeyEvent,
    Modifiers,
};

use super::agent_swap::AgentSwapState;

/// Handler for Shift+Tab planner toggle
pub struct PlannerToggleHandler {
    swap_state: AgentSwapState,
}

impl PlannerToggleHandler {
    pub fn new(swap_state: AgentSwapState) -> Self {
        Self { swap_state }
    }
}

impl rustyline::ConditionalEventHandler for PlannerToggleHandler {
    fn handle(
        &self,
        _evt: &rustyline::Event,
        _n: rustyline::RepeatCount,
        _positive: bool,
        _ctx: &rustyline::EventContext<'_>,
    ) -> Option<Cmd> {
        let target_agent = self.swap_state.toggle();
        self.swap_state.set_pending_swap(target_agent);
        Some(Cmd::AcceptLine)
    }
}

/// Binds agent-related keyboard shortcuts
pub fn bind_agent_shortcuts<H: rustyline::Helper>(
    rl: &mut Editor<H, FileHistory>,
    agents: &crate::cli::agent::Agents,
    swap_state: &AgentSwapState,
) -> Result<()> {
    swap_state.set_current_agent(agents.active_idx.clone());

    // Bind Shift+Tab for planner toggle
    rl.bind_sequence(
        KeyEvent(KeyCode::BackTab, Modifiers::empty()),
        EventHandler::Conditional(Box::new(PlannerToggleHandler::new(swap_state.clone()))),
    );

    // Future: Add more agent shortcuts here
    // rl.bind_sequence(KeyEvent(KeyCode::F1, Modifiers::empty()),
    // AgentSwitchHandler::new("agent_name"));

    Ok(())
}
