#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;
use std::sync::{Mutex, Once, atomic::{AtomicUsize, Ordering}};
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
#[allow(dead_code)]
static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
#[allow(dead_code)]
const TEST_NAMES: &[&str] = &[
    "test_help_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(feature = "help", feature = "sanity"))]
fn test_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /help command... | Description: Tests the /help command to display all available commands and verify core functionality like quit, clear, tools, and help commands are present");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/help")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify help content
    assert!(response.contains("Commands:"), "Missing Commands section");
    println!("✅ Found Commands section with all available commands");
    
    assert!(response.contains("quit"), "Missing quit command");
    assert!(response.contains("clear"), "Missing clear command");
    assert!(response.contains("tools"), "Missing tools command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Verified core commands: quit, clear, tools, help");
    
    // Verify specific useful commands
    if response.contains("context") {
        println!("✅ Found context management command");
    }
    if response.contains("agent") {
        println!("✅ Found agent management command");
    }
    if response.contains("model") {
        println!("✅ Found model selection command");
    }
    
    println!("✅ All help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}
