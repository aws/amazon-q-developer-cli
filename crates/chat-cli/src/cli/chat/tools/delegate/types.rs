use serde::{
    Deserialize,
    Serialize,
};
use strum::{Display, EnumString};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Display, EnumString)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Running,
    Completed,
    Failed,
}

impl Default for AgentStatus {
    fn default() -> Self {
        Self::Running
    }
}

impl AgentStatus {
    // No methods currently needed - all functionality is in format_status
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct AgentExecution {
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub status: AgentStatus,
    #[serde(default)]
    pub launched_at: String,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub pid: u32,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub output: String,
}

impl AgentExecution {
    pub fn format_status(&self) -> String {
        match self.status {
            AgentStatus::Running => {
                format!("Agent '{}' is still running. Please wait...", self.agent)
            },
            AgentStatus::Completed => {
                format!("Agent '{}' completed successfully.\n\nOutput:\n{}", 
                    self.agent, self.output)
            },
            AgentStatus::Failed => {
                format!("Agent '{}' failed.\nExit code: {}\n\nError:\n{}", 
                    self.agent, 
                    self.exit_code.unwrap_or(-1),
                    self.output)
            },
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AgentConfig {
    pub description: Option<String>,
    #[serde(rename = "allowedTools")]
    pub allowed_tools: Vec<String>,
}
