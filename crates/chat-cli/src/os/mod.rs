#![allow(dead_code)]

use std::time::Duration;

pub mod diagnostics;
mod env;
mod fs;
mod sysinfo;

use chat_cli_v2::util::log_on_err::LogOnErr;
pub use env::Env;
use eyre::Result;
pub use fs::Fs;
pub use sysinfo::SysInfo;

use crate::api_client::ApiClient;
use crate::auth::builder_id::BuilderIdToken;
use crate::database::{
    AuthProfile,
    Database,
};
use crate::rollout::Rollout;
use crate::telemetry::{
    TelemetryThread,
    telemetry_enabled,
};

const WINDOWS_USER_HOME: &str = "C:\\Users\\testuser";
const UNIX_USER_HOME: &str = "/home/testuser";

pub const ACTIVE_USER_HOME: &str = if cfg!(windows) {
    WINDOWS_USER_HOME
} else {
    UNIX_USER_HOME
};

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
        let mut database = Database::new(&env, &fs)
            .await
            .log_on_err("Os::new: Database::new failed")?;

        // For API key users, discover the correct regional endpoint before creating the client.
        let endpoint = if crate::util::env_var::get_api_key().is_some() {
            crate::api_client::profile::discover_endpoint_for_api_key(&env, &fs, &mut database).await
        } else {
            None
        };

        let client = ApiClient::new(&env, &fs, &mut database, endpoint)
            .await
            .log_on_err("Os::new: ApiClient::new failed")?;
        let region = client.region().to_string();
        let token = BuilderIdToken::load(&database, None)
            .await
            .log_on_err("Os::new: BuilderIdToken::load failed")?;
        let telemetry_enabled = telemetry_enabled(&database);
        let authenticated = token.is_some()
            || crate::auth::social::is_social_logged_in(&database).await
            || crate::auth::external_idp::is_external_idp_logged_in(&database).await
            || crate::util::env_var::get_api_key().is_some()
            || (env.get("KIRO_TEST_MODE").is_ok() && database.get_telemetry_user_id().ok().flatten().is_some());
        if authenticated {
            refresh_telemetry_user_id(&client, &database, telemetry_enabled).await;
        } else {
            let _ = database.clear_telemetry_user_id();
        }
        let telemetry = TelemetryThread::new(&env, &fs, &mut database, Some(&region), telemetry_enabled)
            .await
            .log_on_err("Os::new: TelemetryThread::new failed")?;
        Rollout::init(
            database.get_client_id().ok().flatten(),
            crate::rollout::resolve_segment_start_url(
                token.as_ref().and_then(|t| t.start_url.clone()),
                database.get_start_url().ok().flatten(),
            ),
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

    pub fn path_resolver(&self) -> crate::util::paths::PathResolver<'_> {
        crate::util::paths::PathResolver::new(&self.env, &self.fs)
    }

    /// This method is for "refreshing" or re-initializing resources (ApiClient and TelemetryThread)
    /// that can be initialized before the application is aware of the region that these resources
    /// should be configured with, namely before login occurs.
    /// Ideally these resources should be refactored out of the Os struct
    pub async fn set_auth_profile(&mut self, profile: &AuthProfile) -> Result<()> {
        self.database.set_auth_profile(profile)?;
        self.rebuild_after_auth_transition(None, true).await
    }

    pub(crate) async fn refresh_telemetry_identity(&mut self) -> Result<()> {
        self.rebuild_after_auth_transition(None, true).await
    }

    pub(crate) async fn reset_telemetry_after_logout(&mut self, region: Option<&str>) -> Result<()> {
        let clear_result = self.database.clear_telemetry_user_id();
        let rebuild_result = self.rebuild_after_auth_transition(region, false).await;
        clear_result?;
        rebuild_result
    }

    pub(crate) fn telemetry_region(&self) -> Option<String> {
        Some(self.client.region().to_string())
    }

    async fn rebuild_after_auth_transition(
        &mut self,
        region_override: Option<&str>,
        refresh_identity: bool,
    ) -> Result<()> {
        if refresh_identity {
            self.database.clear_telemetry_user_id()?;
        }
        let client_result = self.rebuild_api_client().await;
        let region = region_override.map_or_else(|| self.client.region().to_string(), str::to_owned);
        let telemetry_enabled = telemetry_enabled(&self.database);
        if refresh_identity && client_result.is_ok() {
            refresh_telemetry_user_id(&self.client, &self.database, telemetry_enabled).await;
        }
        let telemetry_result = self.rebuild_telemetry(Some(&region), telemetry_enabled).await;
        client_result?;
        telemetry_result
    }

    async fn rebuild_api_client(&mut self) -> Result<()> {
        self.client
            .refresh_auth_profile(&self.env, &self.fs, &mut self.database)
            .await?;
        Ok(())
    }

    async fn rebuild_telemetry(&mut self, region: Option<&str>, telemetry_enabled: bool) -> Result<()> {
        let telemetry =
            TelemetryThread::new(&self.env, &self.fs, &mut self.database, region, telemetry_enabled).await?;
        let old_telemetry = std::mem::replace(&mut self.telemetry, telemetry);
        old_telemetry.finish().await?;
        Ok(())
    }
}

async fn refresh_telemetry_user_id(client: &ApiClient, database: &Database, telemetry_enabled: bool) {
    let cached_user_id = database.get_telemetry_user_id().ok().flatten();
    if !should_refresh_telemetry_user_id(telemetry_enabled, cached_user_id.as_deref()) {
        return;
    }

    if let Ok(Ok(output)) = tokio::time::timeout(Duration::from_secs(5), client.get_usage_limits()).await
        && let Some(info) = output.user_info()
    {
        let _ = database.set_telemetry_user_id(info.user_id());
    }
}

fn should_refresh_telemetry_user_id(telemetry_enabled: bool, cached_user_id: Option<&str>) -> bool {
    telemetry_enabled && cached_user_id.is_none_or(|user_id| user_id.trim().is_empty())
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

        os.set_auth_profile(&profile).await.unwrap();
        assert_eq!(os.database.get_auth_profile().unwrap().unwrap(), profile);
        assert_eq!(os.client.get_profile().unwrap(), profile);
        assert_eq!(os.client.region(), "us-gov-east-1");
    }

    #[test]
    fn telemetry_identity_refresh_requires_enabled_cold_cache() {
        assert!(should_refresh_telemetry_user_id(true, None));
        assert!(!should_refresh_telemetry_user_id(false, None));
        assert!(!should_refresh_telemetry_user_id(true, Some("cached")));
        assert!(should_refresh_telemetry_user_id(true, Some("  ")));
    }

    async fn enable_test_telemetry(os: &mut Os) {
        unsafe {
            os.env.set_var("KIRO_TELEMETRY_OTEL", "0");
        }
        let telemetry = TelemetryThread::new(&os.env, &os.fs, &mut os.database, None, true)
            .await
            .unwrap();
        let old_telemetry = std::mem::replace(&mut os.telemetry, telemetry);
        old_telemetry.finish().await.unwrap();
        assert!(os.telemetry.is_enabled());
    }

    #[tokio::test]
    async fn auth_transition_drops_stale_identity_when_refresh_is_unavailable() {
        let mut os = Os::new().await.unwrap();
        os.database.set_telemetry_user_id("stale-user-id").unwrap();
        enable_test_telemetry(&mut os).await;

        os.refresh_telemetry_identity().await.unwrap();

        assert_eq!(os.database.get_telemetry_user_id().unwrap(), None);
        assert!(!os.telemetry.is_enabled());
    }

    #[tokio::test]
    async fn logout_reset_clears_identity_and_rebuilds_telemetry() {
        let mut os = Os::new().await.unwrap();
        os.database.set_telemetry_user_id("user-id").unwrap();
        enable_test_telemetry(&mut os).await;

        os.reset_telemetry_after_logout(None).await.unwrap();

        assert_eq!(os.database.get_telemetry_user_id().unwrap(), None);
        assert!(!os.telemetry.is_enabled());
    }
}
