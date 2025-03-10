use fig_os_shim::Context;
use similar::{
    ChangeTag,
    TextDiff,
};
use syntect::util::as_24_bit_terminal_escaped;
use tracing::{
    error,
    warn,
};

use crate::util::THEME_SET;

/// Stylizes the diff between two strings with proper line numbers.
///
/// When used with `str_replace`, the `prev` should be the old string content and `new` should be
/// the new string content. The function will generate a diff that shows line numbers from the
/// original file context.
///
/// # Arguments
/// * `ctx` - The context for environment information
/// * `prev` - The previous/old string content
/// * `new` - The new string content that will replace the old content
/// * `start_line_number` - Optional starting line number for context (default: 1)
///
/// # Returns
/// A formatted string showing the diff with line numbers
pub fn stylize_diff(ctx: &Context, prev: &String, new: &String, start_line_number: Option<usize>) -> String {
    // Check if 24-bit color is supported
    let use_24bit_color = match ctx.env().get("COLORTERM") {
        Ok(s) if s == "truecolor" => true,
        _ => {
            warn!("24bit color is not supported, falling back to nonstylized diff");
            false
        },
    };

    if !use_24bit_color {
        // Fallback to simple diff without styling
        return nonstylized_diff(prev, new, start_line_number);
    }

    let ts = &*THEME_SET;
    let theme = &ts.themes["base16-ocean.dark"];

    // Get theme colors for diff styling
    let (delete_fg, insert_fg, equal_fg, bg) = match (theme.settings.foreground, theme.settings.background) {
        (Some(fg), Some(bg)) => {
            // Use theme-based colors for consistency with stylized_file
            let delete_color = syntect::highlighting::Color {
                r: 224,
                g: 108,
                b: 117,
                a: 255, // Red-ish color for deletions
            };
            let insert_color = syntect::highlighting::Color {
                r: 152,
                g: 195,
                b: 121,
                a: 255, // Green-ish color for insertions
            };
            (delete_color, insert_color, fg, bg)
        },
        _ => {
            error!("missing theme colors, falling back to nonstylized diff");
            return nonstylized_diff(prev, new, start_line_number);
        },
    };

    // Define styles for different change types
    let delete_style = syntect::highlighting::Style {
        foreground: delete_fg,
        background: bg,
        font_style: syntect::highlighting::FontStyle::default(),
    };

    let insert_style = syntect::highlighting::Style {
        foreground: insert_fg,
        background: bg,
        font_style: syntect::highlighting::FontStyle::default(),
    };

    let equal_style = syntect::highlighting::Style {
        foreground: equal_fg,
        background: bg,
        font_style: syntect::highlighting::FontStyle::default(),
    };

    let gutter_style = syntect::highlighting::Style {
        foreground: equal_fg,
        background: bg,
        font_style: syntect::highlighting::FontStyle::default(),
    };

    let mut updates = String::new();

    // Get the starting line number (default to 1 if not provided)
    let line_offset = start_line_number.unwrap_or(1).saturating_sub(1);

    // We'll use the original strings directly and handle line numbers in the stylize_hunk function
    // Ensure both strings end with a newline to avoid layout issues in the diff
    let adjusted_prev = if prev.ends_with('\n') {
        prev.clone()
    } else {
        prev.clone() + "\n"
    };
    let adjusted_new = if new.ends_with('\n') {
        new.clone()
    } else {
        new.clone() + "\n"
    };

    let diff = TextDiff::configure().diff_lines(&adjusted_prev, &adjusted_new);

    // Process each hunk and collect the styled output
    for hunk in diff
        .unified_diff()
        .context_radius(5)
        .missing_newline_hint(false) // Disable the "No newline" message
        .iter_hunks()
    {
        updates.push_str(&stylize_hunk(
            &hunk,
            delete_style,
            insert_style,
            equal_style,
            gutter_style,
            line_offset, // Pass the line_offset to adjust line numbers
        ));
    }

    updates
}

// This function modifies our hunk to color the lines in our hunk using our style
fn stylize_hunk<'a>(
    hunk: &similar::udiff::UnifiedDiffHunk<'a, 'a, 'a, 'a, str>,
    delete_style: syntect::highlighting::Style,
    insert_style: syntect::highlighting::Style,
    equal_style: syntect::highlighting::Style,
    gutter_style: syntect::highlighting::Style,
    line_offset: usize,
) -> String {
    let mut result = String::new();

    // Process each line in the hunk
    for change in hunk.iter_changes() {
        let (prefix, style, line_num) = match change.tag() {
            ChangeTag::Delete => {
                let idx = change.old_index().unwrap();
                // Add the line_offset to adjust the displayed line number
                ("-", delete_style, format!("{:<4} ", idx + 1 + line_offset))
            },
            ChangeTag::Insert => {
                let idx = change.new_index().unwrap();
                // Add the line_offset to adjust the displayed line number
                ("+", insert_style, format!("{:<4} ", idx + 1 + line_offset))
            },
            ChangeTag::Equal => {
                let old_idx = change.old_index().unwrap();
                // Add the line_offset to adjust the displayed line number
                (" ", equal_style, format!("{:<4} ", old_idx + 1 + line_offset))
            },
        };

        // Style the line number with gutter style
        result.push_str(&as_24_bit_terminal_escaped(&[(gutter_style, &line_num)], true));

        // Style the prefix and content with the appropriate style
        let prefix_str = prefix;
        let content = change.value();

        // Style the prefix and content inline
        result.push_str(&as_24_bit_terminal_escaped(&[(style, prefix_str)], true));
        result.push_str(&as_24_bit_terminal_escaped(&[(style, content)], true));
    }

    result
}

/// Fallback function for when 24-bit color is not available
fn nonstylized_diff(prev: &String, new: &String, start_line_number: Option<usize>) -> String {
    // Get the starting line number (default to 1 if not provided)
    let line_offset = start_line_number.unwrap_or(1).saturating_sub(1);

    // Use the original strings directly
    // Ensure both strings end with a newline to avoid layout issues in the diff
    let adjusted_prev = if prev.ends_with('\n') {
        prev.clone()
    } else {
        prev.clone() + "\n"
    };
    let adjusted_new = if new.ends_with('\n') {
        new.clone()
    } else {
        new.clone() + "\n"
    };

    let diff = TextDiff::configure().diff_lines(&adjusted_prev, &adjusted_new);

    let mut updates = String::new();
    updates.push('\n');

    // Track the current line numbers for both old and new text
    // We don't need to add line_offset here since it's already in the input
    let mut old_line_num = 1;
    let mut new_line_num = 1;

    for (idx, group) in diff.grouped_ops(5).iter().enumerate() {
        if idx > 0 {
            updates.push_str(&format!("{:-^1$}\n", "-", 80));
        }
        for op in group {
            for change in diff.iter_inline_changes(op) {
                // Update line numbers based on the change type
                let sign = match change.tag() {
                    ChangeTag::Delete => {
                        old_line_num += 1;
                        "-"
                    },
                    ChangeTag::Insert => {
                        new_line_num += 1;
                        "+"
                    },
                    ChangeTag::Equal => {
                        old_line_num += 1;
                        new_line_num += 1;
                        " "
                    },
                };

                // Format line numbers in gutter
                let old_display = if change.tag() == ChangeTag::Insert {
                    "    ".to_string()
                } else {
                    format!("{:<4}", old_line_num + line_offset - 1)
                };

                let new_display = if change.tag() == ChangeTag::Delete {
                    "    ".to_string()
                } else {
                    format!("{:<4}", new_line_num + line_offset - 1)
                };

                updates.push_str(&format!("{} {} |{}", old_display, new_display, sign));

                for (_, value) in change.iter_strings_lossy() {
                    updates.push_str(value.as_ref());
                }
            }
        }
    }

    updates
}
