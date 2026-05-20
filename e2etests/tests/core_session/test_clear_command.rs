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
    "test_clear_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(feature = "clear", feature = "sanity"))]
fn test_clear_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /clear command... | Description: Tests the <code> /clear</code> command to clear conversation history and verify that previous context is no longer remembered by the AI");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    println!("✅ Q Chat session started");
    
    // Send initial message
    println!("\n🔍 Sending prompt: 'My name is TestUser'");
    let _initial_response = chat.execute_command("My name is TestUser")?;
    println!("📝 Initial response: {} bytes", _initial_response.len());
    println!("📝 INITIAL RESPONSE OUTPUT:");
    println!("{}", _initial_response);
    println!("📝 END INITIAL RESPONSE");
    
    // Execute clear command
    println!("\n🔍 Executing command: '/clear'");
    let _clear_response = chat.execute_command("/clear")?;

    println!("✅ Clear command executed");
    
    // Check if AI remembers previous conversation
    println!("\n🔍 Sending prompt: 'What is my name?'");
    let test_response = chat.execute_command("What is my name?")?;
    println!("📝 Test response: {} bytes", test_response.len());
    println!("📝 TEST RESPONSE OUTPUT:");
    println!("{}", test_response);
    println!("📝 END TEST RESPONSE");
    
    // Verify history is cleared - AI shouldn't remember the name
    assert!(!test_response.to_lowercase().contains("testuser"), "Clear command failed - AI still remembers previous conversation");
    println!("✅ Clear command successful - Conversation history cleared.");
    
   // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}