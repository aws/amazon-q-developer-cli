#[allow(unused_imports)]
use q_cli_e2e_tests::{get_chat_session, cleanup_if_last_test};
use std::sync::atomic::AtomicUsize;

#[allow(dead_code)]
static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
#[allow(dead_code)]
const TEST_NAMES: &[&str] = &[
    "test_save_command",
    "test_save_command_argument_validation",
    "test_save_help_command",
    "test_save_h_flag_command",
    "test_save_force_command",
    "test_save_f_flag_command",
    "test_load_help_command",
    "test_load_h_flag_command",
    "test_load_command",
    "test_load_command_argument_validation"
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[allow(dead_code)]
struct FileCleanup<'a> {
    path: &'a str,
}

impl<'a> Drop for FileCleanup<'a> {
    fn drop(&mut self) {
        if std::path::Path::new(self.path).exists() {
            let _ = std::fs::remove_file(self.path);
            println!("✅ Cleaned up test file");
        }
    }
}

#[test]
#[cfg(all(feature = "save_load", feature = "regression"))]
fn test_save_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /save command...");
    
    let save_path = "/tmp/qcli_test_save.json";
    let _cleanup = FileCleanup { path: save_path };
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Create actual conversation content
    let _help_response = chat.execute_command("/help")?;
    let _tools_response = chat.execute_command("/tools")?;
    println!("✅ Created conversation content with /help and /tools commands");
    
    // Execute /save command
    let response = chat.execute_command(&format!("/save {}", save_path))?;
    
    println!("📝 Save response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify "Exported conversation state to [file path]" message
    assert!(response.contains("Exported") && response.contains(save_path), "Missing export confirmation message");
    println!("✅ Found expected export message with file path");
    
    // Verify file was created and contains expected data
    assert!(std::path::Path::new(save_path).exists(), "Save file was not created");
    println!("✅ Save file created at {}", save_path);
    
    let file_content = std::fs::read_to_string(save_path)?;
    assert!(file_content.contains("help") || file_content.contains("tools"), "File missing expected conversation data");
    println!("✅ File contains expected conversation data");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "regression"))]
fn test_save_command_argument_validation() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /save command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/save")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify save error message
    assert!(response.contains("error"), "Missing save error message");
    println!("✅ Found save error message");
    
    assert!(response.contains("Usage"), "Missing Usage section");
    assert!(response.contains("/save"), "Missing /save command in usage");
    println!("✅ Found Usage section with /save command");
    
    assert!(response.contains("Arguments"), "Missing Arguments section");
    assert!(response.contains("<PATH>"), "Missing PATH argument");
    println!("✅ Found Arguments section with PATH parameter");
    
    println!("✅ All help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "regression"))]
fn test_save_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /save --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/save --help")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify save command help content
    assert!(response.contains("Save"), "Missing save command description");
    println!("✅ Found save command description");
    
    assert!(response.contains("Usage"), "Missing Usage section");
    assert!(response.contains("/save"), "Missing /save command in usage");
    println!("✅ Found Usage section with /save command");
    
    assert!(response.contains("Arguments"), "Missing Arguments section");
    assert!(response.contains("<PATH>"), "Missing PATH argument");
    println!("✅ Found Arguments section with PATH parameter");
    
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-f"), "Missing -f flag");
    assert!(response.contains("--force"), "Missing --force flag");
    assert!(response.contains("-h"), "Missing -h flag");
    assert!(response.contains("--help") || response.contains("—help"), "Missing --help flag");
    println!("✅ Found Options section with -f, --force, -h, --help flags");
    
    println!("✅ All help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    
    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "regression"))]
fn test_save_h_flag_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /save -h command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/save -h")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify save command help content
    assert!(response.contains("Save"), "Missing save command description");
    println!("✅ Found save command description");
    
    assert!(response.contains("Usage"), "Missing Usage section");
    assert!(response.contains("/save"), "Missing /save command in usage");
    println!("✅ Found Usage section with /save command");
    
    assert!(response.contains("Arguments"), "Missing Arguments section");
    assert!(response.contains("<PATH>"), "Missing PATH argument");
    println!("✅ Found Arguments section with PATH parameter");
    
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-f"), "Missing -f flag");
    assert!(response.contains("--force"), "Missing --force flag");
    assert!(response.contains("-h"), "Missing -h flag");
    assert!(response.contains("--help") || response.contains("—help"), "Missing --help flag");
    println!("✅ Found Options section with -f, --force, -h, --help flags");
    
    println!("✅ All help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "regression"))]
fn test_save_force_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /save --force command...");
    
    let save_path = "/tmp/qcli_test_save.json";
    let _cleanup = FileCleanup { path: save_path };

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Create actual conversation content
    let _help_response = chat.execute_command("/help")?;
    let _tools_response = chat.execute_command("/tools")?;
    println!("✅ Created conversation content with /help and /tools commands");

    // Execute /save command first
    let response = chat.execute_command(&format!("/save {}", save_path))?;
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    assert!(response.contains("Exported"), "Initial save should succeed");
    println!("✅ Initial save completed");

    // Add more conversation content after initial save
    let _prompt_response = chat.execute_command("/context show")?;
    println!("✅ Added more conversation content after initial save");

    // Execute /save --force command to overwrite with new content
    let force_response = chat.execute_command(&format!("/save --force {}", save_path))?;

    println!("📝 Save force response: {} bytes", force_response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", force_response);
    println!("📝 END OUTPUT");

    // Verify force save message
    assert!(force_response.contains("Exported") && force_response.contains(save_path), "Missing export confirmation message");
    println!("✅ Found expected export message with file path");

    // Verify file exists and contains data
    assert!(std::path::Path::new(save_path).exists(), "Save file was not created");
    println!("✅ Save file created at {}", save_path);

    let file_content = std::fs::read_to_string(save_path)?;
    assert!(file_content.contains("help") || file_content.contains("tools"), "File missing initial conversation data");
    assert!(file_content.contains("context"), "File missing additional conversation data");
    println!("✅ File contains expected conversation data including additional content");

    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "regression"))]
fn test_save_f_flag_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /save -f command...");
    
    let save_path = "/tmp/qcli_test_save.json";
    let _cleanup = FileCleanup { path: save_path };

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Create actual conversation content
    let _help_response = chat.execute_command("/help")?;
    let _tools_response = chat.execute_command("/tools")?;
    println!("✅ Created conversation content with /help and /tools commands");

    // Execute /save command first
    let response = chat.execute_command(&format!("/save {}", save_path))?;
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    assert!(response.contains("Exported"), "Initial save should succeed");
    println!("✅ Initial save completed");

    // Add more conversation content after initial save
    let _prompt_response = chat.execute_command("/context show")?;
    println!("✅ Added more conversation content after initial save");

    // Execute /save -f command to overwrite with new content
    let force_response = chat.execute_command(&format!("/save -f {}", save_path))?;

    println!("📝 Save force response: {} bytes", force_response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", force_response);
    println!("📝 END OUTPUT");

    // Verify force save message
    assert!(force_response.contains("Exported") && force_response.contains(save_path), "Missing export confirmation message");
    println!("✅ Found expected export message with file path");

    // Verify file exists and contains data
    assert!(std::path::Path::new(save_path).exists(), "Save file was not created");
    println!("✅ Save file created at {}", save_path);

    let file_content = std::fs::read_to_string(save_path)?;
    assert!(file_content.contains("help") || file_content.contains("tools"), "File missing initial conversation data");
    assert!(file_content.contains("context"), "File missing additional conversation data");
    println!("✅ File contains expected conversation data including additional content");

    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "regression"))]
fn test_load_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /load --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/load --help")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify load command help content
    assert!(response.contains("Load"), "Missing load command description");
    println!("✅ Found load command description");
    
    assert!(response.contains("Usage"), "Missing Usage section");
    assert!(response.contains("/load"), "Missing /load command in usage");
    println!("✅ Found Usage section with /load command");
    
    assert!(response.contains("Arguments"), "Missing Arguments section");
    assert!(response.contains("<PATH>"), "Missing PATH argument");
    println!("✅ Found Arguments section with PATH parameter");
    
    assert!(response.contains("Options"), "Missing Options section");
    assert!(response.contains("-h"), "Missing -h flag");
    assert!(response.contains("--help"), "Missing --help flag");
    println!("✅ Found Options section with -h, --help flags");
    
    println!("✅ All help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "regression"))]
fn test_load_h_flag_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /load -h command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/load -h")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify load command help content
    assert!(response.contains("Load"), "Missing load command description");
    println!("✅ Found load command description");
    
    assert!(response.contains("Usage"), "Missing Usage section");
    assert!(response.contains("/load"), "Missing /load command in usage");
    println!("✅ Found Usage section with /load command");
    
    assert!(response.contains("Arguments"), "Missing Arguments section");
    assert!(response.contains("<PATH>"), "Missing PATH argument");
    println!("✅ Found Arguments section with PATH parameter");
    
    assert!(response.contains("Options"), "Missing Options section");
    assert!(response.contains("-h"), "Missing -h flag");
    assert!(response.contains("--help"), "Missing --help flag");
    println!("✅ Found Options section with -h, --help flags");
    
    println!("✅ All help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "regression"))]
fn test_load_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /load command...");
    
    let save_path = "/tmp/qcli_test_load.json";
    let _cleanup = FileCleanup { path: save_path };
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Create actual conversation content
    let _help_response = chat.execute_command("/help")?;
    let _tools_response = chat.execute_command("/tools")?;
    println!("✅ Created conversation content with /help and /tools commands");
    
    // Execute /save command to create a file to load
    let save_response = chat.execute_command(&format!("/save {}", save_path))?;
    
    println!("📝 Save response: {} bytes", save_response.len());
    println!("📝 SAVE OUTPUT:");
    println!("{}", save_response);
    println!("📝 END SAVE OUTPUT");
    
    // Verify save was successful
    assert!(save_response.contains("Exported") && save_response.contains(save_path), "Missing export confirmation message");
    println!("✅ Save completed successfully");
    
    // Verify file was created
    assert!(std::path::Path::new(save_path).exists(), "Save file was not created");
    println!("✅ Save file created at {}", save_path);
    
    // Execute /load command to load the saved conversation
    let load_response = chat.execute_command(&format!("/load {}", save_path))?;
    
    println!("📝 Load response: {} bytes", load_response.len());
    println!("📝 LOAD OUTPUT:");
    println!("{}", load_response);
    println!("📝 END LOAD OUTPUT");
    
    // Verify load was successful
    assert!(!load_response.is_empty(), "Load command should return non-empty response");
    assert!(load_response.contains("Imported conversation state from") && load_response.contains(save_path), "Missing import confirmation message");
    println!("✅ Load command executed successfully and imported conversation state");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "regression"))]
fn test_load_command_argument_validation() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /load command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/load")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify load error message
    assert!(response.contains("error:"), "Missing load error message");
    println!("✅ Found load error message");
    
    assert!(response.contains("Usage"), "Missing Usage section");
    assert!(response.contains("/load"), "Missing /load command in usage");
    println!("✅ Found Usage section with /load command");
    
    assert!(response.contains("Arguments"), "Missing Arguments section");
    assert!(response.contains("<PATH>"), "Missing PATH argument");
    println!("✅ Found Arguments section with PATH parameter");
    
    assert!(response.contains("Options"), "Missing Options section");
    assert!(response.contains("-h"), "Missing -h flag");
    assert!(response.contains("--help") || response.contains("—help"), "Missing --help flag");
    println!("✅ Found Options section with -h, --help flags");
    
    println!("✅ All help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

