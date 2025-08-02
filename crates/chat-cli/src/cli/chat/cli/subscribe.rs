use clap::Args;
use crossterm::style::{
    Color,
    Stylize,
};
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
use crate::os::Os;
use crate::util::system_info::is_remote;

const SUBSCRIBE_TITLE_TEXT: &str = color_print::cstr! { "<white!,bold>Subscribe to Q Developer Pro</white!,bold>" };

const SUBSCRIBE_TEXT: &str = color_print::cstr! { "During the upgrade, you'll be asked to link your Builder ID to the AWS account that will be billed the monthly subscription fee.

Need help? Visit our subscription support page> <blue!>https://docs.aws.amazon.com/console/amazonq/upgrade-builder-id</blue!>" };

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct SubscribeArgs {
    #[arg(long)]
    manage: bool,
}

impl SubscribeArgs {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        if is_idc_user(&os.database)
            .await
            .map_err(|e| ChatError::Custom(e.to_string().into()))?
        {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print("\nYour Q Developer Pro subscription is managed through IAM Identity Center.\n\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
        } else if self.manage {
            queue!(session.stderr, style::Print("\n"),)?;
            match get_subscription_status_with_spinner(os, &mut session.stderr).await {
                Ok(status) => {
                    if status != ActualSubscriptionStatus::Active {
                        queue!(
                            session.stderr,
                            style::SetForegroundColor(Color::Yellow),
                            style::Print("You don't seem to have a Q Developer Pro subscription. "),
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print("Use "),
                            style::SetForegroundColor(Color::Green),
                            style::Print("/subscribe"),
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print(" to upgrade your subscription.\n\n"),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    }
                },
                Err(err) => {
                    queue!(
                        session.stderr,
                        style::SetForegroundColor(Color::Red),
                        style::Print(format!("Failed to get subscription status: {}\n\n", err)),
                        style::SetForegroundColor(Color::Reset),
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
                    style::ResetColor,
                    style::SetForegroundColor(Color::Reset),
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
                    style::SetForegroundColor(Color::Yellow),
                    style::Print("Your Builder ID already has a Q Developer Pro subscription.\n\n"),
                    style::SetForegroundColor(Color::Reset),
                )?;
                return Ok(());
            }
        },
        Err(e) => {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Red),
                style::Print(format!("{}\n\n", e)),
                style::SetForegroundColor(Color::Reset),
            )?;
            // Don't exit early here, the check isn't required to subscribe.
        },
    }

    // Upgrade information
    queue!(
        session.stderr,
        style::Print(SUBSCRIBE_TITLE_TEXT),
        style::SetForegroundColor(Color::Grey),
        style::Print(format!("\n\n{}\n\n", SUBSCRIBE_TEXT)),
        style::SetForegroundColor(Color::Reset),
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

    let user_input = session.read_user_input(&prompt, true).await;
    queue!(
        session.stderr,
        style::SetForegroundColor(Color::Reset),
        style::Print("\n"),
    )?;

    if !user_input.is_some_and(|i| ["y", "Y"].contains(&i.as_str())) {
        execute!(
            session.stderr,
            style::SetForegroundColor(Color::Red),
            style::Print("Upgrade cancelled.\n\n"),
            style::SetForegroundColor(Color::Reset),
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
            style::SetForegroundColor(Color::DarkGrey),
            style::Print(format!(
                "{} Having issues opening the AWS console? Try copy and pasting the URL > {}\n\n",
                "?".magenta(),
                url.blue()
            )),
            style::SetForegroundColor(Color::Reset),
        )?;
    }

    execute!(
        session.stderr,
        style::Print("Once upgraded, type a new prompt to continue your work, or type /quit to exit the chat.\n\n")
    )?;

    Ok(())
}
