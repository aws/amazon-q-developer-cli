use accessibility_sys::AXIsProcessTrusted;
use objc2::ClassType;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{
    NSURL,
    ns_string,
};

static ACCESSIBILITY_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

pub fn open_accessibility() -> bool {
    let string = ns_string!(ACCESSIBILITY_SETTINGS_URL);
    let url = unsafe { NSURL::initWithString(NSURL::alloc(), string) };
    if let Some(url) = url {
        let workspace = unsafe { NSWorkspace::sharedWorkspace() };
        unsafe { workspace.openURL(&url) }
    } else {
        false
    }
}

pub fn accessibility_is_enabled() -> bool {
    unsafe { AXIsProcessTrusted() }
}
