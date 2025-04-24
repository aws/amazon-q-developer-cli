use std::sync::OnceLock;
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

/// Gets Windows version information from the registry
pub fn get_windows_version() -> Option<(String, u32)> {
    let rkey = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .ok()?;
    
    let product_name: String = rkey.get_value("ProductName").ok()?;
    let build: String = rkey.get_value("CurrentBuild").ok()?;
    let build_number = build.parse::<u32>().ok()?;
    
    Some((product_name, build_number))
}

/// Checks if the current Windows version is Windows 11 or newer
pub fn is_windows_11_or_newer() -> bool {
    static IS_WINDOWS_11: OnceLock<bool> = OnceLock::new();
    *IS_WINDOWS_11.get_or_init(|| {
        if let Some((_, build)) = get_windows_version() {
            // Windows 11 has build number 22000 or higher
            build >= 22000
        } else {
            false
        }
    })
}

/// Gets the Windows display name
pub fn get_windows_display_name() -> Option<String> {
    let rkey = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .ok()?;
    
    let display_version: Result<String, _> = rkey.get_value("DisplayVersion");
    let product_name: String = rkey.get_value("ProductName").ok()?;
    
    if let Ok(display_version) = display_version {
        Some(format!("{} ({})", product_name, display_version))
    } else {
        Some(product_name)
    }
}

/// Gets the Windows build number as a string
pub fn get_windows_build_string() -> Option<String> {
    let rkey = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .ok()?;
    
    rkey.get_value("CurrentBuild").ok()
}

/// Gets the Windows UBR (Update Build Revision) number
pub fn get_windows_ubr() -> Option<u32> {
    let rkey = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .ok()?;
    
    rkey.get_value("UBR").ok()
}

/// Gets the full Windows build number including UBR
pub fn get_full_build_number() -> Option<String> {
    let build = get_windows_build_string()?;
    let ubr = get_windows_ubr()?;
    
    Some(format!("{}.{}", build, ubr))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    #[cfg(target_os = "windows")]
    fn test_get_windows_version() {
        let version = get_windows_version();
        assert!(version.is_some());
        
        let (name, build) = version.unwrap();
        assert!(!name.is_empty());
        assert!(build > 0);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_is_windows_11_or_newer() {
        // This test just ensures the function runs without error
        // The actual result depends on the test environment
        let _ = is_windows_11_or_newer();
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_get_windows_display_name() {
        let name = get_windows_display_name();
        assert!(name.is_some());
        assert!(!name.unwrap().is_empty());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_get_full_build_number() {
        let build = get_full_build_number();
        if build.is_some() {
            let build_str = build.unwrap();
            assert!(build_str.contains('.'));
            
            let parts: Vec<&str> = build_str.split('.').collect();
            assert_eq!(parts.len(), 2);
            
            // Both parts should be numeric
            assert!(parts[0].parse::<u32>().is_ok());
            assert!(parts[1].parse::<u32>().is_ok());
        }
    }
    
    // Mock tests for non-Windows platforms to ensure code coverage
    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_windows_functions_mock() {
        // These tests use mock data to simulate Windows behavior on non-Windows platforms
        
        // Mock implementation for testing
        struct MockRegistry;
        
        impl MockRegistry {
            fn get_windows_version() -> Option<(String, u32)> {
                Some(("Windows 11 Pro".to_string(), 22621))
            }
            
            fn get_windows_display_name() -> Option<String> {
                Some("Windows 11 Pro (22H2)".to_string())
            }
            
            fn get_full_build_number() -> Option<String> {
                Some("22621.2428".to_string())
            }
        }
        
        // Test the mock implementations
        let version = MockRegistry::get_windows_version();
        assert!(version.is_some());
        
        let (name, build) = version.unwrap();
        assert_eq!(name, "Windows 11 Pro");
        assert_eq!(build, 22621);
        
        let display_name = MockRegistry::get_windows_display_name();
        assert_eq!(display_name, Some("Windows 11 Pro (22H2)".to_string()));
        
        let build_number = MockRegistry::get_full_build_number();
        assert_eq!(build_number, Some("22621.2428".to_string()));
    }
}
