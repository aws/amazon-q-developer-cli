pub mod clipboard;
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
use super::context::ContextFile;
use crate::util::env_var::get_term;

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
    if let Some(term) = get_term() {
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
/// * `files` - A mutable reference to a vector of ContextFile. This vector will be sorted by size
///   and modified to remove dropped files.
///
/// Returns the dropped files
pub fn drop_matched_context_files(files: &mut Vec<ContextFile>, limit: usize) -> Result<Vec<ContextFile>> {
    // Sort by size (largest first)
    files.sort_by_key(|b| std::cmp::Reverse(b.size()));

    let mut total_size = 0;
    let mut dropped_files = Vec::new();

    for file in files.iter() {
        let size = file.size();
        if total_size + size > limit {
            dropped_files.push(file.clone());
        } else {
            total_size += size;
        }
    }

    // Remove dropped files from the original vector
    files.retain(|f| !dropped_files.iter().any(|d| d.filepath() == f.filepath()));

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
                "input: {input} with max bytes: {max_bytes} failed"
            );
        }
    }

    #[test]
    fn test_drop_matched_context_files() {
        use crate::cli::chat::context::ContextFile;

        let mut files = vec![
            ContextFile::Full {
                filepath: "file1".to_string(),
                content: "This is a test file".to_string(),
            },
            ContextFile::Full {
                filepath: "file3".to_string(),
                content: "Yet another test file that's has the largest context file".to_string(),
            },
        ];
        let limit = 9;

        let dropped_files = drop_matched_context_files(&mut files, limit).unwrap();
        assert_eq!(dropped_files.len(), 1);

        if let ContextFile::Full { filepath, .. } = &dropped_files[0] {
            assert_eq!(filepath, "file3");
        } else {
            panic!("Expected Full file");
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
            assert!(!is_hidden(ch), "char {ch:?} should NOT be hidden");
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
