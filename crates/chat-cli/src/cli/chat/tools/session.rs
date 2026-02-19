use std::io::Write;

use eyre::Result;
use serde::{
    Deserialize,
    Serialize,
};
use strum::{
    EnumMessage,
    IntoEnumIterator,
};

use super::{
    InvokeOutput,
    OutputKind,
    ToolInfo,
};
use crate::cli::experiment::experiment_manager::ExperimentManager;
use crate::database::settings::{
    Setting,
    SettingScope,
};
use crate::os::Os;

#[derive(Debug, Clone, Deserialize)]
pub struct Session {
    /// Operation: "list", "get", "set"
    operation: String,
    /// Setting key (e.g., "chat.disableMarkdownRendering")
    #[serde(default)]
    key: Option<String>,
    /// Value to set
    #[serde(default)]
    value: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SettingInfo {
    pub key: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_value: Option<serde_json::Value>,
    pub overridable: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<Vec<SettingInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Internal: indicates a pending set operation for post_process
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_set: Option<bool>,
}

impl Session {
    pub const INFO: ToolInfo = ToolInfo {
        spec_name: "session",
        preferred_alias: "session",
        aliases: &["session"],
    };

    pub fn queue_description(&self, tool: &super::tool::Tool, output: &mut impl Write) -> Result<()> {
        match self.operation.as_str() {
            "list" => write!(output, "kiro-cli settings")?,
            "get" => {
                if let Some(key) = &self.key {
                    write!(output, "kiro-cli settings {}", key)?;
                }
            },
            "set" => {
                if let (Some(key), Some(value)) = (&self.key, &self.value) {
                    write!(output, "kiro-cli settings {} {}", key, value)?;
                }
            },
            "reset" => {
                if let Some(key) = &self.key {
                    write!(output, "kiro-cli settings {} --delete", key)?;
                } else {
                    write!(output, "Clearing all session settings")?;
                }
            },
            _ => {},
        }
        super::display_tool_use(tool, output)?;
        writeln!(output)?;
        Ok(())
    }

    pub async fn invoke(&self, os: &Os, _output: &mut impl Write) -> Result<InvokeOutput> {
        let response = match self.operation.as_str() {
            "list" => Self::list_settings(os),
            "get" => self.get_setting(os),
            "set" => self.prepare_set_setting(),
            "reset" => self.prepare_reset(),
            _ => SessionResponse {
                success: false,
                settings: None,
                key: None,
                value: None,
                message: Some(format!(
                    "Unknown operation: {}. Use 'list', 'get', 'set', or 'reset'",
                    self.operation
                )),
                pending_set: None,
            },
        };

        Ok(InvokeOutput {
            output: OutputKind::Json(serde_json::to_value(&response)?),
        })
    }

    /// Called after invoke to apply pending set/reset operations
    pub async fn post_process(
        &self,
        result: &InvokeOutput,
        session: &mut crate::cli::chat::ChatSession,
        os: &mut Os,
    ) -> Result<()> {
        // Check if we have a pending operation
        if let OutputKind::Json(json) = &result.output
            && let Ok(response) = serde_json::from_value::<SessionResponse>(json.clone())
            && response.pending_set == Some(true)
        {
            match self.operation.as_str() {
                "set" => {
                    if let (Some(key), Some(value)) = (&response.key, &response.value)
                        && let Ok(setting) = Setting::try_from(key.as_str())
                    {
                        // Use session scope (in-memory only, not persisted)
                        os.database
                            .settings
                            .set(setting, value.clone(), Some(SettingScope::Session))
                            .await?;

                        // Reload tools if this is an experiment toggle
                        if ExperimentManager::is_experiment_setting(setting) {
                            session.reload_builtin_tools(os).await?;
                        }
                    }
                },
                "reset" => {
                    if let Some(key) = &response.key
                        && let Ok(setting) = Setting::try_from(key.as_str())
                    {
                        // Reset single setting
                        os.database
                            .settings
                            .remove(setting, Some(SettingScope::Session))
                            .await?;

                        // Reload tools if this is an experiment toggle
                        if ExperimentManager::is_experiment_setting(setting) {
                            session.reload_builtin_tools(os).await?;
                        }
                    } else if response.key.is_none() {
                        // Reset all session overrides - reload tools to be safe
                        os.database.settings.clear_session();
                        session.reload_builtin_tools(os).await?;
                    }
                },
                _ => {},
            }
        }
        Ok(())
    }

    fn list_settings(os: &Os) -> SessionResponse {
        // Only show settings that are currently configured (have non-default values)
        let configured_settings: Vec<SettingInfo> = Setting::iter()
            .filter(|s| s.is_session_safe() && os.database.settings.get(*s).is_some())
            .map(|setting| SettingInfo {
                key: setting.as_ref().to_string(),
                description: setting.get_message().unwrap_or("No description").to_string(),
                current_value: os.database.settings.get(setting).cloned(),
                overridable: true,
            })
            .collect();

        SessionResponse {
            success: true,
            settings: Some(configured_settings),
            key: None,
            value: None,
            message: None,
            pending_set: None,
        }
    }

    fn get_setting(&self, os: &Os) -> SessionResponse {
        let Some(key) = &self.key else {
            return SessionResponse {
                success: false,
                settings: None,
                key: None,
                value: None,
                message: Some("Missing 'key' parameter".into()),
                pending_set: None,
            };
        };

        match Setting::try_from(key.as_str()) {
            Ok(setting) => SessionResponse {
                success: true,
                settings: None,
                key: Some(key.clone()),
                value: os.database.settings.get(setting).cloned(),
                message: None,
                pending_set: None,
            },
            Err(_) => SessionResponse {
                success: false,
                settings: None,
                key: Some(key.clone()),
                value: None,
                message: Some(format!("Unknown setting: {}", key)),
                pending_set: None,
            },
        }
    }

    /// Validates and prepares a set operation (actual set happens in post_process)
    fn prepare_set_setting(&self) -> SessionResponse {
        let Some(key) = &self.key else {
            return SessionResponse {
                success: false,
                settings: None,
                key: None,
                value: None,
                message: Some("Missing 'key' parameter".into()),
                pending_set: None,
            };
        };

        let Some(value) = &self.value else {
            return SessionResponse {
                success: false,
                settings: None,
                key: Some(key.clone()),
                value: None,
                message: Some("Missing 'value' parameter".into()),
                pending_set: None,
            };
        };

        let setting = match Setting::try_from(key.as_str()) {
            Ok(s) => s,
            Err(_) => {
                return SessionResponse {
                    success: false,
                    settings: None,
                    key: Some(key.clone()),
                    value: None,
                    message: Some(format!("Unknown setting: {}", key)),
                    pending_set: None,
                };
            },
        };

        if !setting.is_session_safe() {
            return SessionResponse {
                success: false,
                settings: None,
                key: Some(key.clone()),
                value: None,
                message: Some(format!("Setting '{}' cannot be changed via session tool", key)),
                pending_set: None,
            };
        }

        // Return success with pending_set flag - actual set happens in post_process
        SessionResponse {
            success: true,
            settings: None,
            key: Some(key.clone()),
            value: Some(value.clone()),
            message: Some(format!("Set {} = {}", key, value)),
            pending_set: Some(true),
        }
    }

    fn prepare_reset(&self) -> SessionResponse {
        // If key is provided, reset single setting; otherwise reset all
        if let Some(key) = &self.key {
            SessionResponse {
                success: true,
                settings: None,
                key: Some(key.clone()),
                value: None,
                message: Some(format!("Reset session override for {}", key)),
                pending_set: Some(true),
            }
        } else {
            SessionResponse {
                success: true,
                settings: None,
                key: None,
                value: None,
                message: Some("Reset all session overrides".to_string()),
                pending_set: Some(true),
            }
        }
    }

    pub async fn validate(&self, _os: &Os) -> Result<()> {
        match self.operation.as_str() {
            "list" | "get" | "set" | "reset" => Ok(()),
            op => Err(eyre::eyre!(
                "Invalid operation '{}'. Use 'list', 'get', 'set', or 'reset'",
                op
            )),
        }
    }

    pub fn eval_perm(&self, _os: &Os, _agent: &crate::cli::agent::Agent) -> crate::cli::agent::PermissionEvalResult {
        use crate::cli::agent::PermissionEvalResult;

        match self.operation.as_str() {
            "set" | "reset" => {
                // Always ask user for set/reset operations, regardless of trust status
                PermissionEvalResult::ask()
            },
            // list and get are read-only, safe to allow
            _ => PermissionEvalResult::Allow,
        }
    }
}
