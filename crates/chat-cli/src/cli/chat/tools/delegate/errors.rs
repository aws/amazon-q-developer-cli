pub struct AgentError;

impl AgentError {
    pub fn not_found(agent: &str, available: &[String]) -> String {
        if available.is_empty() {
            format!(
                "✗ I can't find agent '{}'. No agents are configured. You need to set up agents first.",
                agent
            )
        } else {
            format!(
                "✗ I can't find agent '{}'. Available agents: {}\n\nPlease use one of the available agents or set up the '{}' agent first.",
                agent,
                available.join(", "),
                agent
            )
        }
    }

    pub fn already_running(agent: &str) -> String {
        format!("Agent '{}' is already running a task", agent)
    }

    pub fn no_execution_found(agent: &str) -> String {
        format!("No execution found for agent '{}'", agent)
    }
}
