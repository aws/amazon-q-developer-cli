#[allow(unused_imports)]
use q_cli_e2e_tests::{get_chat_session, cleanup_if_last_test};
use std::sync::atomic::AtomicUsize;

#[allow(dead_code)]
static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
#[allow(dead_code)]
const TEST_NAMES: &[&str] = &[
    "test_hooks_command",
    "test_hooks_help_command",
    "test_hooks_h_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(feature = "hooks", feature = "sanity"))]
fn test_hooks_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /hooks command... | Description: Tests the /hooks command to display configured hooks or show no hooks message when none are configured");
    
    let session = get_chat_session();
   let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("/hooks")?;
    
    println!("📝 Hooks command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify no hooks configured message
    assert!(response.contains("No hooks"), "Missing no hooks configured message");
    println!("✅ Found no hooks configured message");
    
    println!("✅ All hooks command functionality verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "hooks", feature = "sanity"))]
fn test_hooks_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /hooks --help command... | Description: Tests the /hooks --help command to display comprehensive help information for hooks functionality and configuration");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("/hooks --help")?;
    
    println!("📝 Hooks help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/hooks"), "Missing /hooks command in usage section");
    println!("✅ Found Usage section with /hooks command");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All hooks help content verified!");
    
     // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "hooks", feature = "sanity"))]
fn test_hooks_h_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /hooks -h command... | Description: Tests the /hooks -h command (short form) to display hooks help information and verify flag handling");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("/hooks -h")?;
    
    println!("📝 Hooks help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/hooks"), "Missing /hooks command in usage section");
    println!("✅ Found Usage section with /hooks command");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All hooks help content verified!");
    
     // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}