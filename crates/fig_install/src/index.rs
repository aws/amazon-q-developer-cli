use std::collections::hash_map::DefaultHasher;
use std::hash::{
    Hash,
    Hasher,
};
use std::sync::LazyLock;
use std::time::{
    SystemTime,
    UNIX_EPOCH,
};

use cfg_if::cfg_if;
use fig_os_shim::Context;
use fig_util::manifest::{
    Channel,
    FileType,
    Os,
    TargetTriple,
    Variant,
    bundle_metadata,
};
use fig_util::system_info::get_system_id;
use semver::Version;
use serde::{
    Deserialize,
    Serialize,
};
use strum::{
    Display,
    EnumString,
};
use tracing::{
    error,
    info,
    trace,
};
use url::Url;

use crate::Error;

const DEFAULT_RELEASE_URL: &str = "https://desktop-release.q.us-east-1.amazonaws.com";

/// The url to check for updates from, tries the following order:
/// - The env var `Q_DESKTOP_RELEASE_URL`
/// - The setting `install.releaseUrl`
/// - Falls back to the default or the build time env var `Q_BUILD_DESKTOP_RELEASE_URL`
static RELEASE_URL: LazyLock<Url> = LazyLock::new(|| {
    match std::env::var("Q_DESKTOP_RELEASE_URL") {
        Ok(s) => Url::parse(&s),
        Err(_) => match fig_settings::settings::get_string("install.releaseUrl") {
            Ok(Some(s)) => Url::parse(&s),
            _ => Url::parse(option_env!("Q_BUILD_DESKTOP_RELEASE_URL").unwrap_or(DEFAULT_RELEASE_URL)),
        },
    }
    .unwrap()
});

fn deser_enum_other<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match T::from_str(<&str as Deserialize<'de>>::deserialize(deserializer)?) {
        Ok(s) => Ok(s),
        Err(err) => Err(serde::de::Error::custom(err)),
    }
}

fn deser_opt_enum_other<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match Option::<&'de str>::deserialize(deserializer)? {
        Some(s) => match T::from_str(s) {
            Ok(s) => Ok(Some(s)),
            Err(err) => Err(serde::de::Error::custom(err)),
        },
        None => Ok(None),
    }
}

#[allow(unused)]
#[derive(Deserialize, Serialize, Debug)]
pub struct Index {
    supported: Vec<Support>,
    versions: Vec<RemoteVersion>,
}

impl Index {
    #[allow(dead_code)]
    pub(crate) fn latest(&self) -> Option<&RemoteVersion> {
        self.versions.iter().max_by(|a, b| a.version.cmp(&b.version))
    }

    /// Determines the next package in the index to update to, given the provided parameters.
    ///
    /// If `file_type` is [Option::None], then the returned package *may have a different file type
    /// than the currently installed version*. This is useful to check if an update exists for the
    /// given target and variant without filtering on file type, e.g. in the case of Linux desktop
    /// bundles.
    pub fn find_next_version(
        &self,
        target_triple: &TargetTriple,
        variant: &Variant,
        file_type: Option<&FileType>,
        current_version: &str,
        ignore_rollout: bool,
        threshold_override: Option<u8>,
    ) -> Result<Option<UpdatePackage>, Error> {
        if !self.supported.iter().any(|support| {
            support.target_triple.as_ref() == Some(target_triple)
                && support.variant == *variant
                && (file_type.is_none()
                    || file_type.is_some_and(|file_type| support.file_type.as_ref() == Some(file_type)))
        }) {
            error!("No support found for: {} {} {:?}", target_triple, variant, file_type);
            return Err(Error::SystemNotOnChannel);
        }

        let right_now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        let mut valid_versions = self
            .versions
            .iter()
            .filter(|version| {
                version.packages.iter().any(|package| {
                    package.target_triple.as_ref() == Some(target_triple)
                        && package.variant == *variant
                        && (file_type.is_none()
                            || file_type.is_some_and(|file_type| package.file_type.as_ref() == Some(file_type)))
                })
            })
            .filter(|version| match &version.rollout {
                Some(rollout) => rollout.start <= right_now,
                None => true,
            })
            .collect::<Vec<&RemoteVersion>>();

        valid_versions.sort_unstable_by(|lhs, rhs| lhs.version.cmp(&rhs.version));
        valid_versions.reverse();

        let Some(sys_id) = get_system_id() else {
            return Err(Error::SystemIdNotFound);
        };
        let system_threshold = threshold_override.unwrap_or_else(|| {
            let mut hasher = DefaultHasher::new();
            // different for each system
            sys_id.hash(&mut hasher);
            // different for each version, which prevents people from getting repeatedly hit by untested
            // releases
            current_version.hash(&mut hasher);

            (hasher.finish() % 0xff) as u8
        });

        let chosen = valid_versions.into_iter().next().filter(|entry| {
            if let Some(rollout) = &entry.rollout {
                if ignore_rollout {
                    trace!("accepted update candidate {} because rollout is ignored", entry.version);
                    return true;
                }
                if rollout.end < right_now {
                    trace!("accepted update candidate {} because rollout is over", entry.version);
                    return true;
                }
                if rollout.start > right_now {
                    trace!(
                        "rejected update candidate {} because rollout hasn't started yet",
                        entry.version
                    );
                    return false;
                }

                // interpolate rollout progress
                let offset_into = (right_now - rollout.start) as f64;
                let rollout_length = (rollout.end - rollout.start) as f64;
                let progress = offset_into / rollout_length;
                let remote_threshold = (progress * 256.0).round().clamp(0.0, 256.0) as u8;

                if remote_threshold >= system_threshold {
                    // the rollout chose us
                    info!(
                        "accepted update candidate {} with remote_threshold {remote_threshold} and system_threshold {system_threshold}",
                        entry.version
                    );
                    true
                } else {
                    info!(
                        "rejected update candidate {} because remote_threshold {remote_threshold} is below system_threshold {system_threshold}",
                        entry.version
                    );
                    false
                }
            } else {
                true
            }
        });

        if chosen.is_none() {
            // no upgrade candidates
            return Ok(None);
        }

        let chosen = chosen.unwrap();
        let package = chosen
            .packages
            .iter()
            .find(|package| {
                package.target_triple.as_ref() == Some(target_triple)
                    && package.variant == *variant
                    && (file_type.is_none()
                        || file_type.is_some_and(|file_type| package.file_type.as_ref() == Some(file_type)))
            })
            .unwrap();

        if match Version::parse(current_version) {
            Ok(current_version) => chosen.version <= current_version,
            Err(err) => {
                error!("failed parsing current version semver: {err:?}");
                chosen.version.to_string() == current_version
            },
        } {
            return Ok(None);
        }

        Ok(Some(UpdatePackage {
            version: chosen.version.clone(),
            download_url: package.download_url(),
            sha256: package.sha256.clone(),
            size: package.size,
            cli_path: package.cli_path.clone(),
        }))
    }
}

#[allow(unused)]
#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Support {
    #[serde(deserialize_with = "deser_enum_other")]
    architecture: PackageArchitecture,
    #[serde(deserialize_with = "deser_enum_other")]
    variant: Variant,
    #[serde(deserialize_with = "deser_opt_enum_other", default)]
    target_triple: Option<TargetTriple>,
    #[serde(deserialize_with = "deser_opt_enum_other", default)]
    os: Option<Os>,
    #[serde(deserialize_with = "deser_opt_enum_other", default)]
    file_type: Option<FileType>,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct RemoteVersion {
    pub version: Version,
    pub rollout: Option<Rollout>,
    pub packages: Vec<Package>,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Rollout {
    start: u64,
    end: u64,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    #[serde(deserialize_with = "deser_enum_other")]
    pub(crate) architecture: PackageArchitecture,
    #[serde(deserialize_with = "deser_enum_other")]
    pub(crate) variant: Variant,
    #[serde(deserialize_with = "deser_opt_enum_other", default)]
    pub(crate) target_triple: Option<TargetTriple>,
    #[serde(deserialize_with = "deser_opt_enum_other", default)]
    pub(crate) os: Option<Os>,
    #[serde(deserialize_with = "deser_opt_enum_other", default)]
    pub(crate) file_type: Option<FileType>,
    pub(crate) download: String,
    pub(crate) sha256: String,
    pub(crate) size: u64,
    pub(crate) cli_path: Option<String>,
}

impl Package {
    pub(crate) fn download_url(&self) -> Url {
        let mut url = RELEASE_URL.clone();
        url.set_path(&self.download);
        url
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct UpdatePackage {
    /// The version of the package
    pub version: Version,
    /// The url to download the archive from
    pub download_url: Url,
    /// The sha256 sum of the archive
    pub sha256: String,
    /// Size of the package in bytes
    pub size: u64,
    /// Path to the CLI in the bundle
    pub cli_path: Option<String>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, EnumString, Debug, Display)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum PackageArchitecture {
    #[serde(rename = "x86_64")]
    #[strum(serialize = "x86_64")]
    X86_64,
    #[serde(rename = "aarch64")]
    #[strum(serialize = "aarch64")]
    AArch64,
    Universal,
    #[strum(default)]
    Other(String),
}

impl PackageArchitecture {
    #[allow(dead_code)]
    const fn from_system() -> Self {
        cfg_if! {
            if #[cfg(target_os = "macos")] {
                PackageArchitecture::Universal
            } else if #[cfg(target_arch = "x86_64")] {
                PackageArchitecture::X86_64
            } else if #[cfg(target_arch = "aarch64")] {
                PackageArchitecture::AArch64
            } else {
                compile_error!("unknown architecture")
            }
        }
    }
}

fn index_endpoint(_channel: &Channel) -> Url {
    let mut url = RELEASE_URL.clone();
    url.set_path("index.json");
    url
}

pub async fn pull(channel: &Channel) -> Result<Index, Error> {
    let response = fig_request::client()
        .expect("Unable to create HTTP client")
        .get(index_endpoint(channel))
        .send()
        .await?;
    let index = response.json().await?;
    Ok(index)
}

pub async fn check_for_updates(
    channel: Channel,
    target_triple: &TargetTriple,
    variant: &Variant,
    file_type: Option<&FileType>,
    ignore_rollout: bool,
) -> Result<Option<UpdatePackage>, Error> {
    const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
    pull(&channel)
        .await?
        .find_next_version(target_triple, variant, file_type, CURRENT_VERSION, ignore_rollout, None)
}

pub(crate) async fn get_file_type(ctx: &Context, variant: &Variant) -> Result<FileType, Error> {
    match ctx.platform().os() {
        fig_os_shim::Os::Mac => Ok(FileType::Dmg),
        fig_os_shim::Os::Linux => match variant {
            Variant::Full => Ok(bundle_metadata(ctx)
                .await?
                .ok_or(Error::BundleMetadataNotFound)?
                .packaged_as),
            Variant::Minimal => Ok(FileType::TarZst),
            Variant::Other(_) => Err(Error::UnsupportedPlatform),
        },
        _ => Err(Error::UnsupportedPlatform),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use fig_util::{
        OLD_CLI_BINARY_NAMES,
        OLD_PRODUCT_NAME,
    };

    use super::*;

    macro_rules! test_ser_deser {
        ($ty:ident, $variant:expr, $text:expr) => {
            let quoted = format!("\"{}\"", $text);
            assert_eq!(quoted, serde_json::to_string(&$variant).unwrap());
            assert_eq!($variant, serde_json::from_str(&quoted).unwrap());
            assert_eq!($variant, $ty::from_str($text).unwrap());
            assert_eq!($text, $variant.to_string());
        };
    }

    #[test]
    fn test_package_architecture_serialize_deserialize() {
        test_ser_deser!(PackageArchitecture, PackageArchitecture::X86_64, "x86_64");
        test_ser_deser!(PackageArchitecture, PackageArchitecture::AArch64, "aarch64");
        test_ser_deser!(PackageArchitecture, PackageArchitecture::Universal, "universal");
    }

    #[tokio::test]
    #[cfg(target_os = "macos")]
    async fn pull_test() {
        let index = pull(&Channel::Stable).await.unwrap();
        println!("{:#?}", index);
        assert!(!index.supported.is_empty());
        assert!(!index.versions.is_empty());
    }

    #[tokio::test]
    #[cfg(target_os = "macos")]
    #[ignore = "New index format not used yet"]
    async fn check_test() {
        check_for_updates(
            Channel::Stable,
            &TargetTriple::UniversalAppleDarwin,
            &Variant::Full,
            Some(FileType::Dmg).as_ref(),
            false,
        )
        .await
        .unwrap();
    }

    #[test]
    fn test_release_url() {
        println!("{}", *RELEASE_URL);
        println!("{:#?}", *RELEASE_URL);
    }

    #[test]
    fn index_serde_test() {
        let old_cli_name = OLD_CLI_BINARY_NAMES[0];
        let json_str = serde_json::json!({
            "supported": [
                {
                    "kind": "dmg",
                    "targetTriple": "universal-apple-darwin",
                    "os": "macos",
                    "architecture": "universal",
                    "variant": "full",
                    "fileType": "dmg"
                },
                {
                    "kind": "deb",
                    "targetTriple": "x86_64-unknown-linux-gnu",
                    "os": "linux",
                    "architecture": "x86_64",
                    "variant": "headless",
                    "fileType": "tarZst"
                }
            ],
            "versions": [
                {
                    "version": "0.7.0",
                    "rollout": null,
                    "packages": [
                        {
                            "kind": "dmg",
                            "architecture": "universal",
                            "variant": "full",
                            "download": format!("0.7.0/{old_cli_name}.dmg"),
                            "sha256": "4213d7649e4b1a2ec50adc0266d32d3e1e1f952ed6a863c28d7538190dc92472",
                            "size": 82975504
                        }
                    ]
                },
                {
                    "version": "0.15.3",
                    "packages": [
                        {
                            "kind": "dmg",
                            "architecture": "universal",
                            "variant": "full",
                            "download": format!("0.15.3/{OLD_PRODUCT_NAME}.dmg"),
                            "sha256": "87a311e493bb2b0e68a1b4b5d267c79628d23c1e39b0a62d1a80b0c2352f80a2",
                            "size": 88174538,
                            "cliPath": format!("Contents/MacOS/{old_cli_name}")
                        }
                    ]
                },
                {
                    "version": "1.0.0",
                    "packages": [
                        {
                            "kind": "deb",
                            "fileType": "dmg",
                            "os": "macos",
                            "architecture": "universal",
                            "variant": "full",
                            "download": "1.0.0/Q.dmg",
                            "sha256": "87a311e493bb2b0e68a1b4b5d267c79628d23c1e39b0a62d1a80b0c2352f80a2",
                            "size": 88174538,
                            "cliPath": format!("Contents/MacOS/{old_cli_name}"),
                        },
                        {
                            "kind": "deb",
                            "fileType": "tarZst",
                            "os": "linux",
                            "architecture": "x86_64",
                            "variant": "headless",
                            "download": "1.0.0/q-x86_64-linux.tar.zst",
                            "sha256": "5a6abea56bfa91bd58d49fe40322058d0efea825f7e19f7fb7db1c204ae625b6",
                            "size": 76836772,
                        }
                    ]
                },
                {
                    "version": "2.0.0",
                    "packages": [
                        {
                            // random values to ensure forward compat
                            "kind": "abc",
                            "fileType": "abc",
                            "os": "abc",
                            "architecture": "abc",
                            "variant": "abc",
                            "download": "abc",
                            "sha256": "abc",
                            "size": 123,
                            "cliPath": "abc",
                            "otherField": "abc"
                        }
                    ]
                }
            ]
        })
        .to_string();

        let index = serde_json::from_str::<Index>(&json_str).unwrap();
        println!("{:#?}", index);

        assert_eq!(index.supported.len(), 2);
        assert_eq!(index.supported[0], Support {
            architecture: PackageArchitecture::Universal,
            target_triple: Some(TargetTriple::UniversalAppleDarwin),
            variant: Variant::Full,
            os: Some(Os::Macos),
            file_type: Some(FileType::Dmg),
        });
        assert_eq!(index.supported[1], Support {
            architecture: PackageArchitecture::X86_64,
            target_triple: Some(TargetTriple::X86_64UnknownLinuxGnu),
            variant: Variant::Minimal,
            os: Some(Os::Linux),
            file_type: Some(FileType::TarZst),
        });

        assert_eq!(index.versions.len(), 4);

        // check the 1.0.0 entry matches
        assert_eq!(index.versions[2], RemoteVersion {
            version: Version::new(1, 0, 0),
            rollout: None,
            packages: vec![
                Package {
                    architecture: PackageArchitecture::Universal,
                    variant: Variant::Full,
                    os: Some(Os::Macos),
                    target_triple: None,
                    file_type: Some(FileType::Dmg),
                    download: "1.0.0/Q.dmg".into(),
                    sha256: "87a311e493bb2b0e68a1b4b5d267c79628d23c1e39b0a62d1a80b0c2352f80a2".into(),
                    size: 88174538,
                    cli_path: Some(format!("Contents/MacOS/{old_cli_name}")),
                },
                Package {
                    architecture: PackageArchitecture::X86_64,
                    variant: Variant::Minimal,
                    os: Some(Os::Linux),
                    target_triple: None,
                    file_type: Some(FileType::TarZst),
                    download: "1.0.0/q-x86_64-linux.tar.zst".into(),
                    sha256: "5a6abea56bfa91bd58d49fe40322058d0efea825f7e19f7fb7db1c204ae625b6".into(),
                    size: 76836772,
                    cli_path: None,
                }
            ],
        });
    }

    fn load_test_index() -> Index {
        serde_json::from_str(include_str!("../test_files/test-index.json")).unwrap()
    }

    #[test]
    fn index_latest_version_does_not_upgrade() {
        let next = load_test_index()
            .find_next_version(
                &TargetTriple::AArch64UnknownLinuxMusl,
                &Variant::Minimal,
                Some(&FileType::TarZst),
                "1.2.1",
                true,
                None,
            )
            .unwrap();
        assert!(next.is_none());
    }

    #[test]
    fn index_outdated_version_upgrades_to_correct_version() {
        let next = load_test_index()
            .find_next_version(
                &TargetTriple::AArch64UnknownLinuxMusl,
                &Variant::Minimal,
                Some(&FileType::TarZst),
                "1.2.0",
                true,
                None,
            )
            .unwrap()
            .expect("Should have UpdatePackage");
        assert_eq!(next.version.to_string(), "1.2.1".to_owned());
        assert_eq!(next.sha256, "a8112".to_owned());
    }

    #[test]
    fn index_missing_support_returns_error() {
        let next = load_test_index().find_next_version(
            &TargetTriple::AArch64UnknownLinuxMusl,
            &Variant::Full,
            Some(&FileType::TarZst),
            "1.2.1",
            true,
            None,
        );
        assert!(next.is_err());
    }

    #[test]
    fn index_with_optional_filetype_returns_highest_version() {
        let next = load_test_index()
            .find_next_version(
                &TargetTriple::X86_64UnknownLinuxGnu,
                &Variant::Full,
                None,
                "1.0.5",
                true,
                None,
            )
            .unwrap()
            .expect("should have update package");
        assert_eq!(next.version.to_string().as_str(), "1.2.1");
    }
}
