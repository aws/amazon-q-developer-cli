#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;
use std::sync::{Mutex, Once, atomic::{AtomicUsize}};
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
static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
#[allow(dead_code)]
const TEST_NAMES: &[&str] = &[
    "test_quit_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(feature = "quit", feature = "sanity"))]
fn test_quit_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /quit command... | Description: Tests the <code> /quit</code> command to properly terminate the Q Chat session and exit cleanly");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    println!("✅ Q Chat session started");
    
    chat.execute_command("/quit")?;
    
    println!("✅ /quit command executed successfully");
    println!("✅ Test completed successfully");

    Ok(())
}
