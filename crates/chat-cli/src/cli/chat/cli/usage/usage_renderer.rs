use crossterm::style::{Attribute, Color};
use crossterm::{execute, queue, style};

use crate::cli::chat::token_counter::TokenCount;
use crate::cli::chat::{ChatError, ChatSession};
use crate::theme::StyledText;

/// Calculate usage percentage from token counts (private utility)
fn calculate_usage_percentage(tokens: TokenCount, context_window_size: usize) -> f32 {
    (tokens.value() as f32 / context_window_size as f32) * 100.0
}

/// Render billing information section
/// Render billing information section
pub async fn render_billing_info(billing_data: &super::BillingUsageData, session: &mut ChatSession, show_unsupported_message: bool) -> Result<(), ChatError> {
    match &billing_data.status {
        super::BillingDataStatus::Available => {
            render_available_billing(billing_data, session).await
        },
        super::BillingDataStatus::FeatureNotSupported => {
            if show_unsupported_message {
                execute!(
                    session.stderr,
                    style::Print("Credit based usage is not supported for your subscription\n"),
                )?;
            }
            Ok(())
        },
        super::BillingDataStatus::BackendError(error_msg) => {
            execute!(
                session.stderr,
                style::SetForegroundColor(style::Color::Yellow),
                style::Print("⚠️  Warning: Could not retrieve usage information from backend\n"),
                style::SetForegroundColor(style::Color::DarkGrey),
                style::Print(format!("Error: {}\n\n", error_msg)),
                style::ResetColor,
            )?;
            Ok(())
        },
    }
}

/// Render available billing information
async fn render_available_billing(billing_data: &super::BillingUsageData, session: &mut ChatSession) -> Result<(), ChatError> {
    // Header
    execute!(
        session.stderr,
        style::SetAttribute(style::Attribute::Bold),
        style::Print("Usage details\n"),
        style::SetAttribute(style::Attribute::Reset),
        style::Print("To manage your account, upgrade your plan or configure overages use "),
        style::SetForegroundColor(Color::Blue),
        style::Print("/subscribe"),
        style::SetForegroundColor(Color::Reset),
        style::Print(" to open admin hub\n\n"),
    )?;

    // Bonus credits
    if !billing_data.bonus_credits.is_empty() {
        execute!(
            session.stderr,
            style::SetForegroundColor(Color::Red),
            style::Print("🎁 "),
            style::SetForegroundColor(Color::Reset),
            style::SetAttribute(style::Attribute::Bold),
            style::Print("Bonus credits: "),
            style::SetAttribute(style::Attribute::Reset),
            style::Print("You have bonus credits applied to your account, we will use these first, then your plan credits.\n"),
        )?;

        for bonus in &billing_data.bonus_credits {
            execute!(
                session.stderr,
                style::Print(format!("{}: {:.2}/{:.0} credits used, expires in {} days\n", 
                    bonus.name, bonus.used, bonus.total, bonus.days_until_expiry)),
            )?;
        }
        
        execute!(session.stderr, style::Print("\n"))?;
    }

    // Plan information
    execute!(
        session.stderr,
        style::Print(format!("Current plan: {}\n", billing_data.plan_name)),
    )?;

    // Overage information
    execute!(
        session.stderr,
        style::Print(format!("Overages: {}\n", if billing_data.overages_enabled { "On" } else { "Off" })),
    )?;

    execute!(
        session.stderr,
        style::Print(format!("{}\n\n", billing_data.billing_cycle_reset)),
    )?;

    // Display all usage breakdowns
    for breakdown in &billing_data.usage_breakdowns {
        execute!(
            session.stderr,
            style::Print(format!("Current {} usage ({:.2} of {:.0} used)\n", 
                breakdown.display_name.to_lowercase(), 
                breakdown.used, 
                breakdown.limit)),
        )?;

        // Progress bar
        let bar_width = 60;
        let filled_width = (breakdown.percentage as f32 / 100.0 * bar_width as f32) as usize;
        let empty_width = bar_width - filled_width;

        execute!(
            session.stderr,
            style::SetForegroundColor(Color::Magenta),
            style::Print("█".repeat(filled_width)),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print("█".repeat(empty_width)),
            style::SetForegroundColor(Color::Reset),
            style::Print(format!(" {}%\n\n", breakdown.percentage)),
        )?;
    }

    Ok(())
}

/// Render context window information section
pub async fn render_context_window(usage_data: &super::DetailedUsageData, session: &mut ChatSession) -> Result<(), ChatError> {
    if !usage_data.dropped_context_files.is_empty() {
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

    let context_width = ((usage_data.context_tokens.value() as f64 / usage_data.context_window_size as f64)
        * progress_bar_width as f64) as usize;
    let assistant_width = ((usage_data.assistant_tokens.value() as f64 / usage_data.context_window_size as f64)
        * progress_bar_width as f64) as usize;
    let tools_width = ((usage_data.tools_tokens.value() as f64 / usage_data.context_window_size as f64)
        * progress_bar_width as f64) as usize;
    let user_width = ((usage_data.user_tokens.value() as f64 / usage_data.context_window_size as f64)
        * progress_bar_width as f64) as usize;

    let left_over_width = progress_bar_width
        - std::cmp::min(
            context_width + assistant_width + user_width + tools_width,
            progress_bar_width,
        );

    let is_overflow = (context_width + assistant_width + user_width + tools_width) > progress_bar_width;

    let total_percentage = calculate_usage_percentage(usage_data.total_tokens, usage_data.context_window_size);

    if is_overflow {
        queue!(
            session.stderr,
            style::Print(format!(
                "\nCurrent context window ({} of {}k tokens used)\n",
                usage_data.total_tokens,
                usage_data.context_window_size / 1000
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
                usage_data.total_tokens,
                usage_data.context_window_size / 1000
            )),
            // Context files
            StyledText::brand_fg(),
            // add a nice visual to mimic "tiny" progress, so the overrall progress bar doesn't look too
            // empty
            style::Print(
                "|".repeat(if context_width == 0 && usage_data.context_tokens.value() > 0 {
                    1
                } else {
                    0
                })
            ),
            style::Print("█".repeat(context_width)),
            // Tools
            StyledText::error_fg(),
            style::Print("|".repeat(if tools_width == 0 && usage_data.tools_tokens.value() > 0 {
                1
            } else {
                0
            })),
            style::Print("█".repeat(tools_width)),
            // Assistant responses
            StyledText::info_fg(),
            style::Print(
                "|".repeat(if assistant_width == 0 && usage_data.assistant_tokens.value() > 0 {
                    1
                } else {
                    0
                })
            ),
            style::Print("█".repeat(assistant_width)),
            // User prompts
            StyledText::emphasis_fg(),
            style::Print("|".repeat(if user_width == 0 && usage_data.user_tokens.value() > 0 {
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
            usage_data.context_tokens,
            calculate_usage_percentage(usage_data.context_tokens, usage_data.context_window_size)
        )),
        StyledText::error_fg(),
        style::Print("█ Tools:    "),
        StyledText::reset(),
        style::Print(format!(
            " ~{} tokens ({:.2}%)\n",
            usage_data.tools_tokens,
            calculate_usage_percentage(usage_data.tools_tokens, usage_data.context_window_size)
        )),
        StyledText::info_fg(),
        style::Print("█ Kiro responses: "),
        StyledText::reset(),
        style::Print(format!(
            "  ~{} tokens ({:.2}%)\n",
            usage_data.assistant_tokens,
            calculate_usage_percentage(usage_data.assistant_tokens, usage_data.context_window_size)
        )),
        StyledText::emphasis_fg(),
        style::Print("█ Your prompts: "),
        StyledText::reset(),
        style::Print(format!(
            " ~{} tokens ({:.2}%)\n\n",
            usage_data.user_tokens,
            calculate_usage_percentage(usage_data.user_tokens, usage_data.context_window_size)
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
