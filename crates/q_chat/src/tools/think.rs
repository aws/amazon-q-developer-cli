use std::io::Write;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::Result;
use fig_settings::settings;
use serde::Deserialize;

use super::{
    InvokeOutput,
    OutputKind,
};

/// The Think tool allows the model to reason through complex problems during response generation.
/// It provides a dedicated space for the model to process information from tool call results,
/// navigate complex decision trees, and improve the quality of responses in multi-step scenarios.
///
/// This is a beta feature that can be enabled/disabled via settings:
/// `q settings chat.enableThinking true`
#[derive(Debug, Clone, Deserialize)]
pub struct Think {
    /// The thought content that the model wants to process
    pub thought: String,
}

impl Think {
    /// Checks if the thinking feature is enabled in settings
    pub fn is_think_tool_enabled() -> bool {
        // Default to enabled if setting doesn't exist or can't be read
        settings::get_bool_or("chat.enableThinking", true)
    }

    /// Queues up a description of the think tool for the user
    pub fn queue_description(think: &Think, updates: &mut impl Write) -> Result<()> {
        // Only show a description if there's actual thought content
        if !think.thought.trim().is_empty() {
            // Show a preview of the thought that will be displayed
            queue!(
                updates,
                style::SetForegroundColor(Color::Blue),
                style::Print("I'll share my reasoning process: "),
                style::SetForegroundColor(Color::Reset),
                style::Print(&think.thought),
                style::Print("\n")
            )?;
        }
        Ok(())
    }

    /// Invokes the think tool. This doesn't actually perform any system operations,
    /// it's purely for the model's internal reasoning process.
    pub async fn invoke(&self, _updates: &mut impl Write) -> Result<InvokeOutput> {
        // The think tool always returns an empty output because:
        // 1. When enabled with content: We've already shown the thought in queue_description
        // 2. When disabled or empty: Nothing should be shown
        Ok(InvokeOutput {
            output: OutputKind::Text(String::new()),
        })
    }

    /// Validates the thought - accepts empty thoughts
    pub async fn validate(&mut self, _ctx: &fig_os_shim::Context) -> Result<()> {
        // We accept empty thoughts - they'll just be ignored
        // This makes the tool more robust and prevents errors from blocking the model
        Ok(())
    }
}
