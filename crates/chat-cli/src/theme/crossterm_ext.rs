//! Crossterm extensions for the theme system
use chat_cli_ui::legacy_ui_util::ThemeSource;
use crossterm::style::{
    Attribute,
    Color,
    ResetColor,
    SetAttribute,
    SetForegroundColor,
};

use crate::theme::theme;

/// Unified text styling API that provides both string methods and crossterm commands
pub struct StyledText;

impl StyledText {
    // ===== High-level string methods =====
    // These return formatted strings ready for printing

    /// Create error-styled text
    pub fn error(text: &str) -> String {
        format!("\x1b[{}m{}\x1b[0m", color_to_ansi_code(theme().status.error), text)
    }

    /// Create info-styled text
    pub fn info(text: &str) -> String {
        format!("\x1b[{}m{}\x1b[0m", color_to_ansi_code(theme().status.info), text)
    }

    /// Create emphasis-styled text
    pub fn emphasis(text: &str) -> String {
        format!("\x1b[{}m{}\x1b[0m", color_to_ansi_code(theme().ui.emphasis), text)
    }

    /// Create command-styled text
    pub fn command(text: &str) -> String {
        format!(
            "\x1b[{}m{}\x1b[0m",
            color_to_ansi_code(theme().ui.command_highlight),
            text
        )
    }

    // ===== Interactive element string methods =====
    // These are for UI elements that indicate interaction or state

    /// Create prompt-styled text
    pub fn prompt(text: &str) -> String {
        format!(
            "\x1b[{}m{}\x1b[0m",
            color_to_ansi_code(theme().interactive.prompt_symbol),
            text
        )
    }

    /// Create profile-styled text
    pub fn profile(text: &str) -> String {
        format!(
            "\x1b[{}m{}\x1b[0m",
            color_to_ansi_code(theme().interactive.profile_indicator),
            text
        )
    }

    /// Create tangent-styled text
    pub fn tangent(text: &str) -> String {
        format!(
            "\x1b[{}m{}\x1b[0m",
            color_to_ansi_code(theme().interactive.tangent_indicator),
            text
        )
    }

    /// Create usage-low-styled text
    pub fn usage_low(text: &str) -> String {
        format!(
            "\x1b[{}m{}\x1b[0m",
            color_to_ansi_code(theme().interactive.usage_low),
            text
        )
    }

    /// Create usage-medium-styled text
    pub fn usage_medium(text: &str) -> String {
        format!(
            "\x1b[{}m{}\x1b[0m",
            color_to_ansi_code(theme().interactive.usage_medium),
            text
        )
    }

    /// Create usage-high-styled text
    pub fn usage_high(text: &str) -> String {
        format!(
            "\x1b[{}m{}\x1b[0m",
            color_to_ansi_code(theme().interactive.usage_high),
            text
        )
    }

    /// Create brand-styled text (primary brand color)
    pub fn brand(text: &str) -> String {
        format!("\x1b[{}m{}\x1b[0m", color_to_ansi_code(theme().ui.primary_brand), text)
    }

    /// Create primary-styled text (primary text color)
    pub fn primary(text: &str) -> String {
        format!("\x1b[{}m{}\x1b[0m", color_to_ansi_code(theme().ui.primary_text), text)
    }

    /// Create secondary-styled text (muted/helper text)
    pub fn secondary(text: &str) -> String {
        format!("\x1b[{}m{}\x1b[0m", color_to_ansi_code(theme().ui.secondary_text), text)
    }

    /// Create success-styled text
    pub fn success(text: &str) -> String {
        format!("\x1b[{}m{}\x1b[0m", color_to_ansi_code(theme().status.success), text)
    }

    // ===== Low-level crossterm command methods =====
    // These return crossterm commands for complex terminal operations

    /// Set foreground to error color
    pub fn error_fg() -> SetForegroundColor {
        SetForegroundColor(theme().status.error)
    }

    /// Set foreground to warning color
    pub fn warning_fg() -> SetForegroundColor {
        SetForegroundColor(theme().status.warning)
    }

    /// Set foreground to success color
    pub fn success_fg() -> SetForegroundColor {
        SetForegroundColor(theme().status.success)
    }

    /// Set foreground to info color
    pub fn info_fg() -> SetForegroundColor {
        SetForegroundColor(theme().status.info)
    }

    /// Set foreground to primary brand color
    pub fn brand_fg() -> SetForegroundColor {
        SetForegroundColor(theme().ui.primary_brand)
    }

    /// Set foreground to secondary text color
    pub fn secondary_fg() -> SetForegroundColor {
        SetForegroundColor(theme().ui.secondary_text)
    }

    /// Set foreground to emphasis color
    pub fn emphasis_fg() -> SetForegroundColor {
        SetForegroundColor(theme().ui.emphasis)
    }

    /// Reset all styling to default
    pub fn reset() -> ResetColor {
        ResetColor
    }

    /// Reset attributes
    pub fn reset_attributes() -> SetAttribute {
        SetAttribute(Attribute::Reset)
    }
}

impl ThemeSource for StyledText {
    fn error(&self, text: &str) -> String {
        StyledText::error(text)
    }

    fn info(&self, text: &str) -> String {
        StyledText::info(text)
    }

    fn emphasis(&self, text: &str) -> String {
        StyledText::emphasis(text)
    }

    fn command(&self, text: &str) -> String {
        StyledText::command(text)
    }

    fn prompt(&self, text: &str) -> String {
        StyledText::prompt(text)
    }

    fn profile(&self, text: &str) -> String {
        StyledText::profile(text)
    }

    fn tangent(&self, text: &str) -> String {
        StyledText::tangent(text)
    }

    fn usage_low(&self, text: &str) -> String {
        StyledText::usage_low(text)
    }

    fn usage_medium(&self, text: &str) -> String {
        StyledText::usage_medium(text)
    }

    fn usage_high(&self, text: &str) -> String {
        StyledText::usage_high(text)
    }

    fn brand(&self, text: &str) -> String {
        StyledText::brand(text)
    }

    fn primary(&self, text: &str) -> String {
        StyledText::primary(text)
    }

    fn secondary(&self, text: &str) -> String {
        StyledText::secondary(text)
    }

    fn success(&self, text: &str) -> String {
        StyledText::success(text)
    }

    fn error_fg(&self) -> SetForegroundColor {
        StyledText::error_fg()
    }

    fn warning_fg(&self) -> SetForegroundColor {
        StyledText::warning_fg()
    }

    fn success_fg(&self) -> SetForegroundColor {
        StyledText::success_fg()
    }

    fn info_fg(&self) -> SetForegroundColor {
        StyledText::info_fg()
    }

    fn brand_fg(&self) -> SetForegroundColor {
        StyledText::brand_fg()
    }

    fn secondary_fg(&self) -> SetForegroundColor {
        StyledText::secondary_fg()
    }

    fn emphasis_fg(&self) -> SetForegroundColor {
        StyledText::emphasis_fg()
    }

    fn reset(&self) -> ResetColor {
        StyledText::reset()
    }

    fn reset_attributes(&self) -> SetAttribute {
        StyledText::reset_attributes()
    }
}

/// Convert a crossterm Color to ANSI color code
fn color_to_ansi_code(color: Color) -> u8 {
    match color {
        Color::Black => 30,
        Color::DarkGrey => 90,
        Color::Red => 31,
        Color::DarkRed => 31,
        Color::Green => 32,
        Color::DarkGreen => 32,
        Color::Yellow => 33,
        Color::DarkYellow => 33,
        Color::Blue => 34,
        Color::DarkBlue => 34,
        Color::Magenta => 35,
        Color::DarkMagenta => 35,
        Color::Cyan => 36,
        Color::DarkCyan => 36,
        Color::White => 37,
        Color::Grey => 37,
        Color::Rgb { r, g, b } => {
            // For RGB colors, we'll use a simplified mapping to the closest basic color
            // This is a fallback - in practice, most terminals support RGB
            if r > 200 && g < 100 && b < 100 {
                31
            }
            // Red-ish
            else if r < 100 && g > 200 && b < 100 {
                32
            }
            // Green-ish
            else if r > 200 && g > 200 && b < 100 {
                33
            }
            // Yellow-ish
            else if r < 100 && g < 100 && b > 200 {
                34
            }
            // Blue-ish
            else if r > 200 && g < 100 && b > 200 {
                35
            }
            // Magenta-ish
            else if r < 100 && g > 200 && b > 200 {
                36
            }
            // Cyan-ish
            else if r > 150 && g > 150 && b > 150 {
                37
            }
            // White-ish
            else {
                30
            } // Black-ish
        },
        Color::AnsiValue(val) => {
            // Map ANSI 256 colors to basic 8 colors
            match val {
                0..=7 => 30 + val,
                8..=15 => 90 + (val - 8),
                _ => 37, // Default to white for other values
            }
        },
        Color::Reset => 37, // Default to white
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_styled_text_string_methods() {
        // Test that string methods return non-empty strings
        assert!(!StyledText::error("test").is_empty());
        assert!(!StyledText::info("test").is_empty());
        assert!(!StyledText::emphasis("test").is_empty());
        assert!(!StyledText::command("test").is_empty());
        assert!(!StyledText::prompt("test").is_empty());
        assert!(!StyledText::profile("test").is_empty());
        assert!(!StyledText::tangent("test").is_empty());
        assert!(!StyledText::usage_low("test").is_empty());
        assert!(!StyledText::usage_medium("test").is_empty());
        assert!(!StyledText::usage_high("test").is_empty());
    }

    #[test]
    fn test_styled_text_crossterm_methods() {
        // Test that crossterm methods return the expected command types
        let _error_fg = StyledText::error_fg();
        let _warning_fg = StyledText::warning_fg();
        let _success_fg = StyledText::success_fg();
        let _info_fg = StyledText::info_fg();
        let _brand_fg = StyledText::brand_fg();
        let _secondary_fg = StyledText::secondary_fg();
        let _emphasis_fg = StyledText::emphasis_fg();
        let _reset = StyledText::reset();
        let _reset_attr = StyledText::reset_attributes();

        assert!(true);
    }

    #[test]
    fn test_color_to_ansi_code() {
        assert_eq!(color_to_ansi_code(Color::Red), 31);
        assert_eq!(color_to_ansi_code(Color::Green), 32);
        assert_eq!(color_to_ansi_code(Color::Blue), 34);
        assert_eq!(color_to_ansi_code(Color::Yellow), 33);
        assert_eq!(color_to_ansi_code(Color::Cyan), 36);
        assert_eq!(color_to_ansi_code(Color::Magenta), 35);
        assert_eq!(color_to_ansi_code(Color::White), 37);
        assert_eq!(color_to_ansi_code(Color::Black), 30);
    }
}
