use std::io;
use std::sync::OnceLock;

use serde::{
    Deserialize,
    Serialize,
};
use winreg::RegKey;
use winreg::enums::HKEY_LOCAL_MACHINE;

use super::{
    OSVersion,
    OsRelease,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayServer {
    Win32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DesktopEnvironment {
    Windows,
    WindowsTerminal,
}

pub fn get_os_release() -> Option<&'static OsRelease> {
    static OS_RELEASE: OnceLock<Option<OsRelease>> = OnceLock::new();
    OS_RELEASE.get_or_init(|| OsRelease::load().ok()).as_ref()
}

pub fn get_os_version() -> Option<OSVersion> {
    let rkey = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .ok()?;

    let build: String = rkey.get_value("CurrentBuild").ok()?;
    let name: String = rkey.get_value("ProductName").ok()?;

    Some(OSVersion::Windows {
        name,
        build: build.parse::<u32>().ok()?,
    })
}

impl OsRelease {
    fn registry_path() -> &'static str {
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"
    }

    pub(crate) fn load() -> io::Result<OsRelease> {
        let reg_key = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(Self::registry_path())?;

        let os_release = OsRelease {
            // Map Windows registry values to OsRelease fields
            name: reg_key.get_value("ProductName").ok(),
            pretty_name: reg_key.get_value("ProductName").ok(),

            // Use ReleaseId or DisplayVersion for version_id
            version_id: reg_key
                .get_value("ReleaseId")
                .or_else(|_| reg_key.get_value("DisplayVersion"))
                .ok(),

            // Use DisplayVersion or ReleaseId for version
            version: reg_key
                .get_value("DisplayVersion")
                .or_else(|_| reg_key.get_value("ReleaseId"))
                .ok(),

            // Use CurrentBuild for build_id
            build_id: reg_key.get_value("CurrentBuild").ok(),

            // Use EditionID for variant_id
            variant: Some("Windows".to_string()),
            variant_id: reg_key.get_value("EditionID").ok(),

            // Set Windows as the ID
            id: Some("windows".to_string()),
        };

        Ok(os_release)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn os_release() {
        let info = OsRelease::load().unwrap();
        assert!(info.name.is_some());
        assert!(info.id.is_some());
        assert_eq!(info.id, Some("windows".to_string()));
    }
}
