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
use crate::auth::builder_id::{
    BuilderIdToken,
    PollCreateToken,
    TokenType,
    poll_create_token,
    start_device_authorization,
};
use crate::auth::pkce::start_pkce_authorization;
use crate::auth::portal::{
    PortalResult,
    start_unified_auth,
};
use crate::auth::social::SocialProvider;
use crate::constants::PRODUCT_NAME;
use crate::database::Database;
use crate::os::Os;
use crate::telemetry::{
    QProfileSwitchIntent,
    TelemetryResult,
};
use crate::util::spinner::{
    Spinner,
    SpinnerComponent,
};
use crate::util::system_info::is_remote;
use crate::util::{
    CLI_BINARY_NAME,
    choose,
    input,
};

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

    /// Social provider (google or github)
    #[arg(long, value_enum)]
    pub social: Option<SocialProvider>,

    /// Always use the OAuth device flow for authentication. Useful for instances where browser
    /// redirects cannot be handled.
    #[arg(long)]
    pub use_device_flow: bool,
}

impl LoginArgs {
    pub async fn execute(self, os: &mut Os) -> Result<ExitCode> {
        if is_logged_in(&mut os.database).await {
            eyre::bail!(
                "Already logged in, please logout with {} first",
                format!("{CLI_BINARY_NAME} logout").magenta()
            );
        }

        let is_remote_env = is_remote() || self.use_device_flow;

        if !is_remote_env {
            // LOCAL ENVIRONMENT: Ignore all CLI flags and always use unified auth portal
            info!("Using unified auth portal for login");

            let mut pre_portal_spinner = Spinner::new(vec![
                SpinnerComponent::Spinner,
                SpinnerComponent::Text(" Opening auth portal and logging in...".into()),
            ]);
            match start_unified_auth(&mut os.database).await? {
                PortalResult::Social(provider) => {
                    pre_portal_spinner.stop_with_message(format!("Logged in with {}", provider));
                    os.telemetry.send_user_logged_in(None, None, Some(provider)).ok();
                    return Ok(ExitCode::SUCCESS);
                },
                PortalResult::Internal { issuer_uri } => {
                    pre_portal_spinner.stop();
                    // EXACTLY same with original PKCE path: this registers client + completes auth.
                    let (client, registration) = start_pkce_authorization(Some(issuer_uri.clone()), None).await?;

                    match crate::util::open::open_url_async(&registration.url).await {
                        // If it succeeded, finish PKCE.
                        Ok(()) => {
                            let mut spinner = Spinner::new(vec![
                                SpinnerComponent::Spinner,
                                SpinnerComponent::Text(" Logging in...".into()),
                            ]);
                            let issuer_url = registration.issuer_url().to_string();
                            let region = registration.region().to_string();
                            let ctrl_c_stream = ctrl_c();
                            tokio::select! {
                                res = registration.finish(&client, Some(&mut os.database)) => res?,
                                Ok(_) = ctrl_c_stream => {
                                    #[allow(clippy::exit)]
                                    exit(1);
                                },
                            }
                            os.telemetry
                                .send_user_logged_in(Some(issuer_url), Some(region), None)
                                .ok();
                            spinner.stop_with_message("Logged in".into());
                        },
                        // If we are unable to open the link with the browser, then fallback to
                        // the device code flow.
                        Err(err) => {
                            // Try device code flow.
                            error!(%err, "Failed to open URL with browser, falling back to device code flow");
                            try_device_authorization(os, Some(issuer_uri.clone()), None).await?;
                        },
                    }
                },
            }
        } else {
            // REMOTE ENVIRONMENT: Use existing device flow for BuilderID/IdC only
            info!("Remote environment detected - using device flow authentication");

            // Social login is not supported in remote environments
            if self.social.is_some() {
                bail!(
                    "Social login is not supported in remote environments. Please use BuilderID or Identity Center authentication."
                );
            }

            // Show menu for BuilderID or IdC only
            let login_method = match self.license {
                Some(LicenseType::Free) => AuthMethod::BuilderId,
                Some(LicenseType::Pro) => AuthMethod::IdentityCenter,
                None => {
                    if self.identity_provider.is_some() && self.region.is_some() {
                        // If license is specified and --identity-provider and --region are specified,
                        // the license is determined to be pro
                        AuthMethod::IdentityCenter
                    } else {
                        // --license is not specified, prompt the user to choose for remote
                        let options = [AuthMethod::BuilderId, AuthMethod::IdentityCenter];
                        let prompt = "Select login method (Social login not available in remote environment)";
                        let i = match choose(prompt, &options)? {
                            Some(i) => i,
                            None => bail!("No login method selected"),
                        };
                        options[i]
                    }
                },
            };

            match login_method {
                AuthMethod::BuilderId | AuthMethod::IdentityCenter => {
                    let (start_url, region) = match login_method {
                        AuthMethod::BuilderId => (None, None),
                        AuthMethod::IdentityCenter => {
                            let default_start_url = match self.identity_provider {
                                Some(start_url) => Some(start_url),
                                None => os.database.get_start_url()?,
                            };
                            let default_region = match self.region {
                                Some(region) => Some(region),
                                None => os.database.get_idc_region()?,
                            };

                            let start_url = input("Enter Start URL", default_start_url.as_deref())?;
                            let region = input("Enter Region", default_region.as_deref())?.trim().to_string();

                            let _ = os.database.set_start_url(start_url.clone());
                            let _ = os.database.set_idc_region(region.clone());

                            (Some(start_url), Some(region))
                        },
                    };

                    // Remote machine won't be able to handle browser opening and redirects,
                    // hence always use device code flow.
                    try_device_authorization(os, start_url.clone(), region.clone()).await?;

                    if login_method == AuthMethod::IdentityCenter {
                        select_profile_interactive(os, true).await?;
                    }
                },
            }
        }

        Ok(ExitCode::SUCCESS)
    }
}

pub async fn logout(os: &mut Os) -> Result<ExitCode> {
    let _ = crate::auth::logout(&mut os.database).await;
    let _ = crate::auth::social::logout_social(&os.database).await;

    eprintln!("You are now logged out");
    eprintln!(
        "Run {} to log back in to {PRODUCT_NAME}",
        format!("{CLI_BINARY_NAME} login").magenta()
    );

    Ok(ExitCode::SUCCESS)
}

pub async fn is_logged_in(db: &mut Database) -> bool {
    if crate::auth::is_builder_id_logged_in(db).await {
        return true;
    }
    crate::auth::social::is_social_logged_in(&*db).await
}
#[derive(Args, Debug, PartialEq, Eq, Clone, Default)]
pub struct WhoamiArgs {
    /// Output format to use
    #[arg(long, short, value_enum, default_value_t)]
    format: OutputFormat,
}

impl WhoamiArgs {
    pub async fn execute(self, os: &mut Os) -> Result<ExitCode> {
        // Check for BuilderId/IDC token
        if let Ok(Some(token)) = BuilderIdToken::load(&os.database).await {
            self.format.print(
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
                if let Ok(Some(profile)) = os.database.get_auth_profile() {
                    color_print::cprintln!("\n<em>Profile:</em>\n{}\n{}\n", profile.profile_name, profile.arn);
                }
            }

            return Ok(ExitCode::SUCCESS);
        }

        // Check for social login token
        if let Ok(Some(social_token)) = crate::auth::social::SocialToken::load(&os.database).await {
            self.format.print(
                || format!("Logged in with {}", social_token.provider),
                || {
                    json!({
                        "accountType": "Social",
                        "provider": social_token.provider.to_string(),
                    })
                },
            );
            return Ok(ExitCode::SUCCESS);
        }

        self.format.print(|| "Not logged in", || json!({ "account": null }));
        Ok(ExitCode::FAILURE)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum LicenseType {
    /// Free license (Builder ID)
    Free,
    /// Pro license with Identity Center
    Pro,
}

pub async fn profile(os: &mut Os) -> Result<ExitCode> {
    if let Ok(Some(token)) = BuilderIdToken::load(&os.database).await {
        if matches!(token.token_type(), TokenType::BuilderId) {
            bail!("This command is only available for Pro users");
        }
    }

    select_profile_interactive(os, false).await?;

    Ok(ExitCode::SUCCESS)
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

#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum UserSubcommand {
    Profile,
}

async fn try_device_authorization(os: &mut Os, start_url: Option<String>, region: Option<String>) -> Result<()> {
    let device_auth = start_device_authorization(&os.database, start_url.clone(), region.clone()).await?;

    println!();
    println!("Confirm the following code in the browser");
    println!("Code: {}", device_auth.user_code.bold());
    println!();

    let print_open_url = || println!("Open this URL: {}", device_auth.verification_uri_complete);

    if is_remote() {
        print_open_url();
    } else if let Err(err) = crate::util::open::open_url_async(&device_auth.verification_uri_complete).await {
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
            &os.database,
            device_auth.device_code.clone(),
            start_url.clone(),
            region.clone(),
        )
        .await
        {
            PollCreateToken::Pending => {},
            PollCreateToken::Complete => {
                os.telemetry.send_user_logged_in(start_url, region, None).ok();
                spinner.stop_with_message("Logged in".into());
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

async fn select_profile_interactive(os: &mut Os, whoami: bool) -> Result<()> {
    let mut spinner = Spinner::new(vec![
        SpinnerComponent::Spinner,
        SpinnerComponent::Text(" Fetching profiles...".into()),
    ]);
    let profiles = list_available_profiles(&os.env, &os.fs, &mut os.database).await?;
    if profiles.is_empty() {
        info!("Available profiles was empty");
        return Ok(());
    }

    let sso_region = os.database.get_idc_region()?;
    let total_profiles = profiles.len() as i64;

    if whoami && profiles.len() == 1 {
        if let Some(profile_region) = profiles[0].arn.split(':').nth(3) {
            os.telemetry
                .send_profile_state(
                    QProfileSwitchIntent::Update,
                    profile_region.to_string(),
                    TelemetryResult::Succeeded,
                    sso_region,
                )
                .ok();
        }

        spinner.stop_with_message(String::new());
        os.database.set_auth_profile(&profiles[0])?;
        return Ok(());
    }

    let mut items: Vec<String> = profiles
        .iter()
        .map(|p| format!("{} (arn: {})", p.profile_name, p.arn))
        .collect();
    let active_profile = os.database.get_auth_profile()?;

    if let Some(default_idx) = active_profile
        .as_ref()
        .and_then(|active| profiles.iter().position(|p| p.arn == active.arn))
    {
        items[default_idx] = format!("{} (active)", items[default_idx].as_str());
    }

    spinner.stop_with_message(String::new());
    let selected = Select::with_theme(&crate::util::dialoguer_theme())
        .with_prompt("Select an IAM Identity Center profile")
        .items(&items)
        .default(0)
        .interact_opt()?;

    match selected {
        Some(i) => {
            let chosen = &profiles[i];
            eprintln!("Profile set");
            os.database.set_auth_profile(chosen)?;

            if let Some(profile_region) = chosen.arn.split(':').nth(3) {
                let intent = if whoami {
                    QProfileSwitchIntent::Auth
                } else {
                    QProfileSwitchIntent::User
                };

                os.telemetry
                    .send_did_select_profile(
                        intent,
                        profile_region.to_string(),
                        TelemetryResult::Succeeded,
                        sso_region,
                        Some(total_profiles),
                    )
                    .ok();
            }
        },
        None => {
            os.telemetry
                .send_did_select_profile(
                    QProfileSwitchIntent::User,
                    "not-set".to_string(),
                    TelemetryResult::Cancelled,
                    sso_region,
                    Some(total_profiles),
                )
                .ok();

            bail!("No profile selected.\n");
        },
    }

    Ok(())
}
