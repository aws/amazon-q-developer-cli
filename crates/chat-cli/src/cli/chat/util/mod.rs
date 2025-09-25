pub mod images;
pub mod issue;
#[cfg(test)]
pub mod test;
pub mod ui;

use std::io::Write;
use std::time::Duration;

use aws_smithy_types::{
    Document,
    Number as SmithyNumber,
};
use eyre::Result;

use super::ChatError;
use super::token_counter::TokenCounter;

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

/// Truncates `s` to a maximum length of `max_bytes`, appending `suffix` if `s` was truncated. The
/// result is always guaranteed to be at least less than `max_bytes`.
///
/// If `suffix` is larger than `max_bytes`, or `s` is within `max_bytes`, then this function does
/// nothing.
pub fn truncate_safe_in_place(s: &mut String, max_bytes: usize, suffix: &str) {
    // Do nothing if the suffix is too large to be truncated within max_bytes, or s is already small
    // enough to not be truncated.
    if suffix.len() > max_bytes || s.len() <= max_bytes {
        return;
    }

    let end = truncate_safe(s, max_bytes - suffix.len()).len();
    s.replace_range(end..s.len(), suffix);
    s.truncate(max_bytes);
}

pub fn animate_output(output: &mut impl Write, bytes: &[u8]) -> Result<(), ChatError> {
    for b in bytes.chunks(12) {
        output.write_all(b)?;
        std::thread::sleep(Duration::from_millis(16));
    }
    Ok(())
}

/// Returns `true` if the character is from an invisible or control Unicode range
/// that is considered unsafe for LLM input. These rarely appear in normal input,
/// so stripping them is generally safe.
/// The replacement character U+FFFD (�) is preserved to indicate invalid bytes.
fn is_hidden(c: char) -> bool {
    match c {
        '\u{E0000}'..='\u{E007F}' |     // TAG characters (used for hidden prompts)  
        '\u{200B}'..='\u{200F}'  |      // zero-width space, ZWJ, ZWNJ, RTL/LTR marks  
        '\u{2028}'..='\u{202F}'  |      // line / paragraph separators, narrow NB-SP  
        '\u{205F}'..='\u{206F}'  |      // format control characters  
        '\u{FFF0}'..='\u{FFFC}'  |
        '\u{FFFE}'..='\u{FFFF}'   // Specials block (non-characters) 
        => true,
        _ => false,
    }
}

/// Remove hidden / control characters from `text`.
///
/// * `text`   –  raw user input or file content
///
/// The function keeps things **O(n)** with a single allocation and logs how many
/// characters were dropped. 400 KB worst-case size ⇒ sub-millisecond runtime.
pub fn sanitize_unicode_tags(text: &str) -> String {
    let mut removed = 0;
    let out: String = text
        .chars()
        .filter(|&c| {
            let bad = is_hidden(c);
            if bad {
                removed += 1;
            }
            !bad
        })
        .collect();

    if removed > 0 {
        tracing::debug!("Detected and removed {} hidden chars", removed);
    }
    out
}

/// Send terminal notification (OSC 9 or bell fallback)
pub fn play_notification_bell(requires_confirmation: bool) {
    let message = if requires_confirmation {
        "Amazon Q needs your attention"
    } else {
        "Amazon Q has completed its work"
    };

    send_terminal_notification(message);
}

/// Send notification using OSC 9 or bell fallback
fn send_terminal_notification(message: &str) {
    if supports_osc_notifications() {
        // OSC 9 notification (iTerm2, Ghostty, etc.)
        print!("\x1b]9;{}\x07", message);
        std::io::stdout().flush().unwrap();
    } else if should_play_bell() {
        // Fallback to ASCII bell
        print!("\x07");
        std::io::stdout().flush().unwrap();
    }
}

/// Check if terminal supports OSC 9 notifications
fn supports_osc_notifications() -> bool {
    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        match term_program.as_str() {
            "iTerm.app" | "ghostty" => return true,
            _ => {}
        }
    }
    
    if let Ok(term) = std::env::var("TERM") {
        match term.as_str() {
            "ghostty" => return true,
            _ => {}
        }
    }
    
    false
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
            "ghostty",
            "eat-truecolor",
            "eat-256color",
            "eat-color",
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

/// This is a simple greedy algorithm that drops the largest files first
/// until the total size is below the limit
///
/// # Arguments
/// * `files` - A mutable reference to a vector of tuples: (filename, content). This file will be
///   sorted but the content will not be changed.
///
/// Returns the dropped files
pub fn drop_matched_context_files(files: &mut [(String, String)], limit: usize) -> Result<Vec<(String, String)>> {
    files.sort_by(|a, b| TokenCounter::count_tokens(&b.1).cmp(&TokenCounter::count_tokens(&a.1)));
    let mut total_size = 0;
    let mut dropped_files = Vec::new();

    for (filename, content) in files.iter() {
        let size = TokenCounter::count_tokens(content);
        if total_size + size > limit {
            dropped_files.push((filename.clone(), content.clone()));
        } else {
            total_size += size;
        }
    }
    Ok(dropped_files)
}

pub fn serde_value_to_document(value: serde_json::Value) -> Document {
    match value {
        serde_json::Value::Null => Document::Null,
        serde_json::Value::Bool(bool) => Document::Bool(bool),
        serde_json::Value::Number(number) => {
            if let Some(num) = number.as_u64() {
                Document::Number(SmithyNumber::PosInt(num))
            } else if number.as_i64().is_some_and(|n| n < 0) {
                Document::Number(SmithyNumber::NegInt(number.as_i64().unwrap()))
            } else {
                Document::Number(SmithyNumber::Float(number.as_f64().unwrap_or_default()))
            }
        },
        serde_json::Value::String(string) => Document::String(string),
        serde_json::Value::Array(vec) => {
            Document::Array(vec.clone().into_iter().map(serde_value_to_document).collect::<_>())
        },
        serde_json::Value::Object(map) => Document::Object(
            map.into_iter()
                .map(|(k, v)| (k, serde_value_to_document(v)))
                .collect::<_>(),
        ),
    }
}

pub fn document_to_serde_value(value: Document) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Document::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, document_to_serde_value(v)))
                .collect::<_>(),
        ),
        Document::Array(vec) => Value::Array(vec.clone().into_iter().map(document_to_serde_value).collect::<_>()),
        Document::Number(number) => {
            if let Ok(v) = TryInto::<u64>::try_into(number) {
                Value::Number(v.into())
            } else if let Ok(v) = TryInto::<i64>::try_into(number) {
                Value::Number(v.into())
            } else {
                Value::Number(
                    serde_json::Number::from_f64(number.to_f64_lossy())
                        .unwrap_or(serde_json::Number::from_f64(0.0).expect("converting from 0.0 will not fail")),
                )
            }
        },
        Document::String(s) => serde_json::Value::String(s),
        Document::Bool(b) => serde_json::Value::Bool(b),
        Document::Null => serde_json::Value::Null,
    }
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

    #[test]
    fn test_truncate_safe_in_place() {
        let suffix = "suffix";
        let tests = &[
            ("Hello World", 5, "Hello World"),
            ("Hello World", 7, "Hsuffix"),
            ("Hello World", usize::MAX, "Hello World"),
            // α -> 2 byte length
            ("αααααα", 7, "suffix"),
            ("αααααα", 8, "αsuffix"),
            ("αααααα", 9, "αsuffix"),
        ];
        assert!("α".len() == 2);

        for (input, max_bytes, expected) in tests {
            let mut input = (*input).to_string();
            truncate_safe_in_place(&mut input, *max_bytes, suffix);
            assert_eq!(
                input.as_str(),
                *expected,
                "input: {} with max bytes: {} failed",
                input,
                max_bytes
            );
        }
    }

    #[test]
    fn test_drop_matched_context_files() {
        let mut files = vec![
            ("file1".to_string(), "This is a test file".to_string()),
            (
                "file3".to_string(),
                "Yet another test file that's has the largest context file".to_string(),
            ),
        ];
        let limit = 9;

        let dropped_files = drop_matched_context_files(&mut files, limit).unwrap();
        assert_eq!(dropped_files.len(), 1);
        assert_eq!(dropped_files[0].0, "file3");
        assert_eq!(files.len(), 2);

        for (filename, _) in dropped_files.iter() {
            files.retain(|(f, _)| f != filename);
        }
        assert_eq!(files.len(), 1);
    }
    #[test]
    fn is_hidden_recognises_all_ranges() {
        let samples = ['\u{E0000}', '\u{200B}', '\u{2028}', '\u{205F}', '\u{FFF0}'];

        for ch in samples {
            assert!(is_hidden(ch), "char U+{:X} should be hidden", ch as u32);
        }

        for ch in ['a', '你', '\u{03A9}'] {
            assert!(!is_hidden(ch), "char {:?} should NOT be hidden", ch);
        }
    }

    #[test]
    fn sanitize_keeps_visible_text_intact() {
        let visible = "Rust 🦀 > C";
        assert_eq!(sanitize_unicode_tags(visible), visible);
    }

    #[test]
    fn sanitize_handles_large_mixture() {
        let visible_block = "abcXYZ";
        let hidden_block = "\u{200B}\u{E0000}";
        let mut big_input = String::new();
        for _ in 0..50_000 {
            big_input.push_str(visible_block);
            big_input.push_str(hidden_block);
        }

        let result = sanitize_unicode_tags(&big_input);

        assert_eq!(result.len(), 50_000 * visible_block.len());

        assert!(result.chars().all(|c| !is_hidden(c)));
    }
}
