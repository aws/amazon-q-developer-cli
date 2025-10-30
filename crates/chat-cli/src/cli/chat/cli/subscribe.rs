use clap::Args;
use crossterm::style::Stylize;
use crossterm::{
    cursor,
    execute,
    queue,
    style,
};

use crate::auth::builder_id::is_idc_user;
use crate::cli::chat::{
    ActualSubscriptionStatus,
    ChatError,
    ChatSession,
    ChatState,
    get_subscription_status_with_spinner,
    with_spinner,
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
        } else if self.manage {
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
        } else {
            upgrade_to_pro(os, session).await?;
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}

async fn upgrade_to_pro(os: &mut Os, session: &mut ChatSession) -> Result<(), ChatError> {
    queue!(session.stderr, style::Print("\n"),)?;

    // Get current subscription status
    match get_subscription_status_with_spinner(os, &mut session.stderr).await {
        Ok(status) => {
            if status == ActualSubscriptionStatus::Active {
                queue!(
                    session.stderr,
                    StyledText::warning_fg(),
                    style::Print(format!("{}\n\n", subscription_text::already_subscribed_message())),
                    StyledText::reset(),
                )?;
                return Ok(());
            }
        },
        Err(e) => {
            execute!(
                session.stderr,
                StyledText::error_fg(),
                style::Print(format!("{e}\n\n")),
                StyledText::reset(),
            )?;
            // Don't exit early here, the check isn't required to subscribe.
        },
    }

    // Upgrade information
    queue!(
        session.stderr,
        StyledText::primary_fg(),
        style::Print(subscription_text::subscribe_title()),
        StyledText::secondary_fg(),
        style::Print(format!("\n\n{}\n\n", subscription_text::SUBSCRIBE_INFO)),
        StyledText::reset(),
        cursor::Show
    )?;

    let prompt = format!(
        "{}{}{}{}{}",
        "Would you like to open the AWS console to upgrade? [".dark_grey(),
        "y".green(),
        "/".dark_grey(),
        "n".green(),
        "]: ".dark_grey(),
    );

    let user_input = session.read_user_input(&prompt, true);
    queue!(session.stderr, StyledText::reset(), style::Print("\n"),)?;

    if !user_input.is_some_and(|i| ["y", "Y"].contains(&i.as_str())) {
        execute!(
            session.stderr,
            StyledText::error_fg(),
            style::Print("Upgrade cancelled.\n\n"),
            StyledText::reset(),
        )?;
        return Ok(());
    }

    // Create a subscription token and open the webpage
    let r = os.client.create_subscription_token().await?;

    let url = with_spinner(&mut session.stderr, "Preparing to upgrade...", || async move {
        r.encoded_verification_url()
            .map(|s| s.to_string())
            .ok_or(ChatError::Custom("Missing verification URL".into()))
    })
    .await?;

    if is_remote() || crate::util::open::open_url_async(&url).await.is_err() {
        queue!(
            session.stderr,
            StyledText::secondary_fg(),
            style::Print(format!(
                "{} Having issues opening the AWS console? Try copy and pasting the URL > {}\n\n",
                "?".magenta(),
                url.blue()
            )),
            StyledText::reset(),
        )?;
    }

    execute!(
        session.stderr,
        style::Print("Once upgraded, type a new prompt to continue your work, or type /quit to exit the chat.\n\n")
    )?;

    Ok(())
}
