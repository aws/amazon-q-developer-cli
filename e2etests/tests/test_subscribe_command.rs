use q_cli_e2e_tests::{get_chat_session, cleanup_if_last_test};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
const TEST_NAMES: &[&str] = &[
    "test_subscribe_command",
    "test_subscribe_help_command",
];
const TOTAL_TESTS: usize = TEST_NAMES.len();


#[test]
#[cfg(feature = "subscribe")]
fn test_subscribe_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /subscribe command...");

    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/subscribe")?;
    
    println!("📝 Subscribe response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify subscription management message
    assert!(response.contains("Q Developer Pro subscription") && response.contains("IAM Identity Center"), "Missing subscription management message");
    println!("✅ Found subscription management message");
    
    println!("✅ All subscribe content verified!");

    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}
#[test]
#[cfg(feature = "subscribe")]
fn test_subscribe_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /subscribe --help command...");

    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/subscribe --help")?;
    
    println!("📝 Subscribe help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("Q Developer Pro subscription"), "Missing subscription description");
    println!("✅ Found subscription description");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/subscribe"), "Missing /subscribe command in usage section");
    assert!(response.contains("[OPTIONS]"), "Missing [OPTIONS] in usage section");
    println!("✅ Found Usage section with /subscribe [OPTIONS]");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify manage option
    assert!(response.contains("--manage"), "Missing --manage option");
    println!("✅ Found --manage option");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") , "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All subscribe help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}
