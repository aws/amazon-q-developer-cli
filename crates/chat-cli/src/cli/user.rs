use std::fmt;
use std::fmt::Display;
use std::process::{
    ExitCode,
    exit,
};
use std::time::Duration;

use anstream::{
    eprintln,
    println,
};
use clap::{
    Args,
    Subcommand,
};
use crossterm::style::Stylize;
use dialoguer::Select;
use eyre::{
    Result,
    bail,
};
use serde_json::json;
use tokio::signal::ctrl_c;
use tracing::{
    error,
    info,
};

use super::OutputFormat;
use crate::api_client::list_available_profiles;
use crate::api_client::profile::Profile;
use crate::auth::builder_id::{
    PollCreateToken,
    TokenType,
    poll_create_token,
    start_device_authorization,
};
use crate::auth::pkce::start_pkce_authorization;
use crate::auth::secret_store::SecretStore;
use crate::fig_telemetry::{
    QProfileSwitchIntent,
    TelemetryResult,
};
use crate::fig_util::spinner::{
    Spinner,
    SpinnerComponent,
};
use crate::fig_util::system_info::is_remote;
use crate::fig_util::{
    CHAT_BINARY_NAME,
    PRODUCT_NAME,
    choose,
    input,
};

#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum RootUserSubcommand {
    /// Login
    Login(LoginArgs),
    /// Logout
    Logout,
    /// Prints details about the current user
    Whoami {
        /// Output format to use
        #[arg(long, short, value_enum, default_value_t)]
        format: OutputFormat,
    },
    /// Show the profile associated with this idc user
    Profile,
}

#[derive(Args, Debug, PartialEq, Eq, Clone, Default)]
pub struct LoginArgs {
    /// License type (pro for Identity Center, free for Builder ID)
    #[arg(long, value_enum)]
    pub license: Option<LicenseType>,

    /// Identity provider URL (for Identity Center)
    #[arg(long)]
    pub identity_provider: Option<String>,

    /// Region (for Identity Center)
    #[arg(long)]
    pub region: Option<String>,

    /// Always use the OAuth device flow for authentication. Useful for instances where browser
    /// redirects cannot be handled.
    #[arg(long)]
    pub use_device_flow: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum LicenseType {
    /// Free license with Builder ID
    Free,
    /// Pro license with Identity Center
    Pro,
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
            Self::Login(args) => {
                if crate::auth::is_logged_in().await {
                    eyre::bail!(
                        "Already logged in, please logout with {} first",
                        format!("{CHAT_BINARY_NAME} logout").magenta()
                    );
                }

                login_interactive(args).await?;

                Ok(ExitCode::SUCCESS)
            },
            Self::Logout => {
                let _ = crate::auth::logout().await;

                println!("You are now logged out");
                println!(
                    "Run {} to log back in to {PRODUCT_NAME}",
                    format!("{CHAT_BINARY_NAME} login").magenta()
                );
                Ok(ExitCode::SUCCESS)
            },
            Self::Whoami { format } => {
                let builder_id = crate::auth::builder_id_token().await;

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

                        if matches!(token.token_type(), TokenType::IamIdentityCenter) {
                            if let Ok(Some(profile)) = crate::fig_settings::state::get::<
                                crate::api_client::profile::Profile,
                            >("api.codewhisperer.profile")
                            {
                                color_print::cprintln!(
                                    "\n<em>Profile:</em>\n{}\n{}\n",
                                    profile.profile_name,
                                    profile.arn
                                );
                            }
                        }
                        Ok(ExitCode::SUCCESS)
                    },
                    _ => {
                        format.print(|| "Not logged in", || json!({ "account": null }));
                        Ok(ExitCode::FAILURE)
                    },
                }
            },
            Self::Profile => {
                if !crate::fig_util::system_info::in_cloudshell() && !crate::auth::is_logged_in().await {
                    bail!(
                        "You are not logged in, please log in with {}",
                        format!("{CHAT_BINARY_NAME} login",).bold()
                    );
                }

                if let Ok(Some(token)) = crate::auth::builder_id_token().await {
                    if matches!(token.token_type(), TokenType::BuilderId) {
                        bail!("This command is only available for Pro users");
                    }
                }

                select_profile_interactive(false).await?;

                Ok(ExitCode::SUCCESS)
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
        match self {
            Self::Root(cmd) => cmd.execute().await,
        }
    }
}

pub async fn login_interactive(args: LoginArgs) -> Result<()> {
    let login_method = match args.license {
        Some(LicenseType::Free) => AuthMethod::BuilderId,
        Some(LicenseType::Pro) => AuthMethod::IdentityCenter,
        None => {
            // No license specified, prompt the user to choose
            let options = [AuthMethod::BuilderId, AuthMethod::IdentityCenter];
            let i = match choose("Select login method", &options)? {
                Some(i) => i,
                None => bail!("No login method selected"),
            };
            options[i]
        },
    };

    match login_method {
        AuthMethod::BuilderId | AuthMethod::IdentityCenter => {
            let (start_url, region) = match login_method {
                AuthMethod::BuilderId => (None, None),
                AuthMethod::IdentityCenter => {
                    let default_start_url = args.identity_provider.or_else(|| {
                        crate::fig_settings::state::get_string("auth.idc.start-url")
                            .ok()
                            .flatten()
                    });
                    let default_region = args
                        .region
                        .or_else(|| crate::fig_settings::state::get_string("auth.idc.region").ok().flatten());

                    let start_url = input("Enter Start URL", default_start_url.as_deref())?;
                    let region = input("Enter Region", default_region.as_deref())?;

                    let _ = crate::fig_settings::state::set_value("auth.idc.start-url", start_url.clone());
                    let _ = crate::fig_settings::state::set_value("auth.idc.region", region.clone());

                    (Some(start_url), Some(region))
                },
            };
            let secret_store = SecretStore::new().await?;

            // Remote machine won't be able to handle browser opening and redirects,
            // hence always use device code flow.
            if is_remote() || args.use_device_flow {
                try_device_authorization(&secret_store, start_url.clone(), region.clone()).await?;
            } else {
                let (client, registration) = start_pkce_authorization(start_url.clone(), region.clone()).await?;

                match crate::fig_util::open::open_url_async(&registration.url).await {
                    // If it succeeded, finish PKCE.
                    Ok(()) => {
                        let mut spinner = Spinner::new(vec![
                            SpinnerComponent::Spinner,
                            SpinnerComponent::Text(" Logging in...".into()),
                        ]);
                        let ctrl_c_stream = ctrl_c();
                        tokio::select! {
                            res = registration.finish(&client, Some(&secret_store)) => res?,
                            Ok(_) = ctrl_c_stream => {
                                #[allow(clippy::exit)]
                                exit(1);
                            },
                        }
                        crate::fig_telemetry::send_user_logged_in().await;
                        spinner.stop_with_message("Device authorized".into());
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

    if login_method == AuthMethod::IdentityCenter {
        select_profile_interactive(true).await?;
    }

    eprintln!("Logged in successfully");

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
    } else if let Err(err) = crate::fig_util::open::open_url_async(&device_auth.verification_uri_complete).await {
        error!(%err, "Failed to open URL with browser");
        print_open_url();
    }

    let mut spinner = Spinner::new(vec![
        SpinnerComponent::Spinner,
        SpinnerComponent::Text(" Logging in...".into()),
    ]);

    loop {
        let ctrl_c_stream = ctrl_c();
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(device_auth.interval.try_into().unwrap_or(1))) => (),
            Ok(_) = ctrl_c_stream => {
                #[allow(clippy::exit)]
                exit(1);
            }
        }
        match poll_create_token(
            secret_store,
            device_auth.device_code.clone(),
            start_url.clone(),
            region.clone(),
        )
        .await
        {
            PollCreateToken::Pending => {},
            PollCreateToken::Complete => {
                crate::fig_telemetry::send_user_logged_in().await;
                spinner.stop_with_message("Device authorized".into());
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

async fn select_profile_interactive(whoami: bool) -> Result<()> {
    let mut spinner = Spinner::new(vec![
        SpinnerComponent::Spinner,
        SpinnerComponent::Text(" Fetching profiles...".into()),
    ]);
    let profiles = list_available_profiles().await;
    if profiles.is_empty() {
        info!("Available profiles was empty");
        return Ok(());
    }

    let sso_region: Option<String> = crate::fig_settings::state::get_string("auth.idc.region").ok().flatten();
    let total_profiles = profiles.len() as i64;

    if whoami && profiles.len() == 1 {
        if let Some(profile_region) = profiles[0].arn.split(':').nth(3) {
            crate::fig_telemetry::send_profile_state(
                QProfileSwitchIntent::Update,
                profile_region.to_string(),
                TelemetryResult::Succeeded,
                sso_region,
            )
            .await;
        }
        spinner.stop_with_message(String::new());
        return Ok(crate::fig_settings::state::set_value(
            "api.codewhisperer.profile",
            serde_json::to_value(&profiles[0])?,
        )?);
    }

    let mut items: Vec<String> = profiles
        .iter()
        .map(|p| format!("{} (arn: {})", p.profile_name, p.arn))
        .collect();
    let active_profile: Option<Profile> = crate::fig_settings::state::get("api.codewhisperer.profile")?;

    if let Some(default_idx) = active_profile
        .as_ref()
        .and_then(|active| profiles.iter().position(|p| p.arn == active.arn))
    {
        items[default_idx] = format!("{} (active)", items[default_idx].as_str());
    }

    spinner.stop_with_message(String::new());
    let selected = Select::with_theme(&crate::fig_util::dialoguer_theme())
        .with_prompt("Select an IAM Identity Center profile")
        .items(&items)
        .default(0)
        .interact_opt()?;

    match selected {
        Some(i) => {
            let chosen = &profiles[i];
            let profile = serde_json::to_value(chosen)?;
            eprintln!("Set profile: {}\n", chosen.profile_name.as_str().green());
            crate::fig_settings::state::set_value("api.codewhisperer.profile", profile)?;
            crate::fig_settings::state::remove_value("api.selectedCustomization")?;

            if let Some(profile_region) = chosen.arn.split(':').nth(3) {
                let intent = if whoami {
                    QProfileSwitchIntent::Auth
                } else {
                    QProfileSwitchIntent::User
                };
                crate::fig_telemetry::send_did_select_profile(
                    intent,
                    profile_region.to_string(),
                    TelemetryResult::Succeeded,
                    sso_region,
                    Some(total_profiles),
                )
                .await;
            }
        },
        None => {
            crate::fig_telemetry::send_did_select_profile(
                QProfileSwitchIntent::User,
                "not-set".to_string(),
                TelemetryResult::Cancelled,
                sso_region,
                Some(total_profiles),
            )
            .await;
            bail!("No profile selected.\n");
        },
    }

    Ok(())
}

mod tests {
    #[test]
    #[ignore]
    fn unset_profile() {
        crate::fig_settings::state::remove_value("api.codewhisperer.profile").unwrap();
    }
}
