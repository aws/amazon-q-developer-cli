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
    "test_prompts_command",
    "test_prompts_help_command",
    "test_prompts_list_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();


#[test]
#[cfg(all(feature = "ai_prompts", feature = "sanity"))]
fn test_prompts_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /prompts command... | Description: Tests the <code> /prompts</code> command to display available prompts with usage instructions and argument requirements");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();
    
    let response = chat.execute_command("/prompts")?;
    
    println!("📝 Prompts command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify usage instruction
    assert!(response.contains("Usage:") && response.contains("@") && response.contains("<prompt name>") && response.contains("[...args]"), "Missing usage instruction");
    println!("✅ Found usage instruction");
    
    // Verify table headers
    assert!(response.contains("Prompt"), "Missing Prompt header");
    assert!(response.contains("Arguments") && response.contains("*") && response.contains("required"), "Missing Arguments header");
    println!("✅ Found table headers with required notation");
    
    // Verify command executed successfully
    assert!(!response.is_empty(), "Empty response from prompts command");
    println!("✅ Command executed with response");
    
    println!("✅ All prompts command functionality verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "ai_prompts", feature = "sanity"))]
fn test_prompts_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /prompts --help command... | Description: Tests the <code> /prompts --help</code> command to display comprehensive help information about prompts functionality and MCP server integration");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();
    
    let response = chat.execute_command("/prompts --help")?;
    
    println!("📝 Prompts help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("Prompts are reusable templates that help you quickly access common workflows and tasks"), "Missing prompts description");
    assert!(response.contains("These templates are provided by the mcp servers you have installed and configured"), "Missing MCP servers description");
    println!("✅ Found prompts description");
    
    // Verify usage examples
    assert!(response.contains("@") && response.contains("<prompt name> [arg]") && response.contains("[arg]"), "Missing @ syntax example");
    assert!(response.contains("Retrieve prompt specified"), "Missing retrieve description");
    assert!(response.contains("/prompts") && response.contains("get") && response.contains("<prompt name>") && response.contains("[arg]"), "Missing long form example");
    println!("✅ Found usage examples with @ syntax and long form");
    
    // Verify main description
    assert!(response.contains("View and retrieve prompts"), "Missing main description");
    println!("✅ Found main description");
    
    // Verify Usage section
    assert!(response.contains("Usage:") && response.contains("/prompts") && response.contains("[COMMAND]"), "Missing usage format");
    println!("✅ Found usage format");
    
    // Verify Commands section
    assert!(response.contains("Commands:"), "Missing Commands section");
    assert!(response.contains("list"), "Missing list command");
    assert!(response.contains("get"), "Missing get command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Found all commands: list, get, help");
    
    // Verify command descriptions
    assert!(response.contains("List available prompts from a tool or show all available prompt"), "Missing list description");
    println!("✅ Found command descriptions");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h") && response.contains("--help"), "Missing help flags");
    println!("✅ Found Options section with help flags");
    
    println!("✅ All prompts help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "ai_prompts", feature = "sanity"))]
fn test_prompts_list_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /prompts list command... | Description: Tests the <code> /prompts list</code> command to display all available prompts with their arguments and usage information");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();
    
    let response = chat.execute_command("/prompts list")?;
    
    println!("📝 Prompts list response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify usage instruction
    assert!(response.contains("Usage:") && response.contains("@") && response.contains("<prompt name>") && response.contains("[...args]"), "Missing usage instruction");
    println!("✅ Found usage instruction");
    
    // Verify table headers
    assert!(response.contains("Prompt"), "Missing Prompt header");
    assert!(response.contains("Arguments") && response.contains("*") && response.contains("required"), "Missing Arguments header");
    println!("✅ Found table headers with required notation");
    
    // Verify command executed successfully
    assert!(!response.is_empty(), "Empty response from prompts list command");
    println!("✅ Command executed with response");
    
    println!("✅ All prompts list command functionality verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}