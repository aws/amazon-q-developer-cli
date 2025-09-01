#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;
use std::sync::{Mutex, Once, atomic::{AtomicUsize, Ordering}};

#[allow(dead_code)]
static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
#[allow(dead_code)]
const TEST_NAMES: &[&str] = &[
    "test_editor_help_command",
    "test_help_editor_command",
    "test_editor_h_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

static INIT: Once = Once::new();
static mut CHAT_SESSION: Option<Mutex<q_chat_helper::QChatSession>> = None;

pub fn get_chat_session() -> &'static Mutex<q_chat_helper::QChatSession> {
    unsafe {
        INIT.call_once(|| {
            let chat = q_chat_helper::QChatSession::new().expect("Failed to create chat session");
            println!("✅ Q Chat session started");
            CHAT_SESSION = Some(Mutex::new(chat));
        });
        (&raw const CHAT_SESSION).as_ref().unwrap().as_ref().unwrap()
    }
}

pub fn cleanup_if_last_test(test_count: &AtomicUsize, total_tests: usize) -> Result<usize, Box<dyn std::error::Error>> {
    let count = test_count.fetch_add(1, Ordering::SeqCst) + 1;
    if count == total_tests {
        unsafe {
            if let Some(session) = (&raw const CHAT_SESSION).as_ref().unwrap() {
                if let Ok(mut chat) = session.lock() {
                    chat.quit()?;
                    println!("✅ Test completed successfully");
                }
            }
        }
    }
  Ok(count)
}

#[test]
#[cfg(all(feature = "editor", feature = "sanity"))]
fn test_editor_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /editor --help command... | Description: Tests the /editor --help command to display help information for the editor functionality including usage and options");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/editor --help")?;
    
    println!("📝 Editor help response: {} bytes", response.len());
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

#[test]
#[cfg(all(feature = "editor", feature = "sanity"))]
fn test_help_editor_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /help editor command... | Description: Tests the /help editor command to display editor-specific help information and usage instructions");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

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

#[test]
#[cfg(all(feature = "editor", feature = "sanity"))]
fn test_editor_h_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /editor -h command... | Description: Tests the /editor -h command (short form) to display editor help information and verify proper flag handling");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/editor -h")?;
    
    println!("📝 Editor help response: {} bytes", response.len());
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
