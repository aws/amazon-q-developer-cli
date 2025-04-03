use std::borrow::Cow;

use async_trait::async_trait;
use eyre::Context;
use fig_util::Shell;
use owo_colors::OwoColorize;
use semver::{
    Version,
    VersionReq,
};

use crate::cli::doctor::{
    DoctorCheck,
    DoctorCheckType,
    DoctorError,
    Platform,
};

const BASH_VERSION_REQUEST: &str = ">=5.0.0";

pub struct BashVersionCheck;

#[async_trait]
impl DoctorCheck for BashVersionCheck {
    fn name(&self) -> Cow<'static, str> {
        "Bash is up to date".into()
    }

    async fn get_type(&self, _: &(), _platform: Platform) -> DoctorCheckType {
        if Shell::current_shell() == Some(Shell::Bash) {
            DoctorCheckType::SoftCheck
        } else {
            DoctorCheckType::NoCheck
        }
    }

    async fn check(&self, _: &()) -> Result<(), DoctorError> {
        let (_, version) = Shell::current_shell_version()
            .await
            .context("Failed to get bash versions")?;

        let version = Version::parse(&version).context("Failed to parse bash version")?;

        let version_req = VersionReq::parse(BASH_VERSION_REQUEST).context("failed to parse version requirement")?;
        if version_req.matches(&version) {
            Ok(())
        } else {
            Err(DoctorError::warning(format!(
                "Using Bash {} may cause issues, it is recommended to either update to bash {} or switch to zsh.
  - Install Bash with Brew: {}
  - Change shell default to ZSH: {}",
                version,
                BASH_VERSION_REQUEST,
                "brew install bash && bash".bright_magenta(),
                "chsh -s /bin/zsh && zsh".bright_magenta()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bash_version_check() {
        let check = BashVersionCheck;
        let name = check.name();
        let doctor_type = check.get_type(&(), Platform::current()).await;
        let result = check.check(&()).await;
        println!("{name}: {doctor_type:?} {result:?}");
    }
}
