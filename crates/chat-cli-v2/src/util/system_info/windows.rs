use serde::{
    Deserialize,
    Serialize,
};
use winreg::RegKey;
use winreg::enums::HKEY_LOCAL_MACHINE;

use super::OSVersion;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayServer {
    Win32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DesktopEnvironment {
    Windows,
    WindowsTerminal,
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
