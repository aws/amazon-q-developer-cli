#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;
use std::sync::{Mutex, Once, atomic::{AtomicUsize, Ordering}};
use regex::Regex;

#[allow(dead_code)]
static INIT: Once = Once::new();
#[allow(dead_code)]
static mut CHAT_SESSION: Option<Mutex<q_chat_helper::QChatSession>> = None;

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

#[allow(dead_code)]
const TEST_NAMES: &[&str] = &[
    "test_changelog_command",
    "test_changelog_help_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(feature = "changelog", feature = "sanity"))]
fn test_changelog_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /changelog command... | Description: Tests the <code> /changelog</code> command to display version history and updates");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/changelog")?;
    
    println!("📝 Changelog response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify changelog content
    assert!(response.contains("New") && response.contains("Amazon Q CLI"), "Missing changelog header");
    println!("✅ Found changelog header");
    
    // Verify version format (e.g., 1.16.2)
    let version_regex = Regex::new(r"## \d+\.\d+\.\d+").unwrap();
    assert!(version_regex.is_match(&response), "Missing version format (x.x.x)");
    println!("✅ Found valid version format");
    
    // Verify date format (e.g., 2025-09-19)
    let date_regex = Regex::new(r"\(\d{4}-\d{2}-\d{2}\)").unwrap();
    assert!(date_regex.is_match(&response), "Missing date format (YYYY-MM-DD)");
    println!("✅ Found valid date format");
    
    // Verify /changelog command reference
    assert!(response.contains("/changelog"), "Missing /changelog command reference");
    println!("✅ Found /changelog command reference");
    
    println!("✅ /changelog command test completed successfully");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "changelog", feature = "sanity"))]
fn test_changelog_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /changelog -h command... | Description: Tests the <code> /changelog -h</code> command to display help information");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/changelog -h")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify help content
    assert!(response.contains("Usage:") && response.contains("/changelog"), "Missing usage information");
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found all expected help content");

    println!("✅ /changelog -h command test completed successfully");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}