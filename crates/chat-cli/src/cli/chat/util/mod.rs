pub mod images;
pub mod issue;
pub mod shared_writer;
pub mod ui;

use std::io::Write;
use std::time::Duration;

use super::ChatError;
use crate::util::system_info::in_cloudshell;

const GOV_REGIONS: &[&str] = &["us-gov-east-1", "us-gov-west-1"];

pub fn region_check(capability: &'static str) -> eyre::Result<()> {
    let Ok(region) = std::env::var("AWS_REGION") else {
        return Ok(());
    };

    if in_cloudshell() && GOV_REGIONS.contains(&region.as_str()) {
        eyre::bail!("AWS GovCloud ({region}) is not supported for {capability}.");
    }

    Ok(())
}

pub fn truncate_safe(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    let mut byte_count = 0;
    let mut char_indices = s.char_indices();

    for (byte_idx, _) in &mut char_indices {
        if byte_count + (byte_idx - byte_count) > max_bytes {
            break;
        }
        byte_count = byte_idx;
    }

    &s[..byte_count]
}

pub fn animate_output(output: &mut impl Write, bytes: &[u8]) -> Result<(), ChatError> {
    for b in bytes.chunks(12) {
        output.write_all(b)?;
        std::thread::sleep(Duration::from_millis(16));
    }
    Ok(())
}

/// Play the terminal bell notification sound
pub fn play_notification_bell(requires_confirmation: bool) {
    // Don't play bell for tools that don't require confirmation
    if !requires_confirmation {
        return;
    }

    // Check if we should play the bell based on terminal type
    if should_play_bell() {
        print!("\x07"); // ASCII bell character
        std::io::stdout().flush().unwrap();
    }
}

/// Determine if we should play the bell based on terminal type
fn should_play_bell() -> bool {
    // Get the TERM environment variable
    if let Ok(term) = std::env::var("TERM") {
        // List of terminals known to handle bell character well
        let bell_compatible_terms = [
            "xterm",
            "xterm-256color",
            "screen",
            "screen-256color",
            "tmux",
            "tmux-256color",
            "rxvt",
            "rxvt-unicode",
            "linux",
            "konsole",
            "gnome",
            "gnome-256color",
            "alacritty",
            "iterm2",
        ];

        // Check if the current terminal is in the compatible list
        for compatible_term in bell_compatible_terms.iter() {
            if term.starts_with(compatible_term) {
                return true;
            }
        }

        // For other terminals, don't play the bell
        return false;
    }

    // If TERM is not set, default to not playing the bell
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_safe() {
        assert_eq!(truncate_safe("Hello World", 5), "Hello");
        assert_eq!(truncate_safe("Hello ", 5), "Hello");
        assert_eq!(truncate_safe("Hello World", 11), "Hello World");
        assert_eq!(truncate_safe("Hello World", 15), "Hello World");
    }
}
