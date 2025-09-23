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
        let state = session
            .conversation
            .backend_conversation_state(os, true, &mut session.stderr)
            .await?;

        if !state.dropped_context_files.is_empty() {
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

        let data = state.calculate_conversation_size();
        let tool_specs_json: String = state
            .tools
            .values()
            .filter_map(|s| serde_json::to_string(s).ok())
            .collect::<Vec<String>>()
            .join("");
        let context_token_count: TokenCount = data.context_messages.into();
        let assistant_token_count: TokenCount = data.assistant_messages.into();
        let user_token_count: TokenCount = data.user_messages.into();
        let tools_char_count: CharCount = tool_specs_json.len().into(); // usize → CharCount
        let tools_token_count: TokenCount = tools_char_count.into(); // CharCount → TokenCount
        let total_token_used: TokenCount =
            (data.context_messages + data.user_messages + data.assistant_messages + tools_char_count).into();
        let window_width = session.terminal_width();
        // set a max width for the progress bar for better aesthetic
        let progress_bar_width = std::cmp::min(window_width, 80);

        let context_window_size = context_window_tokens(session.conversation.model_info.as_ref());
        let context_width =
            ((context_token_count.value() as f64 / context_window_size as f64) * progress_bar_width as f64) as usize;
        let assistant_width =
            ((assistant_token_count.value() as f64 / context_window_size as f64) * progress_bar_width as f64) as usize;
        let tools_width =
            ((tools_token_count.value() as f64 / context_window_size as f64) * progress_bar_width as f64) as usize;
        let user_width =
            ((user_token_count.value() as f64 / context_window_size as f64) * progress_bar_width as f64) as usize;

        let left_over_width = progress_bar_width
            - std::cmp::min(
                context_width + assistant_width + user_width + tools_width,
                progress_bar_width,
            );

        let is_overflow = (context_width + assistant_width + user_width + tools_width) > progress_bar_width;

        if is_overflow {
            queue!(
                session.stderr,
                style::Print(format!(
                    "\nCurrent context window ({} of {}k tokens used)\n",
                    total_token_used,
                    context_window_size / 1000
                )),
                style::SetForegroundColor(Color::DarkRed),
                style::Print("█".repeat(progress_bar_width)),
                style::SetForegroundColor(Color::Reset),
                style::Print(" "),
                style::Print(format!(
                    "{:.2}%",
                    (total_token_used.value() as f32 / context_window_size as f32) * 100.0
                )),
            )?;
        } else {
            queue!(
                session.stderr,
                style::Print(format!(
                    "\nCurrent context window ({} of {}k tokens used)\n",
                    total_token_used,
                    context_window_size / 1000
                )),
                // Context files
                style::SetForegroundColor(Color::DarkCyan),
                // add a nice visual to mimic "tiny" progress, so the overrall progress bar doesn't look too
                // empty
                style::Print("|".repeat(if context_width == 0 && *context_token_count > 0 {
                    1
                } else {
                    0
                })),
                style::Print("█".repeat(context_width)),
                // Tools
                style::SetForegroundColor(Color::DarkRed),
                style::Print("|".repeat(if tools_width == 0 && *tools_token_count > 0 {
                    1
                } else {
                    0
                })),
                style::Print("█".repeat(tools_width)),
                // Assistant responses
                style::SetForegroundColor(Color::Blue),
                style::Print("|".repeat(if assistant_width == 0 && *assistant_token_count > 0 {
                    1
                } else {
                    0
                })),
                style::Print("█".repeat(assistant_width)),
                // User prompts
                style::SetForegroundColor(Color::Magenta),
                style::Print("|".repeat(if user_width == 0 && *user_token_count > 0 { 1 } else { 0 })),
                style::Print("█".repeat(user_width)),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("█".repeat(left_over_width)),
                style::Print(" "),
                style::SetForegroundColor(Color::Reset),
                style::Print(format!(
                    "{:.2}%",
                    (total_token_used.value() as f32 / context_window_size as f32) * 100.0
                )),
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
                context_token_count,
                (context_token_count.value() as f32 / context_window_size as f32) * 100.0
            )),
            style::SetForegroundColor(Color::DarkRed),
            style::Print("█ Tools:    "),
            style::SetForegroundColor(Color::Reset),
            style::Print(format!(
                " ~{} tokens ({:.2}%)\n",
                tools_token_count,
                (tools_token_count.value() as f32 / context_window_size as f32) * 100.0
            )),
            style::SetForegroundColor(Color::Blue),
            style::Print("█ Q responses: "),
            style::SetForegroundColor(Color::Reset),
            style::Print(format!(
                "  ~{} tokens ({:.2}%)\n",
                assistant_token_count,
                (assistant_token_count.value() as f32 / context_window_size as f32) * 100.0
            )),
            style::SetForegroundColor(Color::Magenta),
            style::Print("█ Your prompts: "),
            style::SetForegroundColor(Color::Reset),
            style::Print(format!(
                " ~{} tokens ({:.2}%)\n\n",
                user_token_count,
                (user_token_count.value() as f32 / context_window_size as f32) * 100.0
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
