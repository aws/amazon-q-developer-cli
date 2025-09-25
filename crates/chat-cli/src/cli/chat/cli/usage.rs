use std::convert::TryFrom;
use std::time::SystemTime;

use amzn_codewhisperer_client::types::{
    OverageStatus,
    ResourceType,
    SubscriptionType,
    UsageBreakdown,
};
use clap::Args;
use crossterm::style::{
    Attribute,
    Color,
};
use crossterm::{
    execute,
    queue,
    style,
};

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

/// Arguments for the usage command that displays token usage statistics and context window
/// information.
///
/// This command shows how many tokens are being used by different components (context files, tools,
/// assistant responses, and user prompts) within the current chat session's context window.
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct UsageArgs;

impl UsageArgs {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let usage_data = get_detailed_usage_data(session, os).await?;

        if !usage_data.dropped_context_files.is_empty() {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::DarkYellow),
                style::Print("\nSome context files are dropped due to size limit, please run "),
                style::SetForegroundColor(Color::DarkGreen),
                style::Print("/context show "),
                style::SetForegroundColor(Color::DarkYellow),
                style::Print("to learn more.\n"),
                style::SetForegroundColor(style::Color::Reset)
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
                style::SetForegroundColor(Color::DarkRed),
                style::Print("█".repeat(progress_bar_width)),
                style::SetForegroundColor(Color::Reset),
                style::Print(" "),
                style::Print(format!("{:.2}%", total_percentage)),
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
                style::SetForegroundColor(Color::DarkCyan),
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
                style::SetForegroundColor(Color::DarkRed),
                style::Print("|".repeat(if tools_width == 0 && usage_data.tools_tokens.value() > 0 {
                    1
                } else {
                    0
                })),
                style::Print("█".repeat(tools_width)),
                // Assistant responses
                style::SetForegroundColor(Color::Blue),
                style::Print(
                    "|".repeat(if assistant_width == 0 && usage_data.assistant_tokens.value() > 0 {
                        1
                    } else {
                        0
                    })
                ),
                style::Print("█".repeat(assistant_width)),
                // User prompts
                style::SetForegroundColor(Color::Magenta),
                style::Print("|".repeat(if user_width == 0 && usage_data.user_tokens.value() > 0 {
                    1
                } else {
                    0
                })),
                style::Print("█".repeat(user_width)),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("█".repeat(left_over_width)),
                style::Print(" "),
                style::SetForegroundColor(Color::Reset),
                style::Print(format!("{:.2}%", total_percentage)),
            )?;
        }

        execute!(session.stderr, style::Print("\n\n"))?;

        queue!(
            session.stderr,
            style::SetForegroundColor(Color::DarkCyan),
            style::Print("█ Context files: "),
            style::SetForegroundColor(Color::Reset),
            style::Print(format!(
                "~{} tokens ({:.2}%)\n",
                usage_data.context_tokens,
                calculate_usage_percentage(usage_data.context_tokens, usage_data.context_window_size)
            )),
            style::SetForegroundColor(Color::DarkRed),
            style::Print("█ Tools:    "),
            style::SetForegroundColor(Color::Reset),
            style::Print(format!(
                " ~{} tokens ({:.2}%)\n",
                usage_data.tools_tokens,
                calculate_usage_percentage(usage_data.tools_tokens, usage_data.context_window_size)
            )),
            style::SetForegroundColor(Color::Blue),
            style::Print("█ Q responses: "),
            style::SetForegroundColor(Color::Reset),
            style::Print(format!(
                "  ~{} tokens ({:.2}%)\n",
                usage_data.assistant_tokens,
                calculate_usage_percentage(usage_data.assistant_tokens, usage_data.context_window_size)
            )),
            style::SetForegroundColor(Color::Magenta),
            style::Print("█ Your prompts: "),
            style::SetForegroundColor(Color::Reset),
            style::Print(format!(
                " ~{} tokens ({:.2}%)\n\n",
                usage_data.user_tokens,
                calculate_usage_percentage(usage_data.user_tokens, usage_data.context_window_size)
            )),
        )?;

        match os.client.get_usage_limits(None).await {
            Ok(resp) => {
                tracing::debug!(?resp, "Raw get_usage_limits response");
                // Subscription tier
                if let Some(sub) = resp.subscription_info() {
                    let tier_str = match sub.r#type() {
                        SubscriptionType::QDeveloperStandaloneFree => "Free tier",
                        SubscriptionType::QDeveloperStandalone => "Pro tier",
                        SubscriptionType::QDeveloperStandaloneProPlus => "Pro Plus tier",
                        _ => "",
                    };
                    queue!(
                        session.stderr,
                        style::Print("\n"),
                        style::SetAttribute(Attribute::Bold),
                        style::Print(format!("📊 {} Usage limits\n", tier_str)),
                        style::SetAttribute(Attribute::Reset),
                    )?;
                }

                // Usage breakdown
                let list: &[UsageBreakdown] = resp.usage_breakdown_list();
                if list.is_empty() {
                    queue!(session.stderr, style::Print("\nUsage information unavailable\n\n"),)?;
                } else {
                    let ub = list
                        .iter()
                        .find(|b| matches!(b.resource_type(), Some(ResourceType::AgenticRequest)))
                        .unwrap_or_else(|| list.first().expect("UsageBreakdown list is not null"));

                    let current = ub.current_usage();
                    let limit = ub.usage_limit();
                    let overage_charges = ub.overage_charges();
                    let reset_local = match ub.next_date_reset() {
                        Some(dt) => {
                            //  DateTime → SystemTime
                            match SystemTime::try_from(*dt) {
                                Ok(st) => {
                                    let local: chrono::DateTime<chrono::Local> = st.into();
                                    local.format("%m/%d/%Y at %H:%M:%S").to_string()
                                },
                                Err(_) => "1st of next month 12:00:00 GMT".to_string(),
                            }
                        },
                        None => "1st of next month 12:00:00 GMT".to_string(),
                    };

                    // Overage status
                    let overage_msg = match resp.overage_configuration().map(|c| c.overage_status()) {
                        Some(OverageStatus::Enabled) => format!("${:.2} incurred in overages", overage_charges),
                        Some(OverageStatus::Disabled) => "Overage disabled by admin".to_string(),
                        _ => String::new(),
                    };

                    queue!(
                        session.stderr,
                        // Line 1: queries used
                        style::Print(format!("• {} of {} queries used\n", current, limit)),
                        // Line 2: overage info
                        style::Print(format!("• {}\n", overage_msg)),
                        // Line 3: reset time
                        style::Print(format!("• Limits reset on {}\n\n", reset_local)),
                    )?;
                }
            },
            Err(e) => {
                tracing::error!(error = ?e, "Failed to load usage limits with full error");
                queue!(
                    session.stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("\nFailed to load usage limits: {}\n\n", e)),
                    style::SetForegroundColor(Color::Reset),
                )?;
            },
        }

        queue!(
            session.stderr,
            style::SetAttribute(Attribute::Bold),
            style::Print("\n💡 Pro Tips:\n"),
            style::SetAttribute(Attribute::Reset),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print("Run "),
            style::SetForegroundColor(Color::DarkGreen),
            style::Print("/compact"),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print(" to replace the conversation history with its summary\n"),
            style::Print("Run "),
            style::SetForegroundColor(Color::DarkGreen),
            style::Print("/clear"),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print(" to erase the entire chat history\n"),
            style::Print("Run "),
            style::SetForegroundColor(Color::DarkGreen),
            style::Print("/context show"),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print(" to see tokens per context file\n\n"),
            style::SetForegroundColor(Color::Reset),
        )?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}
