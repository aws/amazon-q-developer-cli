#[cfg(target_os = "linux")]
use std::io;
#[cfg(target_os = "linux")]
use std::path::Path;

use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayServer {
    X11,
    Wayland,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DesktopEnvironment {
    Gnome,
    Plasma,
    I3,
    Sway,
}

#[cfg(target_os = "linux")]
pub fn get_os_release() -> Option<&'static OsRelease> {
    use std::sync::OnceLock;

    static OS_RELEASE: OnceLock<Option<OsRelease>> = OnceLock::new();
    OS_RELEASE.get_or_init(|| OsRelease::load().ok()).as_ref()
}

/// Fields from <https://www.man7.org/linux/man-pages/man5/os-release.5.html>
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsRelease {
    pub id: Option<String>,

    pub name: Option<String>,
    pub pretty_name: Option<String>,

    pub version_id: Option<String>,
    pub version: Option<String>,

    pub build_id: Option<String>,

    pub variant_id: Option<String>,
    pub variant: Option<String>,
}

#[cfg(target_os = "linux")]
impl OsRelease {
    fn path() -> &'static Path {
        Path::new("/etc/os-release")
    }

    pub(crate) fn load() -> io::Result<OsRelease> {
        let os_release_str = std::fs::read_to_string(Self::path())?;
        Ok(OsRelease::from_str(&os_release_str))
    }

    pub(crate) fn from_str(s: &str) -> OsRelease {
        // Remove the starting and ending quotes from a string if they match
        let strip_quotes = |s: &str| -> Option<String> {
            if s.starts_with('"') && s.ends_with('"') {
                Some(s[1..s.len() - 1].into())
            } else {
                Some(s.into())
            }
        };

        let mut os_release = OsRelease::default();
        for line in s.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "ID" => os_release.id = strip_quotes(value),
                    "NAME" => os_release.name = strip_quotes(value),
                    "PRETTY_NAME" => os_release.pretty_name = strip_quotes(value),
                    "VERSION" => os_release.version = strip_quotes(value),
                    "VERSION_ID" => os_release.version_id = strip_quotes(value),
                    "BUILD_ID" => os_release.build_id = strip_quotes(value),
                    "VARIANT" => os_release.variant = strip_quotes(value),
                    "VARIANT_ID" => os_release.variant_id = strip_quotes(value),
                    _ => {},
                }
            }
        }
        os_release
    }
}

#[cfg(target_os = "linux")]
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn os_release() {
        if OsRelease::path().exists() {
            OsRelease::load().unwrap();
        } else {
            println!("Skipping os-release test as /etc/os-release does not exist");
        }
    }

    #[test]
    fn os_release_parse() {
        let os_release_str = indoc::indoc! {r#"
            NAME="Amazon Linux"
            VERSION="2023"
            ID="amzn"
            ID_LIKE="fedora"
            VERSION_ID="2023"
            PLATFORM_ID="platform:al2023"
            PRETTY_NAME="Amazon Linux 2023.4.20240416"
            ANSI_COLOR="0;33"
            CPE_NAME="cpe:2.3:o:amazon:amazon_linux:2023"
            HOME_URL="https://aws.amazon.com/linux/amazon-linux-2023/"
            DOCUMENTATION_URL="https://docs.aws.amazon.com/linux/"
            SUPPORT_URL="https://aws.amazon.com/premiumsupport/"
            BUG_REPORT_URL="https://github.com/amazonlinux/amazon-linux-2023"
            VENDOR_NAME="AWS"
            VENDOR_URL="https://aws.amazon.com/"
            SUPPORT_END="2028-03-15"
        "#};

        let os_release = OsRelease::from_str(os_release_str);

        assert_eq!(os_release.id, Some("amzn".into()));

        assert_eq!(os_release.name, Some("Amazon Linux".into()));
        assert_eq!(os_release.pretty_name, Some("Amazon Linux 2023.4.20240416".into()));

        assert_eq!(os_release.version_id, Some("2023".into()));
        assert_eq!(os_release.version, Some("2023".into()));

        assert_eq!(os_release.build_id, None);

        assert_eq!(os_release.variant_id, None);
        assert_eq!(os_release.variant, None);
    }
}
