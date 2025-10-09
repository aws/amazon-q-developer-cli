use std::io::Write;

use crossterm::execute;
use crossterm::style::{
    self,
    Attribute,
};
use eyre::Result;

use crate::cli::feed::Feed;
use crate::constants::ui_text;
use crate::theme::StyledText;

/// Render changelog content from feed.json with manual formatting
pub fn render_changelog_content(output: &mut impl Write) -> Result<()> {
    let feed = Feed::load();
    let recent_entries = feed.get_all_changelogs()
        .into_iter()
        .take(2) // Show last 2 releases
        .collect::<Vec<_>>();

    execute!(output, style::Print("\n"))?;

    // Title
    execute!(output, style::Print(ui_text::changelog_header()),)?;

    // Render recent entries
    for entry in recent_entries {
        // Show version header
        execute!(
            output,
            StyledText::info_fg(),
            style::SetAttribute(Attribute::Bold),
            style::Print(format!("## {} ({})\n", entry.version, entry.date)),
            StyledText::reset_attributes(),
            StyledText::reset(),
        )?;

        let mut sorted_changes = entry.changes.clone();
        sorted_changes.sort_by(|a, b| a.change_type.cmp(&b.change_type));

        for change in &sorted_changes {
            let cleaned_description = clean_pr_links(&change.description);
            let processed_description = process_bold_text(&cleaned_description);
            let capitalized_type = capitalize_first_word(&change.change_type);
            execute!(output, style::Print("• ["))?;
            execute!(
                output,
                StyledText::emphasis_fg(),
                style::Print(&capitalized_type),
                StyledText::reset(),
            )?;
            execute!(output, style::Print("] "))?;
            print_with_bold(output, &processed_description)?;
            execute!(output, style::Print("\n"))?;
        }
        execute!(output, style::Print("\n"))?; // Add spacing between versions
    }

    execute!(
        output,
        style::Print("\nRun `/changelog` anytime to see the latest updates and features!\n\n")
    )?;
    Ok(())
}

/// Capitalizes the first character of a string.
fn capitalize_first_word(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let mut result = first.to_uppercase().collect::<String>();
            result.push_str(chars.as_str());
            result
        },
    }
}

/// Removes PR links and numbers from changelog descriptions to improve readability.
///
/// Removes text matching the pattern " - [#NUMBER](URL)" from the end of descriptions.
///
/// Example input: "A new feature - [#2711](https://github.com/aws/amazon-q-developer-cli/pull/2711)"  
/// Example output: "A new feature"
fn clean_pr_links(text: &str) -> String {
    // Remove PR links like " - [#2711](https://github.com/aws/amazon-q-developer-cli/pull/2711)"
    if let Some(pos) = text.find(" - [#") {
        text[..pos].to_string()
    } else {
        text.to_string()
    }
}

/// Processes text to identify **bold** markdown syntax and returns segments with formatting info.
///
/// Returns a vector of tuples where each tuple contains:
/// - `String`: The text segment
/// - `bool`: Whether this segment should be rendered in bold
///
/// Example input: "This is **bold** text"  
/// Example output: [("This is ", false), ("bold", true), (" text", false)]
fn process_bold_text(text: &str) -> Vec<(String, bool)> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_bold = false;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '*' && chars.peek() == Some(&'*') {
            chars.next(); // consume second *
            if !current.is_empty() {
                result.push((current.clone(), in_bold));
                current.clear();
            }
            in_bold = !in_bold;
        } else {
            current.push(ch);
        }
    }

    if !current.is_empty() {
        result.push((current, in_bold));
    }

    result
}

/// Renders text segments with proper bold formatting using crossterm.
///
/// # Arguments
///
/// * `output` - The writer to output formatted text to
/// * `segments` - Vector of (text, is_bold) tuples from `process_bold_text`
///
/// # Errors
///
/// Returns an error if writing to the output fails.
fn print_with_bold(output: &mut impl Write, segments: &[(String, bool)]) -> Result<()> {
    for (text, is_bold) in segments {
        if *is_bold {
            execute!(
                output,
                style::SetAttribute(Attribute::Bold),
                style::Print(text),
                StyledText::reset_attributes(),
            )?;
        } else {
            execute!(output, style::Print(text))?;
        }
    }
    Ok(())
}
