#[allow(unused_imports)]
use q_cli_e2e_tests::{get_chat_session, cleanup_if_last_test};
use std::sync::atomic::AtomicUsize;

#[allow(dead_code)]
static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
#[allow(dead_code)]
const TEST_NAMES: &[&str] = &[
    "test_usage_command",
    "test_usage_help_command",
    "test_usage_h_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

/// Tests the /usage command to display current context window usage
/// Verifies token usage information, progress bar, breakdown sections, and Pro Tips
#[test]
#[cfg(all(feature = "usage", feature = "sanity"))]
fn test_usage_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /usage command... | Description: Tests the /usage command to display current context window usage. Verifies token usage information, progress bar, breakdown sections, and Pro Tips");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/usage")?;
    
    println!("📝 Tools response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify context window information
    assert!(response.contains("Current context window"), "Missing context window header");
    assert!(response.contains("tokens"), "Missing tokens used information");
    println!("✅ Found context window and token usage information");
    
    // Verify progress bar
    assert!(response.contains("%"), "Missing percentage display");
    println!("✅ Found progress bar with percentage");
    
    // Verify token breakdown sections
    assert!(response.contains(" Context files:"), "Missing Context files section");
    assert!(response.contains(" Tools:"), "Missing Tools section");
    assert!(response.contains(" Q responses:"), "Missing Q responses section");
    assert!(response.contains(" Your prompts:"), "Missing Your prompts section");
    println!("✅ Found all token breakdown sections");
    
    // Verify token counts and percentages format
    assert!(response.contains("tokens ("), "Missing token count format");
    assert!(response.contains("%)"), "Missing percentage format in breakdown");
    println!("✅ Verified token count and percentage format");
    
    // Verify Pro Tips section
    assert!(response.contains(" Pro Tips:"), "Missing Pro Tips section");
    println!("✅ Found Pro Tips section");
    
    // Verify specific tip commands
    assert!(response.contains("/compact"), "Missing /compact command tip");
    assert!(response.contains("/clear"), "Missing /clear command tip");
    assert!(response.contains("/context show"), "Missing /context show command tip");
    println!("✅ Found all command tips: /compact, /clear, /context show");
    
    println!("✅ All usage content verified!");
    
    println!("✅ Test completed successfully");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

/// Tests the /usage --help command to display help information for the usage command
/// Verifies Usage section, Options section, and help flags (-h, --help)
#[test]
#[cfg(all(feature = "usage", feature = "sanity"))]
fn test_usage_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /usage --help command... | Description: Tests the /usage --help command to display help information for the usage command. Verifies Usage section, Options section, and help flags (-h, --help)");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/usage --help")?;
    
    println!("📝 Usage help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");

    assert!(response.contains("/usage"), "Missing /usage command in usage section");
    println!("✅ Found Usage section with /usage command");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with description");
    
    println!("✅ All usage help content verified!");
    
    println!("✅ Test completed successfully");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

/// Tests the /usage -h command (short form of --help)
/// Verifies Usage section, Options section, and help flags (-h, --help)
#[test]
#[cfg(all(feature = "usage", feature = "sanity"))]
fn test_usage_h_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /usage -h command... | Description: Tests the /usage -h command (short form of --help). Verifies Usage section, Options section, and help flags (-h, --help)");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/usage -h")?;
    
    println!("📝 Usage help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/usage"), "Missing /usage command in usage section");
    println!("✅ Found Usage section with /usage command");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with description");
    
    println!("✅ All usage help content verified!");
    
    println!("✅ Test completed successfully");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}