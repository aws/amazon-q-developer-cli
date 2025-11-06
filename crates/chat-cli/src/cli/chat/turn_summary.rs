use crossterm::{execute, style};
use std::io::Write;

use crate::cli::chat::conversation::UserTurnMetadata;

pub fn format_number(value: f64) -> String {
    format!("{:.2}", (value * 100.0).floor() / 100.0)
}

pub fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let mut result = first.to_uppercase().collect::<String>();
            result.push_str(chars.as_str());
            result
        }
    }
}

pub fn format_elapsed_time(elapsed_ms: u64) -> String {
    let elapsed_s = elapsed_ms / 1000;
    if elapsed_s < 60 {
        format!("{elapsed_s}s")
    } else {
        let minutes = elapsed_s / 60;
        let seconds = elapsed_s % 60;
        format!("{minutes}m {seconds}s")
    }
}

pub fn display_turn_usage_summary<W: Write>(
    stderr: &mut W,
    user_turn_metadata: &UserTurnMetadata,
) -> Result<(), std::io::Error> {
    let totals = user_turn_metadata.total_usage();
    let mut parts = Vec::new();
    
    // Add usage info
    for usage in totals {
        let formatted = format_number(usage.value);
        let capitalized_unit = capitalize_first(&usage.unit_plural);
        parts.push(format!("{capitalized_unit} used: {formatted}"));
    }
    
    // Add elapsed time
    if let Some(elapsed_ms) = user_turn_metadata.total_elapsed_time_ms() {
        parts.push(format!("Elapsed time: {}", format_elapsed_time(elapsed_ms)));
    }
    
    if !parts.is_empty() {
        execute!(
            stderr,
            style::SetForegroundColor(style::Color::DarkGrey),
            style::Print(format!("{}\n", parts.join("  "))),
            style::ResetColor,
        )?;
    }
    Ok(())
}
