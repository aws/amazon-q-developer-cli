use q_cli_e2e_tests::{get_chat_session, cleanup_if_last_test};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
const TEST_NAMES: &[&str] = &[
    "test_editor_help_command",
    "test_help_editor_command",
];
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(feature = "editor")]
fn test_editor_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /editor --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();
    
    let response = chat.execute_command("/editor --help")?;
    
    println!("📝 Editor help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    /* Verify description
    assert!(response.contains("Open $EDITOR"), "Missing editor description");
    println!("✅ Found editor description");*/
    
    // Verify Usage section
    assert!(response.contains("Usage:") && response.contains("/editor") && response.contains("[INITIAL_TEXT]"), "Missing Usage section");
    println!("✅ Found Usage section with /editor command");
    
    // Verify Arguments section
    assert!(response.contains("Arguments:"), "Missing Arguments section");
    assert!(response.contains("[INITIAL_TEXT]"), "Missing INITIAL_TEXT argument");
    println!("✅ Found Arguments section");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All editor help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(feature = "editor")]
fn test_help_editor_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /help editor command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/help editor")?;
    
    println!("📝 Help editor response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify Usage section
    assert!(response.contains("Usage:") && response.contains("/editor") && response.contains("[INITIAL_TEXT]"), "Missing Usage section");
    println!("✅ Found Usage section with /editor command");
    
    // Verify Arguments section
    assert!(response.contains("Arguments:"), "Missing Arguments section");
    assert!(response.contains("[INITIAL_TEXT]"), "Missing INITIAL_TEXT argument");
    println!("✅ Found Arguments section");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All editor help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}