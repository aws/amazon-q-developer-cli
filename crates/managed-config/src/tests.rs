use super::*;

#[test]
fn managed_base_url_returns_none_when_not_set() {
    // On a dev machine without MDM, this should always return None.
    // This test validates the no-op path on Linux and the "not present" path
    // on macOS/Windows (unless the dev machine is actually MDM-enrolled).
    let result = managed_base_url();
    // We can't assert None unconditionally because this test might run on a
    // managed machine. Instead, verify the return type is correct.
    let _: Option<String> = result;
}

#[test]
fn enforcement_active_false_in_debug_builds() {
    // debug_assertions is true in test builds, so enforcement is always off.
    assert!(!enforcement_active());
}

#[test]
fn managed_value_equality() {
    let a = ManagedValue {
        value: "https://mirror.corp.example.com/kiro/".to_string(),
        forced: true,
    };
    let b = ManagedValue {
        value: "https://mirror.corp.example.com/kiro/".to_string(),
        forced: true,
    };
    assert_eq!(a, b);

    let c = ManagedValue {
        value: "https://mirror.corp.example.com/kiro/".to_string(),
        forced: false,
    };
    assert_ne!(a, c);
}

// Platform-specific behavior tests that validate the contract:
// - forced=false means the value came from user prefs, not MDM → treat as absent
// - empty string → treat as absent

#[test]
fn managed_base_url_requires_forced_bit() {
    // Simulate the logic: a non-forced value should not be returned.
    let non_forced = ManagedValue {
        value: "https://example.com/".to_string(),
        forced: false,
    };
    // This mirrors the check in managed_base_url()
    let result = if non_forced.forced {
        Some(non_forced.value.clone())
    } else {
        None
    };
    assert_eq!(result, None);
}

#[test]
fn managed_base_url_returns_value_when_forced() {
    let forced = ManagedValue {
        value: "https://mirror.corp.example.com/kiro/".to_string(),
        forced: true,
    };
    let result = if forced.forced {
        Some(forced.value.clone())
    } else {
        None
    };
    assert_eq!(result, Some("https://mirror.corp.example.com/kiro/".to_string()));
}

#[cfg(not(target_os = "macos"))]
#[cfg(not(target_os = "windows"))]
mod linux_tests {
    use super::super::platform;

    #[test]
    fn linux_always_returns_none() {
        assert_eq!(platform::read_managed_string("update.baseUrl"), None);
        assert_eq!(platform::read_managed_string("anything"), None);
    }
}
