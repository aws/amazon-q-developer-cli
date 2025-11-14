use std::io::Write;

use crossterm::style::{
    Color,
    Stylize,
};
use crossterm::terminal::{
    self,
    ClearType,
};
use crossterm::{
    cursor,
    execute,
    style,
};
use eyre::Result;
use strip_ansi_escapes::strip_str;

pub enum TextAlign {
    Left,
    Center,
}

pub fn draw_box(
    output: &mut impl Write,
    title: &str,
    content: &str,
    box_width: usize,
    border_color: Color,
    align: Option<TextAlign>,
) -> Result<()> {
    let align = align.unwrap_or(TextAlign::Center);
    let inner_width = box_width - 4; // account for │ and padding

    // wrap the single line into multiple lines respecting inner width
    // Manually wrap the text by splitting at word boundaries, using visible length for styled text
    // First split by newlines to preserve explicit line breaks
    let mut wrapped_lines = Vec::new();

    for paragraph in content.split('\n') {
        if paragraph.is_empty() {
            // Preserve empty lines
            wrapped_lines.push(String::new());
            continue;
        }

        let mut line = String::new();

        for word in paragraph.split_whitespace() {
            let test_line = if line.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", line, word)
            };

            let visible_len = strip_str(&test_line).len();

            if visible_len <= inner_width {
                line = test_line;
            } else {
                // Check if the word alone is too long
                let word_visible_len = strip_str(word).len();
                if word_visible_len >= inner_width {
                    // Word is too long, we need to break it (but this is rare with styled text)
                    if !line.is_empty() {
                        wrapped_lines.push(line);
                    }
                    wrapped_lines.push(word.to_string());
                    line = String::new();
                } else {
                    // Start a new line with this word
                    if !line.is_empty() {
                        wrapped_lines.push(line);
                    }
                    line = word.to_string();
                }
            }
        }

        if !line.is_empty() {
            wrapped_lines.push(line);
        }
    }

    let top_border = if title.is_empty() {
        // Closed box with no title
        format!(
            "{}",
            style::style(format!("╭{}╮", "─".repeat(box_width - 2))).with(border_color)
        )
    } else {
        // Box with title
        let side_len = (box_width.saturating_sub(title.len())) / 2;
        format!(
            "{} {} {}",
            style::style(format!("╭{}", "─".repeat(side_len - 2))).with(border_color),
            title,
            style::style(format!("{}╮", "─".repeat(box_width - side_len - title.len() - 2))).with(border_color)
        )
    };

    execute!(
        output,
        terminal::Clear(ClearType::CurrentLine),
        cursor::MoveToColumn(0),
        style::Print(format!("{top_border}\n")),
    )?;

    // Top vertical padding
    let top_vertical_border = format!(
        "{}",
        style::style(format!("│{: <width$}│\n", "", width = box_width - 2)).with(border_color)
    );
    execute!(output, style::Print(top_vertical_border))?;

    // Wrapped content with configurable alignment
    for line in wrapped_lines {
        let visible_line_len = strip_str(&line).len();

        // Calculate padding within the inner content area (box_width - 4)
        // This gives us the padding space available after accounting for borders and minimum padding
        let available_padding = inner_width.saturating_sub(visible_line_len);

        let (left_pad, right_pad) = match align {
            TextAlign::Left => {
                // Left align: 1 space on left, rest on right
                (1, available_padding + 1)
            },
            TextAlign::Center => {
                // Center align: split padding evenly
                let left = available_padding / 2 + 1; // +1 for minimum padding
                let right = available_padding - available_padding / 2 + 1; // +1 for minimum padding
                (left, right)
            },
        };

        let left_padding = " ".repeat(left_pad);
        let right_padding = " ".repeat(right_pad);

        let content = format!(
            "{}{}{}{}{}",
            style::style("│").with(border_color),
            left_padding,
            line,
            right_padding,
            style::style("│").with(border_color),
        );
        execute!(output, style::Print(format!("{}\n", content)))?;
    }

    // Bottom vertical padding
    execute!(
        output,
        style::Print(format!("│{: <width$}│\n", "", width = box_width - 2).with(border_color))
    )?;

    // Bottom rounded corner line: ╰────────────╯
    let bottom = format!("╰{}╯", "─".repeat(box_width - 2)).with(border_color);
    execute!(output, style::Print(format!("{}\n", bottom)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use bstr::ByteSlice;

    use super::*;
    use crate::cli::chat::GREETING_BREAK_POINT;
    use crate::theme::theme;

    #[tokio::test]
    async fn test_draw_tip_box() {
        let mut output = vec![];

        // Test with a short tip
        let short_tip = "This is a short tip";
        draw_box(
            &mut output,
            "Did you know?",
            short_tip,
            GREETING_BREAK_POINT,
            theme().ui.secondary_text,
            None,
        )
        .expect("Failed to draw tip box");

        // Test with a longer tip that should wrap
        let long_tip = "This is a much longer tip that should wrap to multiple lines because it exceeds the inner width of the tip box which is calculated based on the GREETING_BREAK_POINT constant";
        draw_box(
            &mut output,
            "Did you know?",
            long_tip,
            GREETING_BREAK_POINT,
            theme().ui.secondary_text,
            None,
        )
        .expect("Failed to draw tip box");

        // Test with a long tip with two long words that should wrap
        let long_tip_with_one_long_word = {
            let mut s = "a".repeat(200);
            s.push(' ');
            s.push_str(&"a".repeat(200));
            s
        };
        draw_box(
            &mut output,
            "Did you know?",
            long_tip_with_one_long_word.as_str(),
            GREETING_BREAK_POINT,
            theme().ui.secondary_text,
            None,
        )
        .expect("Failed to draw tip box");
        // Test with a long tip with two long words that should wrap
        let long_tip_with_two_long_words = "a".repeat(200);
        draw_box(
            &mut output,
            "Did you know?",
            long_tip_with_two_long_words.as_str(),
            GREETING_BREAK_POINT,
            theme().ui.secondary_text,
            None,
        )
        .expect("Failed to draw tip box");

        // Get the output and verify it contains expected formatting elements
        let output_str = output.to_str_lossy();

        // Check for box drawing characters
        assert!(output_str.contains("╭"), "Output should contain top-left corner");
        assert!(output_str.contains("╮"), "Output should contain top-right corner");
        assert!(output_str.contains("│"), "Output should contain vertical lines");
        assert!(output_str.contains("╰"), "Output should contain bottom-left corner");
        assert!(output_str.contains("╯"), "Output should contain bottom-right corner");

        // Check for the label
        assert!(
            output_str.contains("Did you know?"),
            "Output should contain the 'Did you know?' label"
        );

        // Check that both tips are present
        assert!(output_str.contains(short_tip), "Output should contain the short tip");

        // For the long tip, we check for substrings since it will be wrapped
        let long_tip_parts: Vec<&str> = long_tip.split_whitespace().collect();
        for part in long_tip_parts.iter().take(3) {
            assert!(output_str.contains(part), "Output should contain parts of the long tip");
        }
    }
}
