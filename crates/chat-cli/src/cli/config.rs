use std::process::ExitCode;

use anstream::println;
use clap::{Args, Subcommand, ValueEnum};
use eyre::Result;
use serde_json::json;

use crate::database::settings::Setting;
use crate::os::Os;

#[derive(Clone, Debug, ValueEnum, PartialEq)]
pub enum BoolValue {
    True,
    False,
}

impl BoolValue {
    fn as_bool(&self) -> bool {
        matches!(self, BoolValue::True)
    }
}

#[derive(Clone, Debug, Subcommand, PartialEq)]
pub enum ConfigSubcommand {
    /// Enable or disable Bedrock backend mode
    Bedrock {
        /// Enable or disable Bedrock mode (true/false)
        #[arg(value_enum)]
        enable: BoolValue,
    },
    /// Set AWS region for Bedrock
    Region {
        /// AWS region name (e.g., us-west-2)
        region: String,
    },
    /// Set maximum output tokens
    MaxTokens {
        /// Maximum output tokens (e.g., 4096, max 200000)
        tokens: u32,
    },
    /// Select a Bedrock model interactively
    BedrockModel,
    /// Enable or disable extended thinking mode
    Thinking {
        /// Enable or disable thinking mode (true/false)
        #[arg(value_enum)]
        enabled: BoolValue,
    },
    /// Set temperature for model responses
    Temperature {
        /// Temperature value between 0.0 and 1.0
        value: f64,
    },
    /// Manage custom system prompts
    #[command(subcommand)]
    SystemPrompt(SystemPromptCommand),
}

#[derive(Clone, Debug, Subcommand, PartialEq)]
pub enum SystemPromptCommand {
    /// Add a new custom system prompt
    Add {
        /// Name of the system prompt
        name: String,
        /// The prompt text
        prompt: String,
    },
    /// Enable a system prompt
    Enable {
        /// Name of the system prompt to enable
        name: String,
    },
    /// Use default system prompt (deactivate custom prompts)
    Default,
    /// Delete a system prompt
    Delete {
        /// Name of the system prompt to delete
        name: String,
    },
    /// List all system prompts
    List,
}

#[derive(Clone, Debug, Args, PartialEq)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub cmd: ConfigSubcommand,
}

impl ConfigArgs {
    pub async fn execute(&self, os: &mut Os) -> Result<ExitCode> {
        match &self.cmd {
            ConfigSubcommand::Bedrock { enable } => {
                let enabled = enable.as_bool();
                os.database.settings.set(Setting::BedrockEnabled, json!(enabled)).await?;
                println!("Bedrock mode {}", if enabled { "enabled" } else { "disabled" });
                Ok(ExitCode::SUCCESS)
            },
            ConfigSubcommand::Region { region } => {
                os.database.settings.set(Setting::BedrockRegion, json!(region)).await?;
                println!("Bedrock region set to: {}", region);
                Ok(ExitCode::SUCCESS)
            },
            ConfigSubcommand::MaxTokens { tokens } => {
                if *tokens > 200000 {
                    return Err(eyre::eyre!("Maximum output tokens cannot exceed 200000"));
                }
                os.database.settings.set(Setting::BedrockMaxTokens, json!(tokens)).await?;
                println!("Maximum output tokens set to: {}", tokens);
                Ok(ExitCode::SUCCESS)
            },
            ConfigSubcommand::BedrockModel => {
                use crate::api_client::bedrock::BedrockApiClient;
                use dialoguer::Select;

                // Create Bedrock client to list models
                let bedrock_client = BedrockApiClient::new(os.database.clone())
                    .await
                    .map_err(|e| eyre::eyre!("Failed to create Bedrock client: {}", e))?;

                // Get available models
                let models = bedrock_client.list_foundation_models()
                    .await
                    .map_err(|e| eyre::eyre!("Failed to list models: {}", e))?;

                if models.is_empty() {
                    return Err(eyre::eyre!("No models available"));
                }

                // Get current model
                let current_model = os.database.settings.get(Setting::BedrockModel)
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                // Find default selection
                let default_index = current_model
                    .as_ref()
                    .and_then(|current| models.iter().position(|m| m == current))
                    .unwrap_or(0);

                // Show selection dialog
                let selection = Select::with_theme(&crate::util::dialoguer_theme())
                    .with_prompt("Select a Bedrock model")
                    .items(&models)
                    .default(default_index)
                    .interact_opt()
                    .map_err(|e| eyre::eyre!("Failed to select model: {}", e))?;

                if let Some(index) = selection {
                    let selected_model = &models[index];
                    os.database.settings.set(Setting::BedrockModel, json!(selected_model)).await?;
                    println!("Bedrock model set to: {}", selected_model);
                }

                Ok(ExitCode::SUCCESS)
            },
            ConfigSubcommand::Thinking { enabled } => {
                let enabled_bool = enabled.as_bool();
                os.database.settings.set(Setting::BedrockThinkingEnabled, json!(enabled_bool)).await?;
                if enabled_bool {
                    println!("Thinking mode enabled (temperature will be set to 1.0)");
                } else {
                    println!("Thinking mode disabled");
                }
                Ok(ExitCode::SUCCESS)
            },
            ConfigSubcommand::Temperature { value } => {
                if !(0.0..=1.0).contains(value) {
                    return Err(eyre::eyre!("Temperature must be between 0.0 and 1.0"));
                }
                
                let thinking_enabled = os
                    .database
                    .settings
                    .get(Setting::BedrockThinkingEnabled)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                
                if thinking_enabled {
                    println!("Warning: Thinking mode is enabled. Temperature is automatically set to 1.0");
                    return Ok(ExitCode::SUCCESS);
                }
                
                os.database.settings.set(Setting::BedrockTemperature, json!(value)).await?;
                println!("Temperature set to: {}", value);
                Ok(ExitCode::SUCCESS)
            },
            ConfigSubcommand::SystemPrompt(cmd) => {
                match cmd {
                    SystemPromptCommand::Add { name, prompt } => {
                        let key = format!("bedrock.systemPrompt.{}", name);
                        os.database.settings.set_raw(&key, json!(prompt)).await?;
                        println!("System prompt '{}' added", name);
                        Ok(ExitCode::SUCCESS)
                    },
                    SystemPromptCommand::Enable { name } => {
                        let key = format!("bedrock.systemPrompt.{}", name);
                        if os.database.settings.get_raw(&key).is_none() {
                            return Err(eyre::eyre!("System prompt '{}' not found", name));
                        }
                        os.database.settings.set(Setting::BedrockSystemPromptActive, json!(name)).await?;
                        println!("System prompt '{}' enabled", name);
                        Ok(ExitCode::SUCCESS)
                    },
                    SystemPromptCommand::Default => {
                        os.database.settings.remove(Setting::BedrockSystemPromptActive).await?;
                        println!("Using default system prompt");
                        Ok(ExitCode::SUCCESS)
                    },
                    SystemPromptCommand::Delete { name } => {
                        let key = format!("bedrock.systemPrompt.{}", name);
                        os.database.settings.remove_raw(&key).await?;
                        
                        let active = os.database.settings.get(Setting::BedrockSystemPromptActive)
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        
                        if active.as_deref() == Some(name.as_str()) {
                            os.database.settings.remove(Setting::BedrockSystemPromptActive).await?;
                            println!("System prompt '{}' deleted and deactivated", name);
                        } else {
                            println!("System prompt '{}' deleted", name);
                        }
                        Ok(ExitCode::SUCCESS)
                    },
                    SystemPromptCommand::List => {
                        let all_settings = os.database.settings.map();
                        let prompts: Vec<_> = all_settings
                            .iter()
                            .filter(|(k, _)| k.starts_with("bedrock.systemPrompt.") && *k != "bedrock.systemPrompt.active")
                            .collect();
                        
                        if prompts.is_empty() {
                            println!("No custom system prompts configured");
                        } else {
                            let active = os.database.settings.get(Setting::BedrockSystemPromptActive)
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            
                            println!("Custom system prompts:");
                            for (key, value) in prompts {
                                let name = key.strip_prefix("bedrock.systemPrompt.").unwrap();
                                let is_active = active.as_deref() == Some(name);
                                let marker = if is_active { " (active)" } else { "" };
                                println!("  - {}{}", name, marker);
                                if let Some(text) = value.as_str() {
                                    let preview = if text.len() > 60 {
                                        format!("{}...", &text[..60])
                                    } else {
                                        text.to_string()
                                    };
                                    println!("    {}", preview);
                                }
                            }
                        }
                        Ok(ExitCode::SUCCESS)
                    },
                }
            },
        }
    }
}
