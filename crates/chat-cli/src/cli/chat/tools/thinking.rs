use std::io::Write;

use crossterm::queue;
use crossterm::style;
use eyre::Result;
use serde::Deserialize;

use super::{
    InvokeOutput,
    OutputKind,
};
use crate::cli::chat::colors::ColorManager;
use crate::database::settings::{Setting, Settings};
use crate::{with_info, with_color};
use crate::os::Os;

/// The Think tool allows the model to reason through complex problems during response generation.
/// It provides a dedicated space for the model to process information from tool call results,
/// navigate complex decision trees, and improve the quality of responses in multi-step scenarios.
///
/// This is a beta feature that can be enabled/disabled via settings:
/// `q settings chat.enableThinking true`
#[derive(Debug, Clone, Deserialize)]
pub struct Thinking {
    /// The thought content that the model wants to process
    pub thought: String,
}

impl Thinking {
    /// Checks if the thinking feature is enabled in settings
    pub fn is_enabled(os: &Os) -> bool {
        os.database.settings.get_bool(Setting::EnabledThinking).unwrap_or(false)
    }

    /// Queues up a description of the think tool for the user
    pub fn queue_description(&self, output: &mut impl Write) -> Result<()> {
        // Only show a description if there's actual thought content
        if !self.thought.trim().is_empty() {
            let settings = Settings::default();
            let color_manager = ColorManager::from_settings(&settings);

            // Show a preview of the thought that will be displayed
            with_info!(output, &color_manager, "I'll share my reasoning process: ")?;
            queue!(output, style::Print(&self.thought), style::Print("\n"))?;
        }
        Ok(())
    }

    /// Invokes the think tool. This doesn't actually perform any system operations,
    /// it's purely for the model's internal reasoning process.
    pub async fn invoke(&self, _updates: impl Write) -> Result<InvokeOutput> {
        // The think tool always returns an empty output because:
        // 1. When enabled with content: We've already shown the thought in queue_description
        // 2. When disabled or empty: Nothing should be shown
        Ok(InvokeOutput {
            output: OutputKind::Text(String::new()),
        })
    }

    /// Validates the thought - accepts empty thoughts
    pub async fn validate(&mut self, _os: &crate::os::Os) -> Result<()> {
        // We accept empty thoughts - they'll just be ignored
        // This makes the tool more robust and prevents errors from blocking the model
        Ok(())
    }
}
