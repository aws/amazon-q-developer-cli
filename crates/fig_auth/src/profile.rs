use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Deserialize, Serialize)]
pub struct Profile {
    pub arn: String,
    pub profile_name: String,
}

impl From<amzn_codewhisperer_client::types::Profile> for Profile {
    fn from(profile: amzn_codewhisperer_client::types::Profile) -> Self {
        Self {
            arn: profile.arn,
            profile_name: profile.profile_name,
        }
    }
}

pub fn set_profile(profile: Profile) -> Result<(), Error> {
    Ok(fig_settings::state::set_value(
        "api.codewhisperer.service",
        serde_json::to_string(&profile)?,
    )?)
}
