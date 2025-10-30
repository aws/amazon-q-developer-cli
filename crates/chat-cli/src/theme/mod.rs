//! Centralized theme system for Amazon Q CLI colors
//!
//! This module provides a unified color management system that replaces inline color
//! definitions throughout the codebase with semantic theme references. The theme system
//! maintains backward compatibility with existing color systems (color_print, crossterm)
//! while providing a consistent API for color usage.

pub mod colors;
pub mod crossterm_ext;

use std::sync::LazyLock;

pub use colors::*;
pub use crossterm_ext::*;

/// ANSI color value for the primary brand color (purple)
pub const BRAND_COLOR_ANSI: u8 = 141;

/// ANSI color value for the secondary text color (muted)
#[cfg(unix)]
pub const SECONDARY_COLOR_ANSI: u8 = 248;

/// Main theme configuration containing all color categories
#[derive(Debug, Clone)]
pub struct Theme {
    /// Colors for status messages (error, warning, success, info)
    pub status: StatusColors,
    /// Colors for UI elements (branding, text, links, etc.)
    pub ui: UiColors,
    /// Colors for interactive elements (prompts, indicators, etc.)
    pub interactive: InteractiveColors,
}

/// Global theme instance available throughout the application
pub static DEFAULT_THEME: LazyLock<Theme> = LazyLock::new(Theme::default);

/// Get a reference to the global theme instance
pub fn theme() -> &'static Theme {
    &DEFAULT_THEME
}

impl Default for Theme {
    /// Creates the default theme with colors matching the current CLI appearance
    fn default() -> Self {
        Self {
            status: StatusColors::default(),
            ui: UiColors::default(),
            interactive: InteractiveColors::default(),
        }
    }
}
