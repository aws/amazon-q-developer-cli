use clap::Parser;
use crossterm::{execute, queue, style};
use eyre::Result;

use crate::cli::chat::{ChatError, ChatSession, ChatState};
use crate::database::settings::ThemeName;
use crate::os::Os;

#[derive(Debug, PartialEq, Parser)]
pub struct ThemeArgs {
    /// Theme name to set (default, light, high-contrast, nord)
    theme: Option<String>,
}

impl ThemeArgs {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let colors = &session.colors;
        
        if let Some(theme_name) = self.theme {
            let theme = match theme_name.as_str() {
                "default" => ThemeName::Default,
                "light" => ThemeName::Light,
                "high-contrast" | "high_contrast" => ThemeName::HighContrast,
                "nord" => ThemeName::Nord,
                _ => {
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(colors.error()),
                        style::Print("Error: "),
                        style::ResetColor,
                        style::Print("Invalid theme name. Available themes: default, light, high-contrast, nord\n")
                    )?;
                    return Ok(ChatState::PromptUser { skip_printing_tools: true });
                }
            };
            
            os.database.settings.set_theme(theme).await.map_err(|e| ChatError::Custom(e.to_string().into()))?;
            session.colors = crate::cli::chat::colors::ColorManager::from_settings(&os.database.settings);
            
            execute!(
                session.stderr,
                style::SetForegroundColor(session.colors.success()),
                style::Print("✓ "),
                style::ResetColor,
                style::Print("Theme set to "),
                style::SetForegroundColor(session.colors.primary()),
                style::Print(&theme_name),
                style::ResetColor,
                style::Print(". Colors updated immediately.\n")
            )?;
        } else {
            let current_theme = os.database.settings.get_theme()
                .map(|t| t.as_str())
                .unwrap_or("default");
            
            execute!(
                session.stderr,
                style::SetForegroundColor(colors.info()),
                style::Print("Current theme: "),
                style::SetForegroundColor(colors.primary()),
                style::Print(current_theme),
                style::ResetColor,
                style::Print("\n\nColor preview:\n")
            )?;
            
            let samples = [
                ("Success", colors.success(), "✓ Operation completed"),
                ("Error", colors.error(), "✗ Something went wrong"),
                ("Warning", colors.warning(), "⚠ Warning message"),
                ("Info", colors.info(), "ℹ Information"),
                ("Primary", colors.primary(), "Primary text"),
                ("Secondary", colors.secondary(), "Secondary text"),
                ("Action", colors.action(), "Action required"),
                ("Data", colors.data(), "Data output"),
            ];
            
            for (name, color, sample) in samples {
                queue!(
                    session.stderr,
                    style::Print("  "),
                    style::SetForegroundColor(color),
                    style::Print(format!("{:<10}", name)),
                    style::ResetColor,
                    style::Print(" "),
                    style::SetForegroundColor(color),
                    style::Print(sample),
                    style::ResetColor,
                    style::Print("\n")
                )?;
            }
            
            execute!(
                session.stderr,
                style::Print("\nAvailable themes: "),
                style::SetForegroundColor(colors.secondary()),
                style::Print("default, light, high-contrast, nord"),
                style::ResetColor,
                style::Print("\nUsage: /theme <theme_name>\n")
            )?;
        }
        
        Ok(ChatState::PromptUser { skip_printing_tools: true })
    }
}
