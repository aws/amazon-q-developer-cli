use std::str::FromStr;

use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
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
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Running => "●",
            Self::Completed => "✓",
            Self::Failed => "✗",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

impl FromStr for AgentStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            _ => Ok(Self::Failed), // Default to Failed for unknown values
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct AgentExecution {
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub task: String,
    #[serde(default, with = "status_serde")]
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

mod status_serde {
    use serde::{
        Deserialize,
        Deserializer,
        Serializer,
    };

    use super::AgentStatus;

    pub fn serialize<S>(status: &AgentStatus, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(status.as_str())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<AgentStatus, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse()
            .map_err(|_err| serde::de::Error::custom("Invalid agent status"))
    }
}

impl AgentExecution {
    pub fn format_status_line(&self) -> String {
        match self.status {
            AgentStatus::Running => format!(
                "{} {} - Running (started {})",
                self.status.symbol(),
                self.agent,
                self.launched_at
            ),
            AgentStatus::Completed => format!(
                "{} {} - Completed ({})",
                self.status.symbol(),
                self.agent,
                self.completed_at.as_deref().unwrap_or_default()
            ),
            AgentStatus::Failed => format!(
                "{} {} - Failed (exit code: {})",
                self.status.symbol(),
                self.agent,
                self.exit_code.unwrap_or(-1)
            ),
        }
    }

    pub fn format_detailed_status(&self) -> String {
        match self.status {
            AgentStatus::Running => {
                format!(
                    "Agent '{}' is currently running.\nTask: {}\nStarted: {}",
                    self.agent, self.task, self.launched_at
                )
            },
            AgentStatus::Completed => {
                let output_preview = if self.output.is_empty() {
                    "No output available".to_string()
                } else {
                    format!(
                        "Agent completed successfully. Output ({} characters):\n\n{}",
                        self.output.len(),
                        self.output
                    )
                };

                format!(
                    "Agent '{}' completed successfully.\nTask: {}\nStarted: {}\nCompleted: {}\n\nOutput:\n{}",
                    self.agent,
                    self.task,
                    self.launched_at,
                    self.completed_at.as_deref().unwrap_or_default(),
                    output_preview
                )
            },
            AgentStatus::Failed => {
                let error_info = self.truncated_output(1000);

                format!(
                    "Agent '{}' failed.\nTask: {}\nStarted: {}\nFailed: {}\nExit code: {}\n\nError details:\n{}",
                    self.agent,
                    self.task,
                    self.launched_at,
                    self.completed_at.as_deref().unwrap_or_default(),
                    self.exit_code.unwrap_or(-1),
                    error_info
                )
            },
        }
    }

    pub fn truncated_output(&self, max_len: usize) -> String {
        if self.output.is_empty() {
            "No error details available".to_string()
        } else if self.output.len() > max_len {
            format!(
                "{}...\n\n[Error output truncated - {} characters total]",
                &self.output[..max_len],
                self.output.len()
            )
        } else {
            self.output.clone()
        }
    }

    pub fn is_active(&self) -> bool {
        self.status == AgentStatus::Running
    }
}

#[derive(Debug, Deserialize)]
pub struct AgentConfig {
    pub description: Option<String>,
    #[serde(rename = "allowedTools")]
    pub allowed_tools: Vec<String>,
}
