#[allow(unused_imports)]
use q_cli_e2e_tests::{get_chat_session, cleanup_if_last_test};
use std::sync::atomic::AtomicUsize;

#[allow(dead_code)]
static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
#[allow(dead_code)]
const TEST_NAMES: &[&str] = &[
    "test_compact_command",
    "test_compact_help_command",
    "test_compact_h_command",
    "test_show_summary",
    "test_compact_truncate_true_command",
    "test_compact_truncate_false_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(feature = "compact", feature = "sanity"))]
fn test_compact_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /compact command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
     
    let response = chat.execute_command("What is AWS?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    let response = chat.execute_command("/compact")?;
    
    println!("📝 Compact response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify compact response - either success or too short
    if response.contains("history") && response.contains("compacted") && response.contains("successfully") {
        println!("✅ Found compact success message");
    } else if response.contains("Conversation") && response.contains("short") {
        println!("✅ Found conversation too short message");
    } else {
        panic!("Missing expected compact response");
    }
    
    println!("✅ All compact content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "compact", feature = "sanity"))]
fn test_compact_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /compact --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("/compact --help")?;
    
    println!("📝 Compact help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing usage format");
    println!("✅ Found usage format");
    
    // Verify Arguments section
    assert!(response.contains("Arguments:"), "Missing Arguments section");
    println!("✅ Found Arguments section");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("--show-summary"), "Missing --show-summary option");
    assert!(response.contains("--messages-to-exclude"), "Missing --messages-to-exclude option");
    assert!(response.contains("--truncate-large-messages"), "Missing --truncate-large-messages option");
    assert!(response.contains("--max-message-length"), "Missing --max-message-length option");
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found all options and help flags");
    
    println!("✅ All compact help content verified!");
    
     // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(any(feature = "compact", feature = "session_mgmt"), feature = "regression"))]
fn test_compact_h_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /compact -h command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("/compact -h")?;
    
    println!("📝 Compact help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing usage format");
    println!("✅ Found usage format");
    
    // Verify Arguments section
    assert!(response.contains("Arguments:"), "Missing Arguments section");
    println!("✅ Found Arguments section");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("--show-summary"), "Missing --show-summary option");
    assert!(response.contains("--messages-to-exclude"), "Missing --messages-to-exclude option");
    assert!(response.contains("--truncate-large-messages"), "Missing --truncate-large-messages option");
    assert!(response.contains("--max-message-length"), "Missing --max-message-length option");
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found all options and help flags");
    
    println!("✅ All compact help content verified!");
    
     // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}