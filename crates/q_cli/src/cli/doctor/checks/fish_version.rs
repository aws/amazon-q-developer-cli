use std::borrow::Cow;

use async_trait::async_trait;
use eyre::Context;
use owo_colors::OwoColorize;
use semver::{
    Version,
    VersionReq,
};
use tokio::process::Command;

use crate::cli::doctor::{
    DoctorCheck,
    DoctorError,
};

const FISH_VERSION_REQUEST: &str = ">=3.3.0";

pub struct FishVersionCheck;

#[async_trait]
impl DoctorCheck for FishVersionCheck {
    fn name(&self) -> Cow<'static, str> {
        "Fish is up to date".into()
    }

    async fn check(&self, _: &()) -> Result<(), DoctorError> {
        if which::which("fish").is_err() {
            // fish is not installed, so we shouldn't check it
            return Ok(());
        }

        let output = Command::new("fish")
            .arg("--version")
            .output()
            .await
            .context("failed getting fish version")?;

        let version = Version::parse(
            &String::from_utf8_lossy(&output.stdout)
                .chars()
                .filter(|char| char.is_numeric() || char == &'.')
                .collect::<String>(),
        )
        .context("failed parsing fish version")?;

        let version_req = VersionReq::parse(FISH_VERSION_REQUEST).context("failed to parse version requirement")?;
        if version_req.matches(&version) {
            Ok(())
        } else {
            Err(DoctorError::warning(format!(
                "Using fish {version} may cause issues, it is recommended to either update to fish {} or switch to zsh.
  - Install fish with Brew: {}
  - Change shell default to ZSH: {}",
                FISH_VERSION_REQUEST,
                "brew install fish && fish".bright_magenta(),
                "chsh -s /bin/zsh && zsh".bright_magenta()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::doctor::Platform;

    #[tokio::test]
    async fn test_fish_version_check() {
        let check = FishVersionCheck;
        let name = check.name();
        let doctor_type = check.get_type(&(), Platform::current()).await;
        let result = check.check(&()).await;
        println!("{name}: {doctor_type:?} {result:?}");
    }
}
