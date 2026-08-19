pub mod diagnostics;
mod env;
mod fs;
mod sysinfo;

use std::sync::Arc;

pub use env::Env;
use eyre::Result;
pub use fs::Fs;
pub use sysinfo::SysInfo;

use crate::api_client::ApiClient;
use crate::database::{
    AuthProfile,
    Database,
    DatabaseError,
};
use crate::telemetry::{
    TelemetryThread,
    build_v2_host_config,
};
use crate::util::log_on_err::LogOnErr;

const WINDOWS_USER_HOME: &str = "C:\\Users\\testuser";
const UNIX_USER_HOME: &str = "/home/testuser";

pub const ACTIVE_USER_HOME: &str = if cfg!(windows) {
    WINDOWS_USER_HOME
} else {
    UNIX_USER_HOME
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TelemetryIdentityValue {
    Present(String),
    Absent,
}

impl TelemetryIdentityValue {
    pub(crate) fn notification_value(&self) -> &str {
        match self {
            Self::Present(user_id) => user_id,
            Self::Absent => "",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TelemetryIdentityUpdate {
    pub(crate) value: TelemetryIdentityValue,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum TelemetryIdentityUpdateError {
    #[error("telemetry identity was invalid")]
    Invalid,
    #[error("failed to persist telemetry identity: {0}")]
    Persistence(#[from] DatabaseError),
}

// TODO OS SHOULD NOT BE CLONE

/// Struct that contains the interface to every system related IO operation.
///
/// Every operation that accesses the file system, environment, or other related platform
/// primitives should be done through a [Context] as this enables testing otherwise untestable
/// code paths in unit tests.
#[derive(Clone, Debug)]
pub struct Os {
    pub env: Env,
    pub fs: Fs,
    pub sysinfo: SysInfo,
    pub database: Database,
    pub client: ApiClient,
    pub telemetry: TelemetryThread,
}

impl Os {
    pub async fn new() -> Result<Self> {
        let env = Env::new();
        let fs = Fs::new();
        let mut database =
            Database::new_with_workspace(crate::util::paths::WorkspacePaths::settings_path_for_env(&env).ok())
                .await
                .log_on_err("Os::new: Database::new_with_workspace failed")?;

        let endpoint = if crate::util::env_var::get_api_key().is_some() {
            crate::api_client::profile::discover_endpoint_for_api_key(&env, &fs, &mut database).await
        } else {
            None
        };

        let client = ApiClient::new(&env, &fs, &mut database, endpoint)
            .await
            .log_on_err("Os::new: ApiClient::new failed")?;
        let region = client.region().to_string();
        let persisted_user_id = database.get_telemetry_user_id().unwrap_or_else(|error| {
            tracing::warn!(%error, "Failed to load persisted telemetry identity");
            None
        });
        let host_config = build_v2_host_config(&env, &fs, &mut database, Some(&region), persisted_user_id)
            .await
            .log_on_err("Os::new: build_v2_host_config failed")?;
        let telemetry = TelemetryThread::new(host_config)
            .await
            .log_on_err("Os::new: TelemetryThread::new failed")?;

        crate::rollout::Rollout::init(
            database.get_client_id().ok().flatten(),
            database.get_start_url().ok().flatten(),
        );

        Ok(Self {
            env,
            fs,
            sysinfo: SysInfo::new(),
            database,
            client,
            telemetry,
        })
    }

    /// This method is for "refreshing" or re-initializing resources (ApiClient and TelemetryThread)
    /// that can be initialized before the application is aware of the region that these resources
    /// should be configured with, namely before login occurs.
    /// Ideally these resources should be refactored out of the Os struct
    pub async fn set_auth_profile(&mut self, profile: &AuthProfile) -> Result<()> {
        let identity_epochs = self.telemetry.identity_epochs();
        identity_epochs.clear(|| self.database.clear_telemetry_user_id())?;
        self.database.set_auth_profile(profile)?;

        self.client
            .refresh_auth_profile(&self.env, &self.fs, &mut self.database)
            .await?;

        let region = self.client.region().to_string();
        let host_config = build_v2_host_config(&self.env, &self.fs, &mut self.database, Some(&region), None).await?;
        let old_telemetry = std::mem::replace(&mut self.telemetry, TelemetryThread::new(host_config).await?);

        old_telemetry.finish().await?;
        Ok(())
    }

    pub(crate) fn telemetry_identity_epochs(&self) -> Arc<kiro_telemetry::IdentityEpochs> {
        self.telemetry.identity_epochs()
    }

    pub(crate) fn current_telemetry_identity_update(&self) -> TelemetryIdentityUpdate {
        let epochs = self.telemetry.identity_epochs();
        let value = epochs
            .resolve(epochs.current())
            .map_or(TelemetryIdentityValue::Absent, |user_id| {
                TelemetryIdentityValue::Present(user_id.to_string())
            });
        TelemetryIdentityUpdate { value }
    }

    pub(crate) fn update_telemetry_user_id(
        &self,
        epochs: &kiro_telemetry::IdentityEpochs,
        observed: u64,
        user_id: &str,
    ) -> Result<Option<TelemetryIdentityUpdate>, TelemetryIdentityUpdateError> {
        Self::update_telemetry_user_id_with(epochs, observed, user_id, |user_id| {
            self.database.set_telemetry_user_id(user_id).map(|_| ())
        })
    }

    fn update_telemetry_user_id_with(
        epochs: &kiro_telemetry::IdentityEpochs,
        observed: u64,
        user_id: &str,
        persist: impl FnOnce(&str) -> Result<(), DatabaseError>,
    ) -> Result<Option<TelemetryIdentityUpdate>, TelemetryIdentityUpdateError> {
        match epochs.identify(observed, user_id, persist) {
            Ok(kiro_telemetry::IdentifyOutcome::Stale) => Ok(None),
            Ok(kiro_telemetry::IdentifyOutcome::Identified) => {
                let value = epochs
                    .resolve(epochs.current())
                    .map_or(TelemetryIdentityValue::Absent, |user_id| {
                        TelemetryIdentityValue::Present(user_id.to_string())
                    });
                Ok(Some(TelemetryIdentityUpdate { value }))
            },
            Err(kiro_telemetry::IdentifyError::Invalid) => Err(TelemetryIdentityUpdateError::Invalid),
            Err(kiro_telemetry::IdentifyError::Persist(error)) => Err(TelemetryIdentityUpdateError::Persistence(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::AuthProfile;

    #[tokio::test]
    async fn test_context_builder_with_test_home() {
        let os = Os::new().await.unwrap();
        unsafe {
            os.env.set_var("hello", "world");
        }

        #[cfg(windows)]
        {
            assert!(os.fs.try_exists(ACTIVE_USER_HOME).await.unwrap());
            assert_eq!(os.env.get("USERPROFILE").unwrap(), ACTIVE_USER_HOME);
        }
        #[cfg(not(windows))]
        {
            assert!(os.fs.try_exists(ACTIVE_USER_HOME).await.unwrap());
            assert_eq!(os.env.get("HOME").unwrap(), ACTIVE_USER_HOME);
        }

        assert_eq!(os.env.get("hello").unwrap(), "world");
    }

    #[tokio::test]
    async fn test_set_auth_profile() {
        let mut os = Os::new().await.unwrap();

        let profile = AuthProfile {
            arn: "arn:aws-us-gov:codewhisperer:us-gov-east-1:123456789012:profile/C39QMYEDUAKW".to_string(),
            profile_name: "test-gov-east-profile".to_string(),
        };

        os.database.set_telemetry_user_id("stale-user-id").unwrap();
        os.set_auth_profile(&profile).await.unwrap();
        assert_eq!(os.database.get_auth_profile().unwrap().unwrap(), profile);
        assert_eq!(os.database.get_telemetry_user_id().unwrap(), None);
        assert_eq!(os.client.get_profile().unwrap(), profile);
        assert_eq!(os.client.region(), "us-gov-east-1");
        let epochs = os.telemetry_identity_epochs();
        assert_eq!(epochs.resolve(epochs.current()), None);
    }

    #[tokio::test]
    async fn stale_identity_completion_is_rejected_after_auth_transition() {
        let os = Os::new().await.unwrap();
        let epochs = os.telemetry_identity_epochs();
        let observed = epochs.current();
        epochs.clear::<()>(|| Ok(())).unwrap();

        assert!(
            os.update_telemetry_user_id(&epochs, observed, "stale-user-id")
                .unwrap()
                .is_none()
        );
        assert_eq!(os.database.get_telemetry_user_id().unwrap(), None);
    }

    #[tokio::test]
    async fn persistence_failure_is_reported_without_publishing_identity_effect() {
        let os = Os::new().await.unwrap();
        let epochs = os.telemetry_identity_epochs();
        let observed = epochs.current();
        let result = Os::update_telemetry_user_id_with(&epochs, observed, "user-id", |_| {
            Err(std::io::Error::other("injected write failure").into())
        });

        assert!(matches!(result, Err(TelemetryIdentityUpdateError::Persistence(_))));
        assert_eq!(epochs.current(), observed);
        assert_eq!(epochs.resolve(observed), None);
        assert_eq!(os.database.get_telemetry_user_id().unwrap(), None);
    }
}
