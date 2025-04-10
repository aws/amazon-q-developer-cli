use std::io;
use std::path::Path;
use std::sync::OnceLock;

use fig_os_shim::EnvProvider;
use regex::Regex;
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    Error,
    UnknownDesktopErrContext,
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

pub fn get_display_server(env: &impl EnvProvider) -> Result<DisplayServer, Error> {
    match env.env().get("XDG_SESSION_TYPE") {
        Ok(session) => match session.as_str() {
            "x11" => Ok(DisplayServer::X11),
            "wayland" => Ok(DisplayServer::Wayland),
            _ => Err(Error::UnknownDisplayServer(session)),
        },
        // x11 is not guarantee this var is set, so we just assume x11 if it is not set
        _ => Ok(DisplayServer::X11),
    }
}

pub fn get_desktop_environment(env: &impl EnvProvider) -> Result<DesktopEnvironment, Error> {
    let env = env.env();

    // Prioritize XDG_CURRENT_DESKTOP and check other common env vars as fallback.
    // https://superuser.com/a/1643180
    let xdg_current_desktop = match env.get("XDG_CURRENT_DESKTOP") {
        Ok(current) => {
            let current_lower = current.to_lowercase();
            let (_, desktop) = current_lower.split_once(':').unwrap_or(("", current_lower.as_str()));
            match desktop.to_lowercase().as_str() {
                "gnome" | "gnome-xorg" | "ubuntu" | "pop" => return Ok(DesktopEnvironment::Gnome),
                "kde" | "plasma" => return Ok(DesktopEnvironment::Plasma),
                "i3" => return Ok(DesktopEnvironment::I3),
                "sway" => return Ok(DesktopEnvironment::Sway),
                _ => current,
            }
        },
        _ => "".into(),
    };

    let xdg_session_desktop = match env.get("XDG_SESSION_DESKTOP") {
        Ok(session) => {
            let session_lower = session.to_lowercase();
            match session_lower.as_str() {
                "gnome" | "ubuntu" => return Ok(DesktopEnvironment::Gnome),
                "kde" => return Ok(DesktopEnvironment::Plasma),
                _ => session,
            }
        },
        _ => "".into(),
    };

    let gdm_session = match env.get("GDMSESSION") {
        Ok(session) if session.to_lowercase().starts_with("ubuntu") => return Ok(DesktopEnvironment::Gnome),
        Ok(session) => session,
        _ => "".into(),
    };

    Err(Error::UnknownDesktop(UnknownDesktopErrContext {
        xdg_current_desktop,
        xdg_session_desktop,
        gdm_session,
    }))
}

pub fn get_os_release() -> Option<&'static OsRelease> {
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

fn containerenv_engine_re() -> &'static Regex {
    static CONTAINERENV_ENGINE_RE: OnceLock<Regex> = OnceLock::new();
    CONTAINERENV_ENGINE_RE.get_or_init(|| Regex::new(r#"engine="([^"\s]+)""#).unwrap())
}

pub enum SandboxKind {
    None,
    Flatpak,
    Snap,
    Docker,
    Container(Option<String>),
}

pub fn detect_sandbox() -> SandboxKind {
    if Path::new("/.flatpak-info").exists() {
        return SandboxKind::Flatpak;
    }
    if std::env::var("SNAP").is_ok() {
        return SandboxKind::Snap;
    }
    if Path::new("/.dockerenv").exists() {
        return SandboxKind::Docker;
    }
    if let Ok(env) = std::fs::read_to_string("/var/run/.containerenv") {
        return SandboxKind::Container(
            containerenv_engine_re()
                .captures(&env)
                .and_then(|x| x.get(1))
                .map(|x| x.as_str().to_string()),
        );
    }

    SandboxKind::None
}

impl SandboxKind {
    pub fn is_container(&self) -> bool {
        matches!(self, SandboxKind::Docker | SandboxKind::Container(_))
    }

    pub fn is_app_runtime(&self) -> bool {
        matches!(self, SandboxKind::Flatpak | SandboxKind::Snap)
    }

    pub fn is_none(&self) -> bool {
        matches!(self, SandboxKind::None)
    }
}

#[cfg(test)]
mod test {
    use fig_os_shim::Env;

    use super::*;

    #[cfg(target_os = "linux")]
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

    #[test]
    fn test_get_desktop_environment() {
        let tests = [
            (vec![("XDG_CURRENT_DESKTOP", "UBUNTU:gnome")], DesktopEnvironment::Gnome),
            (
                vec![("XDG_CURRENT_DESKTOP", "Unity"), ("XDG_SESSION_DESKTOP", "ubuntu")],
                DesktopEnvironment::Gnome,
            ),
            (
                vec![("XDG_CURRENT_DESKTOP", "Unity"), ("XDG_SESSION_DESKTOP", "GNOME")],
                DesktopEnvironment::Gnome,
            ),
            (vec![("GDMSESSION", "ubuntu")], DesktopEnvironment::Gnome),
        ];

        for (env, expected_desktop_env) in tests {
            let env = Env::from_slice(&env);
            assert_eq!(
                get_desktop_environment(&env).unwrap(),
                expected_desktop_env,
                "expected: {:?} from env: {:?}",
                expected_desktop_env,
                env
            );
        }
    }

    #[test]
    fn test_get_desktop_environment_err() {
        let env = Env::from_slice(&[("XDG_CURRENT_DESKTOP", "Unity"), ("XDG_SESSION_DESKTOP", "")]);
        let res = get_desktop_environment(&env);
        println!("{}", res.as_ref().unwrap_err());
        assert!(matches!(res, Err(Error::UnknownDesktop(_))));
    }
}
