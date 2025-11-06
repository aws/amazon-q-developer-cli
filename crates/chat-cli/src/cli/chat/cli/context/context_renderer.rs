//! Rendering for context window information

use crossterm::style::Attribute;
use crossterm::{
    execute,
    queue,
    style,
};

use super::context_data_provider::ContextWindowData;
use crate::cli::chat::token_counter::TokenCount;
use crate::cli::chat::{
    ChatError,
    ChatSession,
};
use crate::theme::StyledText;

/// Calculate context percentage from token counts (private utility)
fn calculate_context_percentage(tokens: TokenCount, context_window_size: usize) -> f32 {
    (tokens.value() as f32 / context_window_size as f32) * 100.0
}

/// Render context window information section
pub async fn render_context_window(
    context_data: &ContextWindowData,
    session: &mut ChatSession,
) -> Result<(), ChatError> {
    if !context_data.dropped_context_files.is_empty() {
        execute!(
            session.stderr,
            StyledText::warning_fg(),
            style::Print("\nSome context files are dropped due to size limit, please run "),
            StyledText::success_fg(),
            style::Print("/context show "),
            StyledText::warning_fg(),
            style::Print("to learn more.\n"),
            StyledText::reset(),
        )?;
    }

    let window_width = session.terminal_width();
    // set a max width for the progress bar for better aesthetic
    let progress_bar_width = std::cmp::min(window_width, 80);

    let context_width = ((context_data.context_tokens.value() as f64 / context_data.context_window_size as f64)
        * progress_bar_width as f64) as usize;
    let assistant_width = ((context_data.assistant_tokens.value() as f64 / context_data.context_window_size as f64)
        * progress_bar_width as f64) as usize;
    let tools_width = ((context_data.tools_tokens.value() as f64 / context_data.context_window_size as f64)
        * progress_bar_width as f64) as usize;
    let user_width = ((context_data.user_tokens.value() as f64 / context_data.context_window_size as f64)
        * progress_bar_width as f64) as usize;

    let left_over_width = progress_bar_width
        - std::cmp::min(
            context_width + assistant_width + user_width + tools_width,
            progress_bar_width,
        );

    let is_overflow = (context_width + assistant_width + user_width + tools_width) > progress_bar_width;

    let total_percentage = calculate_context_percentage(context_data.total_tokens, context_data.context_window_size);

    if is_overflow {
        queue!(
            session.stderr,
            style::Print(format!(
                "\nCurrent context window ({} of {}k tokens used)\n",
                context_data.total_tokens,
                context_data.context_window_size / 1000
            )),
            StyledText::error_fg(),
            style::Print("█".repeat(progress_bar_width)),
            StyledText::reset(),
            style::Print(" "),
            style::Print(format!("{total_percentage:.2}%")),
        )?;
    } else {
        queue!(
            session.stderr,
            style::Print(format!(
                "\nCurrent context window ({} of {}k tokens used)\n",
                context_data.total_tokens,
                context_data.context_window_size / 1000
            )),
            // Context files
            StyledText::brand_fg(),
            // add a nice visual to mimic "tiny" progress, so the overrall progress bar doesn't look too
            // empty
            style::Print(
                "|".repeat(if context_width == 0 && context_data.context_tokens.value() > 0 {
                    1
                } else {
                    0
                })
            ),
            style::Print("█".repeat(context_width)),
            // Tools
            StyledText::error_fg(),
            style::Print(
                "|".repeat(if tools_width == 0 && context_data.tools_tokens.value() > 0 {
                    1
                } else {
                    0
                })
            ),
            style::Print("█".repeat(tools_width)),
            // Assistant responses
            StyledText::info_fg(),
            style::Print(
                "|".repeat(if assistant_width == 0 && context_data.assistant_tokens.value() > 0 {
                    1
                } else {
                    0
                })
            ),
            style::Print("█".repeat(assistant_width)),
            // User prompts
            StyledText::emphasis_fg(),
            style::Print("|".repeat(if user_width == 0 && context_data.user_tokens.value() > 0 {
                1
            } else {
                0
            })),
            style::Print("█".repeat(user_width)),
            StyledText::secondary_fg(),
            style::Print("█".repeat(left_over_width)),
            style::Print(" "),
            StyledText::reset(),
            style::Print(format!("{total_percentage:.2}%")),
        )?;
    }

    execute!(session.stderr, style::Print("\n\n"))?;

    queue!(
        session.stderr,
        StyledText::brand_fg(),
        style::Print("█ Context files: "),
        StyledText::reset(),
        style::Print(format!(
            "~{} tokens ({:.2}%)\n",
            context_data.context_tokens,
            calculate_context_percentage(context_data.context_tokens, context_data.context_window_size)
        )),
        StyledText::error_fg(),
        style::Print("█ Tools:    "),
        StyledText::reset(),
        style::Print(format!(
            " ~{} tokens ({:.2}%)\n",
            context_data.tools_tokens,
            calculate_context_percentage(context_data.tools_tokens, context_data.context_window_size)
        )),
        StyledText::info_fg(),
        style::Print("█ Kiro responses: "),
        StyledText::reset(),
        style::Print(format!(
            "  ~{} tokens ({:.2}%)\n",
            context_data.assistant_tokens,
            calculate_context_percentage(context_data.assistant_tokens, context_data.context_window_size)
        )),
        StyledText::emphasis_fg(),
        style::Print("█ Your prompts: "),
        StyledText::reset(),
        style::Print(format!(
            " ~{} tokens ({:.2}%)\n\n",
            context_data.user_tokens,
            calculate_context_percentage(context_data.user_tokens, context_data.context_window_size)
        )),
    )?;

    queue!(
        session.stderr,
        style::SetAttribute(Attribute::Bold),
        style::Print("\n💡 Pro Tips:\n"),
        StyledText::reset_attributes(),
        StyledText::secondary_fg(),
        style::Print("Run "),
        StyledText::success_fg(),
        style::Print("/compact"),
        StyledText::secondary_fg(),
        style::Print(" to replace the conversation history with its summary\n"),
        style::Print("Run "),
        StyledText::success_fg(),
        style::Print("/clear"),
        StyledText::secondary_fg(),
        style::Print(" to erase the entire chat history\n"),
        style::Print("Run "),
        StyledText::success_fg(),
        style::Print("/context show"),
        StyledText::secondary_fg(),
        style::Print(" to see tokens per context file\n\n"),
        StyledText::reset(),
    )?;

    Ok(())
}
