use std::str::FromStr;

use aws_sdk_cognitoidentity::primitives::DateTimeFormat;
use aws_sdk_cognitoidentity::types::Credentials;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::{
    Map,
    Value,
};
use uuid::Uuid;

use super::{
    Database,
    DatabaseError,
    Table,
};

const CREDENTIALS_KEY: &str = "telemetry-cognito-credentials";
const CLIENT_ID_KEY: &str = "telemetryClientId";
const CODEWHISPERER_PROFILE_KEY: &str = "api.codewhisperer.profile";
const START_URL_KEY: &str = "auth.idc.start-url";
const IDC_REGION_KEY: &str = "auth.idc.region";
const CUSTOMIZATION_STATE_KEY: &str = "api.selectedCustomization";
const ROTATING_TIP_KEY: &str = "chat.greeting.rotating_tips_current_index";

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct CredentialsJson {
    pub access_key_id: Option<String>,
    pub secret_key: Option<String>,
    pub session_token: Option<String>,
    pub expiration: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthProfile {
    pub arn: String,
    pub profile_name: String,
}

impl From<amzn_codewhisperer_client::types::Profile> for AuthProfile {
    fn from(profile: amzn_codewhisperer_client::types::Profile) -> Self {
        Self {
            arn: profile.arn,
            profile_name: profile.profile_name,
        }
    }
}

pub trait StateDatabase {
    /// Get all entries for dumping the persistent application state.
    fn get_all_entries(&self) -> Result<Map<String, serde_json::Value>, DatabaseError>;

    /// Get cognito credentials used by toolkit telemetry.
    fn get_credentials_entry(&mut self) -> Result<Option<CredentialsJson>, DatabaseError>;
    /// Set cognito credentials used by toolkit telemetry.
    fn set_credentials_entry(&mut self, credentials: &Credentials) -> Result<(), DatabaseError>;

    /// Get the current user profile used to determine API endpoints.
    fn get_auth_profile(&self) -> Result<Option<AuthProfile>, DatabaseError>;
    /// Set the current user profile used to determine API endpoints.
    fn set_auth_profile(&mut self, profile: &AuthProfile) -> Result<(), DatabaseError>;
    /// Unset the current user profile used to determine API endpoints.
    fn unset_auth_profile(&mut self) -> Result<(), DatabaseError>;

    // Get the client ID used for telemetry requests.
    fn get_client_id(&mut self) -> Option<Uuid>;
    // Set the client ID used for telemetry requests.
    fn set_client_id(&mut self, client_id: Uuid) -> Result<(), DatabaseError>;

    // Get the start URL used for IdC login.
    fn get_start_url(&mut self) -> Option<String>;
    // Set the start URL used for IdC login.
    fn set_start_url(&mut self, start_url: String) -> Result<(), DatabaseError>;

    // Get the region used for IdC login.
    fn get_idc_region(&mut self) -> Option<String>;
    // Set the region used for IdC login.
    fn set_idc_region(&mut self, region: String) -> Result<(), DatabaseError>;

    // Get the rotating tip used for chat then post increment.
    fn get_increment_rotating_tip(&mut self) -> Result<usize, DatabaseError>;
}

impl StateDatabase for Database {
    fn get_all_entries(&self) -> Result<Map<String, Value>, DatabaseError> {
        self.all_entries(Table::State)
    }

    fn get_credentials_entry(&mut self) -> Result<Option<CredentialsJson>, DatabaseError> {
        Ok(match self.get_entry::<String>(Table::State, CREDENTIALS_KEY)? {
            Some(entry) => Some(serde_json::from_str(&entry)?),
            None => None,
        })
    }

    fn set_credentials_entry(&mut self, credentials: &Credentials) -> Result<(), DatabaseError> {
        let json = serde_json::to_value(CredentialsJson {
            access_key_id: credentials.access_key_id.clone(),
            secret_key: credentials.secret_key.clone(),
            session_token: credentials.session_token.clone(),
            expiration: credentials
                .expiration
                .and_then(|t| t.fmt(DateTimeFormat::DateTime).ok()),
        })?;

        self.set_entry(Table::State, CREDENTIALS_KEY, json)
    }

    fn get_auth_profile(&self) -> Result<Option<AuthProfile>, DatabaseError> {
        Ok(self
            .get_entry::<serde_json::Value>(Table::State, CODEWHISPERER_PROFILE_KEY)?
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()?)
    }

    fn set_auth_profile(&mut self, profile: &AuthProfile) -> Result<(), DatabaseError> {
        self.set_entry(Table::State, CODEWHISPERER_PROFILE_KEY, serde_json::to_value(profile)?)?;
        self.delete_entry(Table::State, CUSTOMIZATION_STATE_KEY)
    }

    fn unset_auth_profile(&mut self) -> Result<(), DatabaseError> {
        self.delete_entry(Table::State, CODEWHISPERER_PROFILE_KEY)?;
        self.delete_entry(Table::State, CUSTOMIZATION_STATE_KEY)
    }

    fn get_client_id(&mut self) -> Option<Uuid> {
        self.get_string(Table::State, CLIENT_ID_KEY)
            .ok()
            .flatten()
            .and_then(|s| Uuid::from_str(&s).ok())
    }

    fn set_client_id(&mut self, client_id: Uuid) -> Result<(), DatabaseError> {
        self.set_entry(Table::State, CLIENT_ID_KEY, client_id.to_string())
    }

    fn get_start_url(&mut self) -> Option<String> {
        self.get_string(Table::State, START_URL_KEY).ok().flatten()
    }

    fn set_start_url(&mut self, start_url: String) -> Result<(), DatabaseError> {
        self.set_entry(Table::State, START_URL_KEY, start_url)
    }

    fn get_idc_region(&mut self) -> Option<String> {
        self.get_string(Table::State, IDC_REGION_KEY).ok().flatten()
    }

    fn set_idc_region(&mut self, region: String) -> Result<(), DatabaseError> {
        self.set_entry(Table::State, IDC_REGION_KEY, region)
    }

    fn get_increment_rotating_tip(&mut self) -> Result<usize, DatabaseError> {
        let tip: usize = self.get_entry(Table::State, ROTATING_TIP_KEY)?.unwrap_or(0);
        self.set_entry(Table::State, ROTATING_TIP_KEY, tip.wrapping_add(1))?;
        Ok(tip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn state_table_tests() {
        let db = Database::new().await.unwrap();

        // set
        db.set_entry(Table::State, "test", "test").unwrap();
        db.set_entry(Table::State, "int", 1).unwrap();
        db.set_entry(Table::State, "float", 1.0).unwrap();
        db.set_entry(Table::State, "bool", true).unwrap();
        db.set_entry(Table::State, "array", vec![1, 2, 3]).unwrap();
        db.set_entry(Table::State, "object", serde_json::json!({ "test": "test" }))
            .unwrap();
        db.set_entry(Table::State, "binary", b"test".to_vec()).unwrap();

        // unset
        db.delete_entry(Table::State, "test").unwrap();
        db.delete_entry(Table::State, "int").unwrap();

        // is some
        assert!(db.get_entry::<String>(Table::State, "test").unwrap().is_none());
        assert!(db.get_entry::<i32>(Table::State, "int").unwrap().is_none());
        assert!(db.get_entry::<f64>(Table::State, "float").unwrap().is_some());
        assert!(db.get_entry::<bool>(Table::State, "bool").unwrap().is_some());
    }
}
