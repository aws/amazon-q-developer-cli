use std::fmt;
use std::fmt::Display;
use std::process::{
    ExitCode,
    exit,
};
use std::time::Duration;

use anstream::println;
use clap::Subcommand;
use crossterm::style::Stylize;
use eyre::{
    Result,
    bail,
};
use fig_auth::builder_id::{
    PollCreateToken,
    TokenType,
    poll_create_token,
    start_device_authorization,
};
use fig_auth::pkce::start_pkce_authorization;
use fig_auth::secret_store::SecretStore;
use fig_ipc::local::{
    login_command,
    logout_command,
};
use fig_util::system_info::is_remote;
use fig_util::{
    CLI_BINARY_NAME,
    PRODUCT_NAME,
};
use serde_json::json;
use tracing::error;

use super::OutputFormat;
use crate::util::spinner::{
    Spinner,
    SpinnerComponent,
};
use crate::util::{
    choose,
    input,
};

#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum RootUserSubcommand {
    /// Login
    Login,
    /// Logout
    Logout,
    /// Prints details about the current user
    Whoami {
        /// Output format to use
        #[arg(long, short, value_enum, default_value_t)]
        format: OutputFormat,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthMethod {
    /// Builder ID (free)
    BuilderId,
    /// IdC (enterprise)
    IdentityCenter,
}

impl Display for AuthMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthMethod::BuilderId => write!(f, "Use for Free with Builder ID"),
            AuthMethod::IdentityCenter => write!(f, "Use with Pro license"),
        }
    }
}

impl RootUserSubcommand {
    pub async fn execute(self) -> Result<ExitCode> {
        match self {
            Self::Login => {
                if fig_auth::is_logged_in().await {
                    eyre::bail!(
                        "Already logged in, please logout with {} first",
                        format!("{CLI_BINARY_NAME} logout").magenta()
                    );
                }

                login_interactive().await?;

                Ok(ExitCode::SUCCESS)
            },
            Self::Logout => {
                let logout_join = logout_command();

                let (_, _) = tokio::join!(logout_join, fig_auth::logout());

                println!("You are now logged out");
                println!(
                    "Run {} to log back in to {PRODUCT_NAME}",
                    format!("{CLI_BINARY_NAME} login").magenta()
                );
                Ok(ExitCode::SUCCESS)
            },
            Self::Whoami { format } => {
                let builder_id = fig_auth::builder_id_token().await;

                match builder_id {
                    Ok(Some(token)) => {
                        format.print(
                            || match token.token_type() {
                                TokenType::BuilderId => "Logged in with Builder ID".into(),
                                TokenType::IamIdentityCenter => {
                                    format!(
                                        "Logged in with IAM Identity Center ({})",
                                        token.start_url.as_ref().unwrap()
                                    )
                                },
                            },
                            || {
                                json!({
                                    "accountType": match token.token_type() {
                                        TokenType::BuilderId => "BuilderId",
                                        TokenType::IamIdentityCenter => "IamIdentityCenter",
                                    },
                                    "startUrl": token.start_url,
                                    "region": token.region,
                                })
                            },
                        );
                        Ok(ExitCode::SUCCESS)
                    },
                    _ => {
                        format.print(|| "Not logged in", || json!({ "account": null }));
                        Ok(ExitCode::FAILURE)
                    },
                }
            },
        }
    }
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum UserSubcommand {
    #[command(flatten)]
    Root(RootUserSubcommand),
}

impl UserSubcommand {
    pub async fn execute(self) -> Result<ExitCode> {
        ctrlc::set_handler(|| exit(1))?;

        match self {
            Self::Root(cmd) => cmd.execute().await,
        }
    }
}

pub async fn login_interactive() -> Result<()> {
    let options = [AuthMethod::BuilderId, AuthMethod::IdentityCenter];
    let i = match choose("Select login method", &options)? {
        Some(i) => i,
        None => bail!("No login method selected"),
    };
    let login_method = options[i];
    match login_method {
        AuthMethod::BuilderId | AuthMethod::IdentityCenter => {
            let (start_url, region) = match login_method {
                AuthMethod::BuilderId => (None, None),
                AuthMethod::IdentityCenter => {
                    let default_start_url = fig_settings::state::get_string("auth.idc.start-url").ok().flatten();
                    let default_region = fig_settings::state::get_string("auth.idc.region").ok().flatten();

                    let start_url = input("Enter Start URL", default_start_url.as_deref())?;
                    let region = input("Enter Region", default_region.as_deref())?;

                    let _ = fig_settings::state::set_value("auth.idc.start-url", start_url.clone());
                    let _ = fig_settings::state::set_value("auth.idc.region", region.clone());

                    (Some(start_url), Some(region))
                },
            };
            let secret_store = SecretStore::new().await?;

            // Remote machine won't be able to handle browser opening and redirects,
            // hence always use device code flow.
            if is_remote() {
                try_device_authorization(&secret_store, start_url.clone(), region.clone()).await?;
            } else {
                let (client, registration) = start_pkce_authorization(start_url.clone(), region.clone()).await?;

                match fig_util::open_url_async(&registration.url).await {
                    // If it succeeded, finish PKCE.
                    Ok(()) => {
                        let mut spinner = Spinner::new(vec![
                            SpinnerComponent::Spinner,
                            SpinnerComponent::Text(" Logging in...".into()),
                        ]);
                        registration.finish(&client, Some(&secret_store)).await?;
                        fig_telemetry::send_user_logged_in().await;
                        spinner.stop_with_message("Logged in successfully".into());
                    },
                    // If we are unable to open the link with the browser, then fallback to
                    // the device code flow.
                    Err(err) => {
                        error!(%err, "Failed to open URL with browser, falling back to device code flow");

                        // Try device code flow.
                        try_device_authorization(&secret_store, start_url.clone(), region.clone()).await?;
                    },
                }
            }
        },
    };

    if let Err(err) = login_command().await {
        error!(%err, "Failed to send login command");
    }

    Ok(())
}

async fn try_device_authorization(
    secret_store: &SecretStore,
    start_url: Option<String>,
    region: Option<String>,
) -> Result<()> {
    let device_auth = start_device_authorization(secret_store, start_url.clone(), region.clone()).await?;

    println!();
    println!("Confirm the following code in the browser");
    println!("Code: {}", device_auth.user_code.bold());
    println!();

    let print_open_url = || println!("Open this URL: {}", device_auth.verification_uri_complete);

    if is_remote() {
        print_open_url();
    } else if let Err(err) = fig_util::open_url_async(&device_auth.verification_uri_complete).await {
        error!(%err, "Failed to open URL with browser");
        print_open_url();
    }

    let mut spinner = Spinner::new(vec![
        SpinnerComponent::Spinner,
        SpinnerComponent::Text(" Logging in...".into()),
    ]);

    loop {
        tokio::time::sleep(Duration::from_secs(device_auth.interval.try_into().unwrap_or(1))).await;
        match poll_create_token(
            secret_store,
            device_auth.device_code.clone(),
            start_url.clone(),
            region.clone(),
        )
        .await
        {
            PollCreateToken::Pending => {},
            PollCreateToken::Complete(_) => {
                fig_telemetry::send_user_logged_in().await;
                spinner.stop_with_message("Logged in successfully".into());
                break;
            },
            PollCreateToken::Error(err) => {
                spinner.stop();
                return Err(err.into());
            },
        };
    }
    Ok(())
}
