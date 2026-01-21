#![allow(dead_code)]

pub mod diagnostics;
mod env;
mod fs;
mod sysinfo;

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
use crate::telemetry::TelemetryThread;

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
        let mut database = Database::new(&env, &fs).await?;
        let client = ApiClient::new(&env, &fs, &mut database, None).await?;
        let token = BuilderIdToken::load(&database, None).await?;
        let region = token.as_ref().and_then(|t| t.region.as_deref());
        let telemetry = TelemetryThread::new(&env, &fs, &mut database, region).await?;

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

        // reconstruct api client
        self.client
            .refresh_auth_profile(&self.env, &self.fs, &mut self.database)
            .await?;

        let region = profile.arn.split(':').nth(3);

        // reconstruct telemetry thread and clients
        let old_telemetry = std::mem::replace(
            &mut self.telemetry,
            TelemetryThread::new(&self.env, &self.fs, &mut self.database, region).await?,
        );

        old_telemetry.finish().await?;
        Ok(())
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

        os.set_auth_profile(&profile).await.unwrap();
        assert_eq!(os.database.get_auth_profile().unwrap().unwrap(), profile);
        assert_eq!(os.client.get_profile().unwrap(), profile);
    }
}
