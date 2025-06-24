use std::borrow::Cow;

use async_trait::async_trait;
use eyre::Context;
use fig_auth::is_amzn_user;
use fig_util::CLI_BINARY_NAME;
use fig_util::env_var::Q_PARENT;
use fig_util::url::AUTOCOMPLETE_SSH_WIKI;
use owo_colors::OwoColorize;
use regex::Regex;

use crate::cli::doctor::{
    DoctorCheck,
    DoctorCheckType,
    DoctorError,
    Platform,
};

pub struct SshdConfigCheck;

#[async_trait]
impl DoctorCheck<()> for SshdConfigCheck {
    fn name(&self) -> Cow<'static, str> {
        "sshd config".into()
    }

    async fn check(&self, _: &()) -> Result<(), DoctorError> {
        let info = vec![
            "The /etc/ssh/sshd_config file needs to have the following line:".into(),
            "  AcceptEnv Q_SET_PARENT".magenta().to_string().into(),
            "  AllowStreamLocalForwarding yes".magenta().to_string().into(),
            "".into(),
            "If your sshd_config is already configured correctly then:".into(),
            format!(
                "  1. Restart sshd, if using systemd: {}",
                "sudo systemctl restart sshd".bold()
            )
            .into(),
            "  2. Disconnect from the remote host".into(),
            format!(
                "  3. Run {} on the local machine",
                format!("{CLI_BINARY_NAME} integrations install ssh",).bold()
            )
            .into(),
            format!(
                "  4. Reconnect to the remote host and run {} again",
                format!("{CLI_BINARY_NAME} doctor").bold()
            )
            .into(),
            "".into(),
            format!("See {AUTOCOMPLETE_SSH_WIKI} for more info").into(),
        ];

        let sshd_config_path = "/etc/ssh/sshd_config";

        let sshd_config = match std::fs::read_to_string(sshd_config_path).context("Could not read sshd_config") {
            Ok(config) => config,
            Err(_err) if std::env::var_os(Q_PARENT).is_some() => {
                // We will assume amzn users have this configured correctly and warn other users.
                if is_amzn_user().await.unwrap_or_default() {
                    return Ok(());
                } else {
                    return Err(DoctorError::warning(format!(
                        "Could not read sshd_config, check {AUTOCOMPLETE_SSH_WIKI} for more info",
                    )));
                }
            },
            Err(err) => {
                return Err(DoctorError::Error {
                    reason: err.to_string().into(),
                    info: info.clone(),
                    fix: None,
                    error: None,
                });
            },
        };

        let is_valid = is_sshd_config_valid(&sshd_config);
        if is_valid {
            Ok(())
        } else {
            Err(DoctorError::Error {
                reason: "SSHD config is not set up correctly".into(),
                info,
                fix: None,
                error: None,
            })
        }
    }

    async fn get_type(&self, _: &(), _: Platform) -> DoctorCheckType {
        if fig_util::system_info::in_ssh() {
            DoctorCheckType::NormalCheck
        } else {
            DoctorCheckType::NoCheck
        }
    }
}

fn is_sshd_config_valid(sshd_config: &str) -> bool {
    let accept_env_regex = Regex::new(r"(?m)^\s*AcceptEnv\s+.*(Q_\*|Q_SET_PARENT)([^\S\r\n]+.*$|$)").unwrap();

    let allow_stream_local_forwarding_regex =
        Regex::new(r"(?m)^\s*AllowStreamLocalForwarding\s+yes([^\S\r\n]+.*$|$)").unwrap();

    let accept_env_match = accept_env_regex.is_match(sshd_config);
    let allow_stream_local_forwarding_match = allow_stream_local_forwarding_regex.is_match(sshd_config);

    accept_env_match && allow_stream_local_forwarding_match
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fish_version_check() {
        let check = SshdConfigCheck;
        let name = check.name();
        let doctor_type = check.get_type(&(), Platform::current()).await;
        let result = check.check(&()).await;
        println!("{name}: {doctor_type:?} {result:?}");
    }

    #[test]
    fn test_is_sshd_config_valid() {
        let valid_configs = [
            "AcceptEnv Q_SET_PARENT\nAllowStreamLocalForwarding yes",
            "AcceptEnv Q_SET_PARENT\nAllowStreamLocalForwarding yes\n# Some comment",
            "# Some comment\nAcceptEnv Q_SET_PARENT\nAllowStreamLocalForwarding yes",
            "Other config 1\nAcceptEnv Q_SET_PARENT\nAllowStreamLocalForwarding yes\n# Some other comment\nOther config 2",
        ];

        let invalid_config = [
            "AcceptEnv Q_SET_PARENT\nAllowStreamLocalForwarding no",
            "AcceptEnv Q_SET_PARENT\n# AllowStreamLocalForwarding yes",
            "Other config 1\nAllowStreamLocalForwarding yes\nOther config 2",
        ];

        for config in valid_configs {
            assert!(is_sshd_config_valid(config), "Expected config to be valid: {config}");
        }

        for config in invalid_config {
            assert!(!is_sshd_config_valid(config), "Expected config to be invalid: {config}");
        }
    }
}
