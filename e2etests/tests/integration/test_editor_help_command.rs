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
    "test_editor_command_interaction",
    "test_editor_command_error",
    "test_editor_with_file_path",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

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

#[test]
#[cfg(all(feature = "editor", feature = "sanity"))]
fn test_editor_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /editor --help command... | Description: Tests the <code> /editor --help</code> command to display help information for the editor functionality including usage and options");
    
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
    println!("\n🔍 Testing /help editor command... | Description: Tests the <code> /help editor</code> command to display editor-specific help information and usage instructions");
    
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
    println!("\n🔍 Testing /editor -h command... | Description: Tests the <code> /editor -h</code> command (short form) to display editor help information and verify proper flag handling");
    
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

#[test]
#[cfg(all(feature = "editor", feature = "sanity"))]
fn test_editor_command_interaction() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /editor command interaction... | Description: Test that the <code> /editor</code> command successfully launches the integrated editor interface");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Execute /editor command to open editor panel
    let response = chat.execute_command("/editor")?;
    
    println!("📝 Editor command response: {} bytes", response.len());
    println!("📝 EDITOR RESPONSE:");
    println!("{}", response);
    println!("📝 END EDITOR RESPONSE");
    
    // Press 'i' to enter insert mode
    let insert_response = chat.send_key_input("i")?;
    println!("📝 Insert mode response: {} bytes", insert_response.len());
    
    // Type "what is aws?"
    let type_response = chat.execute_command("what is aws?")?;
    println!("📝 Type response: {} bytes", type_response.len());
    
    // Press Esc to exit insert mode
    let esc_response = chat.send_key_input("\x1b")?; // ESC key
    println!("📝 Esc response: {} bytes", esc_response.len());
    
    // Execute :wq to save and quit
    let wq_response = chat.send_key_input(":wq\r")?;
    
    println!("📝 Final wq response: {} bytes", wq_response.len());
    println!("📝 WQ RESPONSE:");
    println!("{}", wq_response);
    println!("📝 END WQ RESPONSE");
    
    // Verify expected output
    assert!(wq_response.contains("Content loaded from editor. Submitting prompt..."), "Missing expected editor output message");
    println!("✅ Found expected editor output: 'Content loaded from editor. Submitting prompt...'");
    
    println!("✅ Editor command interaction test completed successfully!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "editor", feature = "sanity"))]
fn test_editor_command_error() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /editor command error handling ... | Description: Tests the <code> /editor <non_exixt_filepath> </code> command error handling when attempting to open a nonexistent file");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Execute /editor command to open editor panel
    let response = chat.execute_command("/editor nonexistent_file.txt")?;
    
    println!("📝 Editor command response: {} bytes", response.len());
    println!("📝 EDITOR RESPONSE:");
    println!("{}", response);
    println!("📝 END EDITOR RESPONSE");
    
    // Press 'i' to enter insert mode
    let insert_response = chat.send_key_input("i")?;
    println!("📝 Insert mode response: {} bytes", insert_response.len());
    
    
    // Press Esc to exit insert mode
    let esc_response = chat.send_key_input("\x1b")?; // ESC key
    println!("📝 Esc response: {} bytes", esc_response.len());
    
    // Execute :wq to save and quit
    let wq_response = chat.send_key_input(":wq\r")?;
    
    println!("📝 Final wq response: {} bytes", wq_response.len());
    println!("📝 WQ RESPONSE:");
    println!("{}", wq_response);
    println!("📝 END WQ RESPONSE");

    
    // Verify expected output
    assert!(wq_response.contains("Content loaded from editor. Submitting prompt..."), "Missing expected editor output message");
    println!("✅ Found expected editor output: 'Content loaded from editor. Submitting prompt...'");
   
    assert!(wq_response.contains("nonexistent_file.txt") && wq_response.contains("does not exist"), "Missing file validation error message");
    println!("✅ Found expected file validation error message");

    println!("✅ Editor command error test completed successfully!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "editor", feature = "sanity"))]
fn test_editor_with_file_path() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /editor <filepath> command... | Description: Tests the <code> /editor <filepath></code> command to load an existing file into the editor and verify content loading");
    
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let test_file_path = format!("{}/test_editor_file.txt", home_dir);
    
    // Create a test file
    std::fs::write(&test_file_path, "Hello from test file\nThis is a test file for editor command.")?;
    println!("✅ Created test file at {}", test_file_path);
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Execute /editor command with file path
    let response = chat.execute_command(&format!("/editor {}", test_file_path))?;
    
    println!("📝 Editor with file response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
     // Press 'i' to enter insert mode
    let insert_response = chat.send_key_input("i")?;
    println!("📝 Insert mode response: {} bytes", insert_response.len());
    
    
    // Press Esc to exit insert mode
    let esc_response = chat.send_key_input("\x1b")?; // ESC key
    println!("📝 Esc response: {} bytes", esc_response.len());
    
    // Execute :wq to save and quit
    let wq_response = chat.send_key_input(":wq\r")?;
    
    println!("📝 Final wq response: {} bytes", wq_response.len());
    println!("📝 WQ RESPONSE:");
    println!("{}", wq_response);
    println!("📝 END WQ RESPONSE");


    if wq_response.contains("Using tool:") && wq_response.contains("Allow this action?"){
            let allow_response = chat.execute_command("y")?;

            println!("📝 Allow response: {} bytes", allow_response.len());
            println!("📝 ALLOW RESPONSE:");
            println!("{}", allow_response);
            println!("📝 END ALLOW RESPONSE");

            // Verify the file content is loaded in editor
            assert!(allow_response.contains("Hello from test file"), "File content not loaded in editor");
            println!("✅ File content loaded successfully in editor");
    
    }
    else{
        // Verify the file content is loaded in editor
        assert!(wq_response.contains("Hello from test file"), "File content not loaded in editor");
        println!("✅ File content loaded successfully in editor");
    }
    
    
    // Clean up test file
    std::fs::remove_file(test_file_path).ok();
    println!("✅ Cleaned up test file");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}