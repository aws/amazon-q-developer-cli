//! Color definitions and semantic color categories for the theme system

use crossterm::style::Color;

// ANSI color value constants
/// Purple brand color
pub const BRAND_PURPLE: u8 = 141;
/// Light grey for emphasis text
pub const LIGHT_GREY: u8 = 252;
/// Medium-light grey for secondary text
pub const MEDIUM_LIGHT_GREY: u8 = 244;
/// Bright magenta for emphasis and current items
pub const BRIGHT_MAGENTA: u8 = 13;

/// Colors for status messages and feedback
#[derive(Debug, Clone)]
pub struct StatusColors {
    /// Error messages and critical warnings
    pub error: Color,
    /// Warning messages and cautions
    pub warning: Color,
    /// Success messages and confirmations
    pub success: Color,
    /// Informational messages and tips
    pub info: Color,
}

/// Colors for general UI elements and text
#[derive(Debug, Clone)]
pub struct UiColors {
    /// Primary brand color
    pub primary_brand: Color,
    /// Primary text color
    pub primary_text: Color,
    /// Secondary/muted text for descriptions and helper text
    pub secondary_text: Color,
    /// Emphasis color for important text and headers (typically magenta)
    pub emphasis: Color,
    /// Color for highlighting commands and code examples (typically green)
    pub command_highlight: Color,
    /// Color for highlighting current/active items
    pub current_item: Color,
}

/// Colors for interactive elements and user interface indicators
#[derive(Debug, Clone)]
pub struct InteractiveColors {
    /// The prompt symbol ("> ")
    pub prompt_symbol: Color,
    /// Profile indicator text ("[profile] ")
    pub profile_indicator: Color,
    /// Tangent mode indicator ("↯ ")
    pub tangent_indicator: Color,
    /// Low usage indicator
    pub usage_low: Color,
    /// Medium usage indicator
    pub usage_medium: Color,
    /// High usage indicator
    pub usage_high: Color,
}

impl Default for StatusColors {
    fn default() -> Self {
        Self {
            error: Color::Red,
            warning: Color::Yellow,
            success: Color::Green,
            info: Color::Blue,
        }
    }
}

impl Default for UiColors {
    fn default() -> Self {
        Self {
            primary_brand: Color::AnsiValue(BRAND_PURPLE),
            primary_text: Color::AnsiValue(LIGHT_GREY),
            secondary_text: Color::AnsiValue(MEDIUM_LIGHT_GREY),
            emphasis: Color::AnsiValue(LIGHT_GREY),
            command_highlight: Color::AnsiValue(BRAND_PURPLE),
            current_item: Color::AnsiValue(BRIGHT_MAGENTA),
        }
    }
}

impl Default for InteractiveColors {
    fn default() -> Self {
        Self {
            prompt_symbol: Color::AnsiValue(BRIGHT_MAGENTA),
            profile_indicator: Color::AnsiValue(BRAND_PURPLE),
            tangent_indicator: Color::AnsiValue(BRIGHT_MAGENTA),
            usage_low: Color::Green,
            usage_medium: Color::Yellow,
            usage_high: Color::Red,
        }
    }
}
