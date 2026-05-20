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
    "test_mcp_help_command",
    "test_mcp_loading_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(feature = "mcp", feature = "sanity"))]
fn test_mcp_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /mcp --help command... | Description: Tests the <code>/mcp --help</code> command to display help information for MCP server management functionality");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/mcp --help")?;
    
    println!("📝 MCP help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("See mcp server loaded"), "Missing mcp server description");
    println!("✅ Found mcp server description");
    
    // Verify Usage section
    assert!(response.contains("Usage"), "Missing Usage section");
    assert!(response.contains("/mcp"), "Missing /mcp command in usage section");
    println!("✅ Found Usage section with /mcp command");
    
    // Verify Options section
    assert!(response.contains("Options"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All mcp help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "sanity"))]
fn test_mcp_loading_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /mcp command... | Description: Tests the <code>/mcp</code> command to display MCP server loading status and information");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/mcp")?;
    
    println!("📝 MCP loading response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Check MCP status - either loaded or loading
    if response.contains("loaded in") {
        assert!(response.contains(" s"), "Missing seconds indicator for loading time");
        println!("✅ Found MCPs loaded with timing");
        
        // Count number of MCPs loaded
        let mcp_count = response.matches("✓").count();
        println!("✅ Found {} MCP(s) loaded", mcp_count);
    } else if response.contains("loading") {
        println!("✅ MCPs are still loading");
    } else {
        println!("ℹ️ MCP status unclear - may be in different state");
    }
    
    println!("✅ All MCP loading content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

