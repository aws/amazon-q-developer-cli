use clap::Args;
use crossterm::style::Stylize;
use crossterm::{
    execute,
    queue,
    style,
};

use crate::auth::builder_id::is_idc_user;
use crate::auth::social::is_social_logged_in;
use crate::cli::chat::{
    ActualSubscriptionStatus,
    ChatError,
    ChatSession,
    ChatState,
    get_subscription_status_with_spinner,
};
use crate::constants::subscription_text;
use crate::os::Os;
use crate::theme::StyledText;
use crate::util::system_info::is_remote;

/// Arguments for the subscribe command to manage Developer Pro subscriptions
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct SubscribeArgs {
    /// Open the AWS console to manage an existing subscription
    #[arg(long)]
    manage: bool,
}

impl SubscribeArgs {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        if is_idc_user(&os.database).await {
            execute!(
                session.stderr,
                StyledText::warning_fg(),
                style::Print(format!("\n{}\n\n", subscription_text::idc_subscription_message())),
                StyledText::reset(),
            )?;
        } else if is_social_logged_in(&os.database).await {
            // Social login - redirect to kiro.dev
            redirect_to_kiro_dev(session).await?;
        } else {
            // Builder ID users - check subscription status
            match get_subscription_status_with_spinner(os, &mut session.stderr).await {
                Ok(ActualSubscriptionStatus::Active) => {
                    // Paid Builder ID (active) - keep current experience
                    if self.manage {
                        manage_subscription(os, session).await?;
                    } else {
                        execute!(
                            session.stderr,
                            StyledText::warning_fg(),
                            style::Print(format!("\n{}\n\n", subscription_text::already_subscribed_message())),
                            StyledText::reset(),
                        )?;
                    }
                },
                _ => {
                    // Free Builder ID - redirect to kiro.dev
                    redirect_to_kiro_dev(session).await?;
                },
            }
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}

async fn redirect_to_kiro_dev(session: &mut ChatSession) -> Result<(), ChatError> {
    execute!(
        session.stderr,
        style::Print("\n"),
        StyledText::primary_fg(),
        style::Print("Visit kiro.dev to manage your subscription.\n\n"),
        StyledText::reset(),
    )?;

    let url = "https://app.kiro.dev/account/usage";
    if is_remote() || crate::util::open::open_url_async(url).await.is_err() {
        execute!(
            session.stderr,
            style::Print(format!("Open this URL: {}\n\n", url.blue())),
        )?;
    }

    Ok(())
}

async fn manage_subscription(os: &mut Os, session: &mut ChatSession) -> Result<(), ChatError> {
    queue!(session.stderr, style::Print("\n"),)?;
    match get_subscription_status_with_spinner(os, &mut session.stderr).await {
        Ok(status) => {
            if status != ActualSubscriptionStatus::Active {
                queue!(
                    session.stderr,
                    StyledText::warning_fg(),
                    style::Print(format!("{}. ", subscription_text::no_subscription_message())),
                    StyledText::secondary_fg(),
                    style::Print("Use "),
                    StyledText::success_fg(),
                    style::Print("/subscribe"),
                    StyledText::secondary_fg(),
                    style::Print(" to upgrade your subscription.\n\n"),
                    StyledText::reset(),
                )?;
            }
        },
        Err(err) => {
            queue!(
                session.stderr,
                StyledText::error_fg(),
                style::Print(format!("Failed to get subscription status: {err}\n\n")),
                StyledText::reset(),
            )?;
        },
    }

    let url = format!(
        "https://{}.console.aws.amazon.com/amazonq/developer/home#/subscriptions",
        os.database
            .get_idc_region()
            .ok()
            .flatten()
            .unwrap_or("us-east-1".to_string())
    );
    if is_remote() || crate::util::open::open_url_async(&url).await.is_err() {
        execute!(
            session.stderr,
            style::Print(format!("Open this URL to manage your subscription: {}\n\n", url.blue())),
            StyledText::reset(),
            StyledText::reset(),
        )?;
    }

    Ok(())
}
