#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;
use std::sync::{Mutex, Once, atomic::{AtomicUsize, Ordering}};
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

// List of covered tests
#[allow(dead_code)]
const TEST_NAMES: &[&str] = &[
    "test_subscribe_command",
    "test_subscribe_manage_command",
    "test_subscribe_help_command",
    "test_subscribe_h_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();


#[test]
#[cfg(all(feature = "subscribe", feature = "sanity"))]
fn test_subscribe_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /subscribe command... | Description: Tests the <code> /subscribe</code> command to display Q Developer Pro subscription information and IAM Identity Center details");

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

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
#[cfg(all(feature = "subscribe", feature = "sanity"))]
fn test_subscribe_manage_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /subscribe --manage command... | Description: Tests the <code> /subscribe --manage</code> command to access subscription management interface for Q Developer Pro");

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("/subscribe --manage")?;
    
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
#[cfg(all(feature = "subscribe", feature = "sanity"))]
fn test_subscribe_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /subscribe --help command... | Description: Tests the <code> /subscribe --help</code> command to display comprehensive help information for subscription management");

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

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
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All subscribe help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "subscribe", feature = "sanity"))]
fn test_subscribe_h_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /subscribe -h command... | Description: Tests the <code> /subscribe -h</code> command (short form) to display subscription help information");

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("/subscribe -h")?;
    
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
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All subscribe help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}