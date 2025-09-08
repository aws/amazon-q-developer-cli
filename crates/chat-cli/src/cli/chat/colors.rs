//! Centralized color management for Amazon Q CLI
//!
//! This module provides a centralized way to manage colors throughout the CLI,
//! ensuring consistent theming and accessibility support.

use crossterm::style::Color;

use crate::database::settings::{
    ColorCategory,
    ColorTheme,
    Settings,
};

/// Color manager that provides semantic color access
#[derive(Clone)]
pub struct ColorManager {
    theme: ColorTheme,
}

impl ColorManager {
    /// Create a new color manager from settings
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            theme: settings.get_color_theme(),
        }
    }

    /// Create a new color manager with default theme
    ///
    /// WARNING: This bypasses user theme settings. Use `from_settings()` instead
    /// when Settings are available to respect user configuration.
    pub fn new_with_default_theme() -> Self {
        Self {
            theme: ColorTheme::default(),
        }
    }

    /// Get color for a specific category
    pub fn get(&self, category: ColorCategory) -> Color {
        self.theme.get_color(category)
    }

    /// Get success color (for completions, positive feedback)
    pub fn success(&self) -> Color {
        self.get(ColorCategory::Success)
    }

    /// Get error color (for failures, critical issues)
    pub fn error(&self) -> Color {
        self.get(ColorCategory::Error)
    }

    /// Get warning color (for cautions, informational alerts)
    pub fn warning(&self) -> Color {
        self.get(ColorCategory::Warning)
    }

    /// Get info color (for informational content, references)
    pub fn info(&self) -> Color {
        self.get(ColorCategory::Info)
    }

    /// Get secondary color (for help text, less prominent elements)
    pub fn secondary(&self) -> Color {
        self.get(ColorCategory::Secondary)
    }

    /// Get primary color (for branding, important system messages)
    pub fn primary(&self) -> Color {
        self.get(ColorCategory::Primary)
    }

    /// Get action color (for tool usage, user interactions)
    pub fn action(&self) -> Color {
        self.get(ColorCategory::Action)
    }

    /// Get data color (for context files, data visualization)
    pub fn data(&self) -> Color {
        self.get(ColorCategory::Data)
    }
}

/// Convenience macros for common color operations
#[macro_export]
macro_rules! with_color {
    ($output:expr, $color_manager:expr, $category:expr, $($arg:tt)*) => {
        {
            use crossterm::{queue, style};
            queue!(
                $output,
                style::SetForegroundColor($color_manager.get($category)),
                style::Print(format!($($arg)*)),
                style::SetForegroundColor(style::Color::Reset)
            )
        }
    };
}

#[macro_export]
macro_rules! with_success {
    ($output:expr, $color_manager:expr, $($arg:tt)*) => {
        with_color!($output, $color_manager, $crate::database::settings::ColorCategory::Success, $($arg)*)
    };
}

#[macro_export]
macro_rules! with_error {
    ($output:expr, $color_manager:expr, $($arg:tt)*) => {
        with_color!($output, $color_manager, $crate::database::settings::ColorCategory::Error, $($arg)*)
    };
}

#[macro_export]
macro_rules! with_warning {
    ($output:expr, $color_manager:expr, $($arg:tt)*) => {
        with_color!($output, $color_manager, $crate::database::settings::ColorCategory::Warning, $($arg)*)
    };
}

#[macro_export]
macro_rules! with_info {
    ($output:expr, $color_manager:expr, $($arg:tt)*) => {
        with_color!($output, $color_manager, $crate::database::settings::ColorCategory::Info, $($arg)*)
    };
}

#[macro_export]
macro_rules! with_secondary {
    ($output:expr, $color_manager:expr, $($arg:tt)*) => {
        with_color!($output, $color_manager, $crate::database::settings::ColorCategory::Secondary, $($arg)*)
    };
}

#[macro_export]
macro_rules! with_primary {
    ($output:expr, $color_manager:expr, $($arg:tt)*) => {
        with_color!($output, $color_manager, $crate::database::settings::ColorCategory::Primary, $($arg)*)
    };
}

#[macro_export]
macro_rules! with_action {
    ($output:expr, $color_manager:expr, $($arg:tt)*) => {
        with_color!($output, $color_manager, $crate::database::settings::ColorCategory::Action, $($arg)*)
    };
}

#[macro_export]
macro_rules! with_data {
    ($output:expr, $color_manager:expr, $($arg:tt)*) => {
        with_color!($output, $color_manager, $crate::database::settings::ColorCategory::Data, $($arg)*)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::settings::Settings;

    #[tokio::test]
    async fn test_color_manager_default() {
        let manager = ColorManager::new_with_default_theme();
        assert_eq!(manager.success(), Color::Green);
        assert_eq!(manager.error(), Color::Red);
        assert_eq!(manager.warning(), Color::Yellow);
        assert_eq!(manager.info(), Color::Blue);
        assert_eq!(manager.secondary(), Color::DarkGrey);
        assert_eq!(manager.primary(), Color::Cyan);
        assert_eq!(manager.action(), Color::Magenta);
        assert_eq!(manager.data(), Color::DarkCyan);
    }

    #[tokio::test]
    async fn test_color_manager_from_settings() {
        let settings = Settings::new().await.unwrap();
        let manager = ColorManager::from_settings(&settings);

        // Should use default colors when no settings are configured
        assert_eq!(manager.success(), Color::Green);
        assert_eq!(manager.error(), Color::Red);
        assert_eq!(manager.secondary(), Color::DarkGrey);
    }
}
