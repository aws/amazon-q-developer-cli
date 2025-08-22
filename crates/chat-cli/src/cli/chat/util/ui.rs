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

pub fn draw_box(
    output: &mut impl Write,
    title: &str,
    content: &str,
    box_width: usize,
    border_color: Color,
) -> Result<()> {
    let inner_width = box_width - 4; // account for │ and padding

    // wrap the single line into multiple lines respecting inner width
    // Manually wrap the text by splitting at word boundaries
    let mut wrapped_lines = Vec::new();
    let mut line = String::new();

    for word in content.split_whitespace() {
        if line.len() + word.len() < inner_width {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        } else {
            // Here we need to account for words that are too long as well
            if word.len() >= inner_width {
                let mut start = 0_usize;
                for (i, _) in word.chars().enumerate() {
                    if i - start >= inner_width {
                        wrapped_lines.push(word[start..i].to_string());
                        start = i;
                    }
                }
                wrapped_lines.push(word[start..].to_string());
                line = String::new();
            } else {
                wrapped_lines.push(line);
                line = word.to_string();
            }
        }
    }

    if !line.is_empty() {
        wrapped_lines.push(line);
    }

    let side_len = (box_width.saturating_sub(title.len())) / 2;
    let top_border = format!(
        "{} {} {}",
        style::style(format!("╭{}", "─".repeat(side_len - 2))).with(border_color),
        title,
        style::style(format!("{}╮", "─".repeat(box_width - side_len - title.len() - 2))).with(border_color)
    );

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

    // Centered wrapped content
    for line in wrapped_lines {
        let visible_line_len = strip_str(&line).len();
        let left_pad = box_width.saturating_sub(4).saturating_sub(visible_line_len) / 2;

        let content = format!(
            "{} {: <pad$}{}{: <rem$} {}",
            style::style("│").with(border_color),
            "",
            line,
            "",
            style::style("│").with(border_color),
            pad = left_pad,
            rem = box_width
                .saturating_sub(4)
                .saturating_sub(left_pad)
                .saturating_sub(visible_line_len),
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
    use crate::cli::chat::colors::ColorManager;
    use crate::cli::chat::GREETING_BREAK_POINT;

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
            ColorManager::default().secondary(),
        )
        .expect("Failed to draw tip box");

        // Test with a longer tip that should wrap
        let long_tip = "This is a much longer tip that should wrap to multiple lines because it exceeds the inner width of the tip box which is calculated based on the GREETING_BREAK_POINT constant";
        draw_box(
            &mut output,
            "Did you know?",
            long_tip,
            GREETING_BREAK_POINT,
            ColorManager::default().secondary(),
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
            ColorManager::default().secondary(),
        )
        .expect("Failed to draw tip box");
        // Test with a long tip with two long words that should wrap
        let long_tip_with_two_long_words = "a".repeat(200);
        draw_box(
            &mut output,
            "Did you know?",
            long_tip_with_two_long_words.as_str(),
            GREETING_BREAK_POINT,
            ColorManager::default().secondary(),
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
