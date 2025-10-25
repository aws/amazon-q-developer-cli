use clap::Args;
use crossterm::style::{Attribute, Color};
use crossterm::{
    execute,
    queue,
    style,
};
use chrono::{DateTime, Utc};

use super::model::context_window_tokens;
use crate::cli::chat::token_counter::{
    CharCount,
    TokenCount,
};
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;
use crate::theme::StyledText;

/// Detailed usage data for context window analysis
#[derive(Debug)]
pub struct DetailedUsageData {
    pub total_tokens: TokenCount,
    pub context_tokens: TokenCount,
    pub assistant_tokens: TokenCount,
    pub user_tokens: TokenCount,
    pub tools_tokens: TokenCount,
    pub context_window_size: usize,
    pub dropped_context_files: Vec<(String, String)>,
}

/// Calculate usage percentage from token counts
pub fn calculate_usage_percentage(tokens: TokenCount, context_window_size: usize) -> f32 {
    (tokens.value() as f32 / context_window_size as f32) * 100.0
}

/// Get detailed usage data for context window analysis
pub async fn get_detailed_usage_data(session: &mut ChatSession, os: &Os) -> Result<DetailedUsageData, ChatError> {
    let context_window_size = context_window_tokens(session.conversation.model_info.as_ref());

    let state = session
        .conversation
        .backend_conversation_state(os, true, &mut std::io::stderr())
        .await?;

    let data = state.calculate_conversation_size();
    let tool_specs_json: String = state
        .tools
        .values()
        .filter_map(|s| serde_json::to_string(s).ok())
        .collect::<Vec<String>>()
        .join("");
    let tools_char_count: CharCount = tool_specs_json.len().into();
    let total_tokens: TokenCount =
        (data.context_messages + data.user_messages + data.assistant_messages + tools_char_count).into();

    Ok(DetailedUsageData {
        total_tokens,
        context_tokens: data.context_messages.into(),
        assistant_tokens: data.assistant_messages.into(),
        user_tokens: data.user_messages.into(),
        tools_tokens: tools_char_count.into(),
        context_window_size,
        dropped_context_files: state.dropped_context_files,
    })
}

/// Get total usage percentage (simple interface for prompt generation)
pub async fn get_total_usage_percentage(session: &mut ChatSession, os: &Os) -> Result<f32, ChatError> {
    let data = get_detailed_usage_data(session, os).await?;
    Ok(calculate_usage_percentage(data.total_tokens, data.context_window_size))
}

/// Display billing and subscription information
async fn display_billing_info(os: &Os, session: &mut ChatSession) -> Result<bool, ChatError> {
    match os.client.get_usage_limits().await {
        Ok(usage_limits) => {
            display_user_and_plan_info(&usage_limits, session).await?;
            display_bonus_credits(&usage_limits, session).await?;
            display_estimated_usage(&usage_limits, session).await?;
            Ok(true)
        },
        Err(_) => {
            // Hide billing section when not authenticated
            Ok(false)
        }
    }
}

async fn display_user_and_plan_info(_usage_limits: &amzn_codewhisperer_client::operation::get_usage_limits::GetUsageLimitsOutput, session: &mut ChatSession) -> Result<(), ChatError> {
    execute!(
        session.stderr,
        style::SetAttribute(style::Attribute::Bold),
        style::Print("Usage details\n"),
        style::SetAttribute(style::Attribute::Reset),
        style::Print("To manage your account, upgrade your plan or configure overages use "),
        style::SetForegroundColor(Color::Blue),
        style::Print("/usage manage"),
        style::SetForegroundColor(Color::Reset),
        style::Print(" to open admin hub\n\n"),
    )?;
    Ok(())
}

async fn display_bonus_credits(usage_limits: &amzn_codewhisperer_client::operation::get_usage_limits::GetUsageLimitsOutput, session: &mut ChatSession) -> Result<(), ChatError> {
    let usage_breakdown = usage_limits.usage_breakdown_list();
    
    // Find Credits resource type for bonus credits
    if let Some(credits) = usage_breakdown.iter().find(|item| {
        item.resource_type().map_or(false, |rt| rt.as_str() == "CREDIT")
    }) {
        if let Some(free_trial_info) = credits.free_trial_info() {
            let used = free_trial_info.current_usage().unwrap_or(0);
            let total = free_trial_info.usage_limit().unwrap_or(0);
            
            // Calculate days until expiry
            if let Some(expiry_timestamp) = free_trial_info.free_trial_expiry() {
                let expiry_secs = expiry_timestamp.secs();
                let expiry_date = DateTime::from_timestamp(expiry_secs, 0).unwrap_or_else(|| Utc::now());
                let now = Utc::now();
                let days_until_expiry = (expiry_date - now).num_days().max(0);
                
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print("🎁 "),
                    style::SetForegroundColor(Color::Reset),
                    style::SetAttribute(style::Attribute::Bold),
                    style::Print("Bonus credits: "),
                    style::SetAttribute(style::Attribute::Reset),
                    style::Print("You have bonus credits applied to your account, we will use these first, then your plan credits.\n"),
                    style::Print(format!("New user credit bonus: {}/{} credits used, expires in {} days\n\n", used, total, days_until_expiry)),
                )?;
            }
        }
    }
    Ok(())
}

async fn display_estimated_usage(usage_limits: &amzn_codewhisperer_client::operation::get_usage_limits::GetUsageLimitsOutput, session: &mut ChatSession) -> Result<(), ChatError> {
    let usage_breakdown = usage_limits.usage_breakdown_list();
    
    // Get plan info
    let plan_name = usage_limits.subscription_info()
        .map(|si| si.subscription_title())
        .unwrap_or("Unknown");

    // Get days until reset
    let days_left = usage_limits.days_until_reset().unwrap_or(0);

    // Get credits info
    if let Some(credits) = usage_breakdown.iter().find(|item| {
        item.resource_type().map_or(false, |rt| rt.as_str() == "CREDIT")
    }) {
        let used = credits.current_usage();
        let limit = credits.usage_limit();
        let percentage = if limit > 0 { (used as f32 / limit as f32 * 100.0) as i32 } else { 0 };

        execute!(
            session.stderr,
            style::Print(format!("Current plan: {}\n", plan_name)),
            style::Print("Overages: Off\n"),
            style::Print(format!("Days left in billing cycle: {}\n\n", days_left)),
            style::Print(format!("Current plan credit usage ({} of {} credits used)\n", used, limit)),
        )?;

        // Draw progress bar
        let bar_width = 60;
        let filled_width = (percentage as f32 / 100.0 * bar_width as f32) as usize;
        let empty_width = bar_width - filled_width;

        execute!(
            session.stderr,
            style::SetForegroundColor(Color::Magenta),
            style::Print("█".repeat(filled_width)),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print("█".repeat(empty_width)),
            style::SetForegroundColor(Color::Reset),
            style::Print(format!(" {}%\n\n", percentage)),
        )?;
    }

    Ok(())
}

/// Arguments for the usage command that displays token usage statistics and context window
/// information.
///
/// This command shows how many tokens are being used by different components (context files, tools,
/// assistant responses, and user prompts) within the current chat session's context window.
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct UsageArgs {
    /// Show only context window usage
    #[arg(long)]
    context: bool,
    /// Show only credits and billing information
    #[arg(long)]
    credits: bool,
}

impl UsageArgs {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        match (self.context, self.credits) {
            (true, false) => {
                // Show only context window usage
                self.show_context_usage(os, session).await
            },
            (false, true) => {
                // Show only credits/billing information
                self.show_credits_info(os, session).await
            },
            (false, false) => {
                // Show both (default behavior)
                self.show_full_usage(os, session).await
            },
            (true, true) => {
                // Both flags specified - show error
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print("Error: Cannot specify both --context and --credits flags\n"),
                    style::SetForegroundColor(Color::Reset),
                )?;
                Ok(ChatState::PromptUser { skip_printing_tools: true })
            }
        }
    }

    async fn show_context_usage(&self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let usage_data = get_detailed_usage_data(session, os).await?;
        self.display_context_window(&usage_data, session).await?;
        Ok(ChatState::PromptUser { skip_printing_tools: true })
    }

    async fn show_credits_info(&self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let billing_displayed = display_billing_info(os, session).await?;
        if !billing_displayed {
            execute!(
                session.stderr,
                style::Print("Credit based usage is not supported for your subscription\n"),
            )?;
        }
        Ok(ChatState::PromptUser { skip_printing_tools: true })
    }

    async fn show_full_usage(&self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Try to display billing information first (silently ignore if not available)
        let _billing_displayed = display_billing_info(os, session).await?;

        let usage_data = get_detailed_usage_data(session, os).await?;
        self.display_context_window(&usage_data, session).await?;
        Ok(ChatState::PromptUser { skip_printing_tools: true })
    }

    async fn display_context_window(&self, usage_data: &DetailedUsageData, session: &mut ChatSession) -> Result<(), ChatError> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_usage_percentage() {
        let char_count: CharCount = 4000.into(); // 4000 chars ≈ 1000 tokens
        let tokens: TokenCount = char_count.into();
        let context_window_size = 10000;
        let percentage = calculate_usage_percentage(tokens, context_window_size);
        assert_eq!(percentage, 10.0);
    }

    #[test]
    fn test_calculate_usage_percentage_zero() {
        let char_count: CharCount = 0.into();
        let tokens: TokenCount = char_count.into();
        let context_window_size = 10000;
        let percentage = calculate_usage_percentage(tokens, context_window_size);
        assert_eq!(percentage, 0.0);
    }

    #[test]
    fn test_calculate_usage_percentage_full() {
        let char_count: CharCount = 40000.into(); // 40000 chars ≈ 10000 tokens
        let tokens: TokenCount = char_count.into();
        let context_window_size = 10000;
        let percentage = calculate_usage_percentage(tokens, context_window_size);
        assert_eq!(percentage, 100.0);
    }
}
