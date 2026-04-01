use std::sync::OnceLock;

use unicode_width::UnicodeWidthChar;

use crate::constants::{
    DEFAULT_AGENT_NAME,
    HELP_AGENT_NAME,
    PLANNER_AGENT_NAME,
};

/// Extra columns the terminal uses beyond what `unicode-width` predicts for
/// each special prompt character. Measured once at startup via DSR (Device
/// Status Report). A positive value means the terminal renders the character
/// wider than `unicode-width` thinks.
#[derive(Debug, Clone, Copy, Default)]
pub struct PromptWidthCorrection {
    pub lambda: i8,
    pub zigzag: i8,
}

static WIDTH_CORRECTION: OnceLock<PromptWidthCorrection> = OnceLock::new();

/// Measure the actual terminal rendering width of a character using DSR
/// (`\x1b[6n`). Returns `None` if the terminal doesn't respond or we're not
/// on a TTY.
#[cfg(unix)]
fn measure_char_width_dsr(ch: char) -> Option<usize> {
    use std::io::Write;
    use std::os::unix::io::AsRawFd;

    let tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    let fd = tty.as_raw_fd();

    // Save original terminal settings
    let mut original: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &mut original) } != 0 {
        return None;
    }
    let mut raw = original;
    unsafe { libc::cfmakeraw(&mut raw) };
    // Set a read timeout so we don't block forever if the terminal doesn't
    // respond to DSR. VMIN=0, VTIME=5 means read() returns after 0.5s even
    // if no data arrives.
    raw.c_cc[libc::VMIN] = 0;
    raw.c_cc[libc::VTIME] = 5;
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
        return None;
    }

    let result = (|| -> Option<usize> {
        let mut tty_w = &tty;
        let mut tty_r = &tty;

        // Move to column 0, query baseline position
        tty_w.write_all(b"\r\x1b[6n").ok()?;
        tty_w.flush().ok()?;
        let base_col = read_cursor_col(&mut tty_r)?;

        // Write the character and query again
        let mut buf = [0u8; 4];
        let encoded = ch.encode_utf8(&mut buf);
        tty_w.write_all(encoded.as_bytes()).ok()?;
        tty_w.write_all(b"\x1b[6n").ok()?;
        tty_w.flush().ok()?;
        let after_col = read_cursor_col(&mut tty_r)?;

        // Clean up: move back and erase the character
        tty_w.write_all(b"\r\x1b[K").ok()?;
        tty_w.flush().ok()?;

        Some(after_col.saturating_sub(base_col))
    })();

    // Restore original terminal settings
    unsafe { libc::tcsetattr(fd, libc::TCSANOW, &original) };

    result
}

/// Parse the column from a DSR response `\x1b[row;colR`.
#[cfg(unix)]
fn read_cursor_col(reader: &mut impl std::io::Read) -> Option<usize> {
    let mut buf = [0u8; 1];
    let mut response = Vec::with_capacity(16);

    loop {
        match reader.read(&mut buf) {
            Ok(0) => return None, // timeout (VMIN=0, VTIME expired)
            Ok(_) => {
                response.push(buf[0]);
                if buf[0] == b'R' {
                    break;
                }
            },
            Err(_) => return None,
        }
    }

    // Parse \x1b[row;colR
    let s = std::str::from_utf8(&response).ok()?;
    let inner = s.strip_prefix("\x1b[")?;
    let inner = inner.strip_suffix('R')?;
    let (_, col_str) = inner.split_once(';')?;
    col_str.parse::<usize>().ok()
}

#[cfg(not(unix))]
fn measure_char_width_dsr(_ch: char) -> Option<usize> {
    None
}

/// Measure the width correction for a character: actual terminal width minus
/// what `unicode-width` reports.
fn char_width_correction(ch: char) -> i8 {
    let unicode_width = UnicodeWidthChar::width(ch).unwrap_or(0);
    match measure_char_width_dsr(ch) {
        Some(actual) => (actual as i8) - (unicode_width as i8),
        None => 0,
    }
}

/// Probe the terminal once and cache the result. Safe to call multiple times.
pub fn init_prompt_width_correction() {
    WIDTH_CORRECTION.get_or_init(|| {
        if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            return PromptWidthCorrection::default();
        }
        let correction = PromptWidthCorrection {
            lambda: char_width_correction('λ'),
            zigzag: char_width_correction('↯'),
        };
        if correction.lambda != 0 || correction.zigzag != 0 {
            tracing::info!(
                lambda = correction.lambda,
                zigzag = correction.zigzag,
                "Detected prompt character width mismatch — padding prompt to compensate"
            );
        }
        correction
    });
}

/// Return the cached correction (falls back to zero if not yet measured).
pub fn prompt_width_correction() -> PromptWidthCorrection {
    WIDTH_CORRECTION.get().copied().unwrap_or_default()
}

/// Components extracted from a prompt string
#[derive(Debug, PartialEq)]
pub struct PromptComponents {
    pub delegate_notifier: Option<String>,
    pub profile: Option<String>,
    pub warning: bool,
    pub tangent_mode: bool,
    pub code_intelligence: bool,
    pub usage_percentage: Option<f32>,
}

/// Parse prompt components from a plain text prompt
pub fn parse_prompt_components(prompt: &str) -> Option<PromptComponents> {
    // Expected format: "[agent] 6% λ ↯ !> " or "> " or "!> " or "[agent] ↯ > " or "6% ↯ > " etc.
    let mut delegate_notifier = None::<String>;
    let mut profile = None;
    let mut warning = false;
    let mut tangent_mode = false;
    let mut code_intelligence = false;
    let mut usage_percentage = None;

    // Check if multi-line prompt (e.g., with rich notification)
    // Everything before the last line is treated as delegate_notifier
    let remaining = if prompt.contains('\n') {
        let lines: Vec<&str> = prompt.lines().collect();
        if lines.len() > 1 {
            // Everything except last line is the notification
            delegate_notifier = Some(lines[..lines.len() - 1].join("\n"));
        }
        // Parse only the last line for prompt components
        lines.last().unwrap_or(&"").trim()
    } else {
        prompt.trim()
    };

    let mut remaining = remaining;

    // Check for agent pattern [agent] first
    if let Some(start) = remaining.find('[')
        && let Some(end) = remaining.find(']')
        && start < end
    {
        let content = &remaining[start + 1..end];
        profile = Some(content.to_string());
        remaining = remaining[end + 1..].trim_start();
    }

    // Check for percentage pattern (e.g., "6% ")
    if let Some(percent_pos) = remaining.find('%') {
        let before_percent = &remaining[..percent_pos];
        if let Ok(percentage) = before_percent.trim().parse::<f32>() {
            usage_percentage = Some(percentage);
            if let Some(space_after_percent) = remaining[percent_pos..].find(' ') {
                remaining = remaining[percent_pos + space_after_percent + 1..].trim_start();
            }
        }
    }

    // Check for code intelligence symbol ƒ first
    if let Some(after_code) = remaining.strip_prefix('λ') {
        code_intelligence = true;
        remaining = after_code.trim_start();
    }

    // Check for tangent mode ↯
    if let Some(after_tangent) = remaining.strip_prefix('↯') {
        tangent_mode = true;
        remaining = after_tangent.trim_start();
    }

    // Check for warning symbol ! (comes after tangent mode)
    if remaining.starts_with('!') {
        warning = true;
        remaining = remaining[1..].trim_start();
    }

    // Should end with "> " for both normal and tangent mode
    if remaining.trim_end() == ">" {
        Some(PromptComponents {
            delegate_notifier,
            profile,
            warning,
            tangent_mode,
            code_intelligence,
            usage_percentage,
        })
    } else {
        None
    }
}

pub fn generate_prompt(
    current_profile: Option<&str>,
    warning: bool,
    tangent_mode: bool,
    code_intelligence: bool,
    usage_percentage: Option<f32>,
) -> String {
    // Generate plain text prompt that will be colored by highlight_prompt.
    //
    // Padding compensates for characters whose actual terminal rendering width
    // differs from what rustyline's unicode-width calculation predicts (e.g. λ
    // rendered as 2 columns in CJK terminals). highlight_prompt reconstructs
    // the prompt from parsed components and ignores extra spaces, so the
    // padding is invisible on screen but makes rustyline's prompt_size correct.
    let correction = prompt_width_correction();
    let mut padding: usize = 0;

    let warning_symbol = if warning { "!" } else { "" };
    let profile_part = current_profile
        .filter(|&p| p != DEFAULT_AGENT_NAME)
        .map(|p| {
            if p == PLANNER_AGENT_NAME {
                "[plan] ".to_string()
            } else if p == HELP_AGENT_NAME {
                "[help] ".to_string()
            } else {
                format!("[{p}] ")
            }
        })
        .unwrap_or_default();

    let percentage_part = usage_percentage.map(|p| format!("{p:.0}% ")).unwrap_or_default();

    let code_intel_symbol = if code_intelligence {
        if correction.lambda > 0 {
            padding += correction.lambda as usize;
        }
        "λ "
    } else {
        ""
    };
    let tangent_symbol = if tangent_mode {
        if correction.zigzag > 0 {
            padding += correction.zigzag as usize;
        }
        "↯ "
    } else {
        ""
    };

    // Insert padding spaces right before "> " so parse_prompt_components
    // absorbs them via trim_start() / trim_end() during round-trip.
    let pad = " ".repeat(padding);
    format!("{profile_part}{percentage_part}{code_intel_symbol}{tangent_symbol}{warning_symbol}{pad}> ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_prompt() {
        // Test default prompt (no profile)
        assert_eq!(generate_prompt(None, false, false, false, None), "> ");
        // Test default prompt with warning
        assert_eq!(generate_prompt(None, true, false, false, None), "!> ");
        // Test tangent mode
        assert_eq!(generate_prompt(None, false, true, false, None), "↯ > ");
        // Test tangent mode with warning
        assert_eq!(generate_prompt(None, true, true, false, None), "↯ !> ");
        // Test default profile (should be same as no profile)
        assert_eq!(
            generate_prompt(Some(DEFAULT_AGENT_NAME), false, false, false, None),
            "> "
        );
        // Test custom profile
        assert_eq!(
            generate_prompt(Some("test-profile"), false, false, false, None),
            "[test-profile] > "
        );
        // Test custom profile with tangent mode
        assert_eq!(
            generate_prompt(Some("test-profile"), false, true, false, None),
            "[test-profile] ↯ > "
        );
        // Test another custom profile with warning
        assert_eq!(generate_prompt(Some("dev"), true, false, false, None), "[dev] !> ");
        // Test custom profile with warning and tangent mode
        assert_eq!(generate_prompt(Some("dev"), true, true, false, None), "[dev] ↯ !> ");
        // Test custom profile with usage percentage
        assert_eq!(
            generate_prompt(Some("rust-agent"), false, false, false, Some(6.2)),
            "[rust-agent] 6% > "
        );
        // Test custom profile with usage percentage and warning
        assert_eq!(
            generate_prompt(Some("rust-agent"), true, false, false, Some(15.7)),
            "[rust-agent] 16% !> "
        );
        // Test usage percentage without profile
        assert_eq!(generate_prompt(None, false, false, false, Some(25.3)), "25% > ");
        // Test usage percentage with tangent mode
        assert_eq!(generate_prompt(None, false, true, false, Some(8.9)), "9% ↯ > ");
    }

    #[test]
    fn test_parse_prompt_components() {
        // Test basic prompt
        let components = parse_prompt_components("> ").unwrap();
        assert!(components.profile.is_none());
        assert!(!components.warning);
        assert!(!components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test warning prompt
        let components = parse_prompt_components("!> ").unwrap();
        assert!(components.profile.is_none());
        assert!(components.warning);
        assert!(!components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test tangent mode
        let components = parse_prompt_components("↯ > ").unwrap();
        assert!(components.profile.is_none());
        assert!(!components.warning);
        assert!(components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test tangent mode with warning
        let components = parse_prompt_components("↯ !> ").unwrap();
        assert!(components.profile.is_none());
        assert!(components.warning);
        assert!(components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test profile prompt
        let components = parse_prompt_components("[test] > ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("test"));
        assert!(!components.warning);
        assert!(!components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test profile with warning
        let components = parse_prompt_components("[dev] !> ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("dev"));
        assert!(components.warning);
        assert!(!components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test profile with tangent mode
        let components = parse_prompt_components("[dev] ↯ > ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("dev"));
        assert!(!components.warning);
        assert!(components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test profile with warning and tangent mode
        let components = parse_prompt_components("[dev] ↯ !> ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("dev"));
        assert!(components.warning);
        assert!(components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test prompts with percentages
        let components = parse_prompt_components("[rust-agent] 6% > ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("rust-agent"));
        assert!(!components.warning);
        assert!(!components.tangent_mode);
        assert_eq!(components.usage_percentage, Some(6.0));

        let components = parse_prompt_components("25% > ").unwrap();
        assert!(components.profile.is_none());
        assert!(!components.warning);
        assert!(!components.tangent_mode);
        assert_eq!(components.usage_percentage, Some(25.0));

        let components = parse_prompt_components("8% ↯ > ").unwrap();
        assert!(components.profile.is_none());
        assert!(!components.warning);
        assert!(components.tangent_mode);
        assert_eq!(components.usage_percentage, Some(8.0));

        // Test invalid prompt
        assert!(parse_prompt_components("invalid").is_none());
    }
}
