use core_foundation::base::{
    CFGetTypeID,
    CFRelease,
    TCFType,
};
use core_foundation::string::{
    CFString,
    CFStringGetTypeID,
    CFStringRef,
};
use core_foundation_sys::preferences::{
    CFPreferencesAppValueIsForced,
    CFPreferencesCopyAppValue,
};

use crate::ManagedValue;

/// The CFPreferences application ID (domain) used for all managed keys.
///
/// `dev.kiro.cli` is the reverse-DNS form of the product domain (cli.kiro.dev), following
/// Apple's preference-domain convention. It is a product-wide domain rather than any
/// single binary's bundle ID: a shared domain lets one MDM profile cover every Kiro
/// binary that reads managed configuration, bundled or not.
///
/// This is not configured by the admin — it's a fixed identifier baked into the binary
/// that tells CoreFoundation which plist domain to read from. The admin's MDM profile
/// targets this same domain string when delivering managed preferences.
const DOMAIN: &str = "dev.kiro.cli";

/// Read a string value from macOS managed preferences.
///
/// Uses CFPreferencesCopyAppValue to get the value and CFPreferencesAppValueIsForced
/// to determine if it was set by MDM (root-owned managed preferences) vs user prefs.
pub(crate) fn read_managed_string(key: &str) -> Option<ManagedValue> {
    let cf_key = CFString::new(key);
    let cf_domain = CFString::new(DOMAIN);

    let key_ref: CFStringRef = cf_key.as_concrete_TypeRef();
    let domain_ref: CFStringRef = cf_domain.as_concrete_TypeRef();

    // SAFETY: CFPreferencesAppValueIsForced is a pure read with no side effects.
    // key_ref and domain_ref are valid CFStringRefs borrowed from owned CFString values
    // that outlive this call.
    let forced = unsafe { CFPreferencesAppValueIsForced(key_ref, domain_ref) != 0 };

    // SAFETY: CFPreferencesCopyAppValue returns a retained CFPropertyListRef (or null).
    // We own the returned reference and must release it (handled via wrap_under_create_rule
    // or explicit CFRelease below).
    let value_ref = unsafe { CFPreferencesCopyAppValue(key_ref, domain_ref) };

    if value_ref.is_null() {
        return None;
    }

    // SAFETY: value_ref is non-null and retained; CFGetTypeID reads its type tag.
    let type_id = unsafe { CFGetTypeID(value_ref) };
    // SAFETY: CFStringGetTypeID is a constant lookup with no preconditions.
    if type_id != unsafe { CFStringGetTypeID() } {
        // SAFETY: value_ref is a retained CF object we own; releasing balances the Create rule.
        unsafe { CFRelease(value_ref) };
        return None;
    }

    // SAFETY: We verified the type is CFString above. wrap_under_create_rule takes ownership
    // of the retained reference, so CFRelease will be called when cf_string is dropped.
    let cf_string: CFString = unsafe { TCFType::wrap_under_create_rule(value_ref as CFStringRef) };
    let value = cf_string.to_string();

    if value.is_empty() {
        return None;
    }

    Some(ManagedValue { value, forced })
}
