pub(crate) mod download;
#[cfg(target_os = "freebsd")]
mod freebsd;
pub mod index;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(windows)]
mod windows;

use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTimeError;

use fig_os_shim::{
    Context,
    Os,
    PlatformProvider,
};
use fig_util::PRODUCT_NAME;
use fig_util::manifest::{
    Channel,
    FileType,
    Variant,
    bundle_metadata,
    manifest,
};
#[cfg(target_os = "freebsd")]
use freebsd as os;
use index::UpdatePackage;
#[cfg(target_os = "linux")]
use linux as os;
#[cfg(target_os = "macos")]
use macos as os;
#[cfg(target_os = "macos")]
pub use os::uninstall_terminal_integrations;
use thiserror::Error;
use tokio::sync::mpsc::Receiver;
use tracing::{
    debug,
    error,
    info,
};
#[cfg(windows)]
use windows as os;

mod common;
pub use common::{
    InstallComponents,
    install,
    uninstall,
};

pub const UNINSTALL_URL: &str = "https://pulse.aws/survey/QYFVDA5H";

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("unsupported platform")]
    UnsupportedPlatform,
    #[error(transparent)]
    Util(#[from] fig_util::Error),
    #[error(transparent)]
    Integration(#[from] fig_integrations::Error),
    #[error(transparent)]
    Settings(#[from] fig_settings::Error),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("error converting path")]
    PathConversionError(#[from] camino::FromPathBufError),
    #[error(transparent)]
    Semver(#[from] semver::Error),
    #[error(transparent)]
    SystemTime(#[from] SystemTimeError),
    #[error(transparent)]
    Strum(#[from] strum::ParseError),
    #[error("could not determine app version")]
    UnclearVersion,
    #[error("please update from your package manager")]
    PackageManaged,
    #[error("failed to update: `{0}`")]
    UpdateFailed(String),
    #[error("failed to update: `{0}`")]
    UpdateFailedPermissions(String),
    #[cfg(target_os = "macos")]
    #[error("failed to update due to auth error: `{0}`")]
    SecurityFramework(#[from] security_framework::base::Error),
    #[error("your system is not supported on this channel")]
    SystemNotOnChannel,
    #[error("manifest not found")]
    ManifestNotFound,
    #[error("Update in progress")]
    UpdateInProgress,
    #[error("could not convert path to cstring")]
    Nul(#[from] std::ffi::NulError),
    #[error("failed to get system id")]
    SystemIdNotFound,
    #[error("unable to find the bundled metadata")]
    BundleMetadataNotFound,
    #[error("unsupported variant: {0}")]
    UnsupportedVariant(String),
}

impl From<fig_util::directories::DirectoryError> for Error {
    fn from(err: fig_util::directories::DirectoryError) -> Self {
        fig_util::Error::Directory(err).into()
    }
}

// The current selected channel
pub fn get_channel() -> Result<Channel, Error> {
    Ok(match fig_settings::state::get_string("updates.channel")? {
        Some(channel) => Channel::from_str(&channel)?,
        None => {
            let manifest_channel = manifest().default_channel;
            if fig_settings::settings::get_bool_or("app.beta", false) {
                manifest_channel.max(Channel::Beta)
            } else {
                manifest_channel
            }
        },
    })
}

/// The highest channel to display to user
pub fn get_max_channel() -> Channel {
    let state_channel = fig_settings::state::get_string("updates.channel")
        .ok()
        .flatten()
        .and_then(|s| Channel::from_str(&s).ok())
        .unwrap_or(Channel::Stable);
    let manifest_channel = manifest().default_channel;
    let settings_channel = if fig_settings::settings::get_bool_or("app.beta", false) {
        Channel::Beta
    } else {
        Channel::Stable
    };

    [state_channel, manifest_channel, settings_channel]
        .into_iter()
        .max()
        .unwrap()
}

pub async fn check_for_updates(ignore_rollout: bool) -> Result<Option<UpdatePackage>, Error> {
    let manifest = manifest();
    let ctx = Context::new();
    let file_type = match (&manifest.variant, ctx.platform().os()) {
        (Variant::Full, fig_os_shim::Os::Linux) => match index::get_file_type(&ctx, &manifest.variant).await {
            Ok(file_type) => Some(file_type),
            _ => None,
        },
        _ => Some(index::get_file_type(&Context::new(), &manifest.variant).await?),
    };
    index::check_for_updates(
        get_channel()?,
        &manifest.target_triple,
        &manifest.variant,
        file_type.as_ref(),
        ignore_rollout,
    )
    .await
}

#[derive(Debug, Clone)]
pub enum UpdateStatus {
    Percent(f32),
    Message(String),
    Error(String),
    Exit,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateOptions {
    /// Ignores the rollout and forces an update if any newer version is available
    pub ignore_rollout: bool,
    /// If the update is interactive and the user will be able to respond to prompts
    pub interactive: bool,
    /// If to relaunch into dashboard after update (false will launch in background)
    pub relaunch_dashboard: bool,
}

/// Attempt to update if there is a newer version of Fig
pub async fn update(
    ctx: Arc<Context>,
    on_update: Option<Box<dyn FnOnce(Receiver<UpdateStatus>) + Send>>,
    UpdateOptions {
        ignore_rollout,
        interactive,
        relaunch_dashboard,
    }: UpdateOptions,
) -> Result<bool, Error> {
    info!("Checking for updates...");
    if let Some(update) = check_for_updates(ignore_rollout).await? {
        info!("Found update: {}", update.version);
        debug!("Update info: {:?}", update);

        if ctx.platform().os() == Os::Linux
            && manifest().variant == Variant::Full
            && bundle_metadata(&ctx)
                .await?
                .is_some_and(|md| md.packaged_as != FileType::AppImage)
        {
            return Err(Error::UpdateFailed(format!(
                "Please use your package manager to update {}",
                PRODUCT_NAME
            )));
        }

        let (tx, rx) = tokio::sync::mpsc::channel(16);

        let lock_file = fig_util::directories::update_lock_path(&ctx)?;

        let now_unix_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // If the lock file is older than 1hr, we can assume it's stale and remove it
        if lock_file.exists() {
            match std::fs::read_to_string(&lock_file) {
                Ok(contents) => {
                    let lock_unix_time = contents.parse::<u64>().unwrap_or(0);
                    if now_unix_time - lock_unix_time < 3600 {
                        return Err(Error::UpdateInProgress);
                    } else {
                        std::fs::remove_file(&lock_file)?;
                    }
                },
                Err(err) => {
                    error!(%err, "Failed to read lock file, but it exists");
                },
            }
        }

        tokio::fs::write(&lock_file, &format!("{now_unix_time}")).await?;

        let join = tokio::spawn(async move {
            tx.send(UpdateStatus::Message("Starting Update...".into())).await.ok();
            if let Err(err) = os::update(update, tx.clone(), interactive, relaunch_dashboard).await {
                error!(%err, "Failed to update");

                if let Err(err) = tokio::fs::remove_file(&lock_file).await {
                    error!(%err, "Failed to remove lock file");
                }

                tx.send(UpdateStatus::Error(format!("{err}"))).await.ok();
                return Err(err);
            }
            tokio::fs::remove_file(&lock_file).await?;
            Ok(())
        });

        if let Some(on_update) = on_update {
            info!("Updating...");
            on_update(rx);
        } else {
            drop(rx);
        }

        join.await.expect("Failed to join update thread")?;
        Ok(true)
    } else {
        info!("No updates available");
        Ok(false)
    }
}
