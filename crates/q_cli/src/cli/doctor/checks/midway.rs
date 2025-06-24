use std::borrow::Cow;

use async_trait::async_trait;
use fig_util::consts::CLI_BINARY_NAME;
use owo_colors::OwoColorize;
use which::which;

use crate::cli::doctor::{
    DoctorCheck,
    DoctorCheckType,
    DoctorError,
    Platform,
};

pub struct MidwayCheck;

#[async_trait]
impl DoctorCheck for MidwayCheck {
    fn name(&self) -> Cow<'static, str> {
        "Midway Auth".into()
    }

    async fn get_type(&self, _: &(), _: Platform) -> DoctorCheckType {
        let amzn_user = matches!(fig_auth::is_amzn_user().await, Ok(true));
        let has_mwinit = which("mwinit").is_ok();

        if amzn_user && has_mwinit {
            DoctorCheckType::NormalCheck
        } else {
            DoctorCheckType::NoCheck
        }
    }

    async fn check(&self, _: &()) -> Result<(), DoctorError> {
        let url = url::Url::parse("https://prod.us-east-1.shellspecs.jupiter.ai.aws.dev/index.json").unwrap();
        match fig_request::midway::midway_request(url).await {
            Ok(_) => Ok(()),
            Err(err) => Err(DoctorError::Error {
                reason: "Failed to make midway request".into(),
                info: vec![
                    format!(
                        "Try running {} and restarting the app with {}.",
                        "mwinit".magenta(),
                        format!("{CLI_BINARY_NAME} restart").magenta()
                    )
                    .into(),
                ],
                fix: None,
                error: Some(err.into()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_midway_check() {
        let check = MidwayCheck;
        let name = check.name();
        let doctor_type = check.get_type(&(), Platform::current()).await;
        let result = check.check(&()).await;
        println!("{name}: {doctor_type:?} {result:?}");
    }
}
