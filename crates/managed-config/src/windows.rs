use winreg::RegKey;
use winreg::enums::HKEY_LOCAL_MACHINE;

use crate::ManagedValue;

const POLICY_SUBKEY: &str = r"SOFTWARE\Policies\Kiro\CLI";

/// Read a string value from Windows policy registry (HKLM only).
///
/// Values under HKLM\SOFTWARE\Policies are writable only by administrators,
/// so their presence is inherently "forced" — a non-admin user cannot plant a fake.
pub(crate) fn read_managed_string(key: &str) -> Option<ManagedValue> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let subkey = hklm.open_subkey(POLICY_SUBKEY).ok()?;
    let value: String = subkey.get_value(key).ok()?;

    if value.is_empty() {
        return None;
    }

    // HKLM policy values are always considered forced — only admins can write them.
    Some(ManagedValue { value, forced: true })
}
