#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

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
#[cfg(all(feature = "save_load", feature = "sanity"))]
fn test_save_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /save command... | Description: Tests the <code> /save</code> command to export conversation state to a file and verify successful file creation with conversation data");
    
    let save_path = "/tmp/qcli_test_save.json";
    let _cleanup = FileCleanup { path: save_path };
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Create actual conversation content
    let _help_response = chat.execute_command_with_timeout("/help",Some(2000))?;
    let _tools_response = chat.execute_command_with_timeout("/tools",Some(2000))?;
    println!("✅ Created conversation content with /help and /tools commands");
    
    // Execute /save command
    let response = chat.execute_command_with_timeout(&format!("/save {}", save_path),Some(2000))?;
    
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
    
    // Release the lock
    drop(chat);
    
    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "sanity"))]
fn test_save_command_argument_validation() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /save command argument validation... | Description: Tests the <code> /save</code> command without required arguments to verify proper error handling and usage display");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command_with_timeout("/save",Some(2000))?;
    
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
    
    // Release the lock
    drop(chat);
    
    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "sanity"))]
fn test_save_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /save --help command... | Description: Tests the <code> /save --help</code> command to display comprehensive help information for save functionality");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command_with_timeout("/save --help",Some(2000))?;
    
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
    
    assert!(response.contains("Options"), "Missing Options section");
    println!("✅ Found Options section");
    
    println!("✅ All help content verified!");
    
    // Release the lock
    drop(chat);
    
    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "sanity"))]
fn test_save_h_flag_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /save -h command... | Description: Tests the <code> /save -h</code> command (short form) to display save help information");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command_with_timeout("/save -h",Some(2000))?;
    
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
    
    assert!(response.contains("Options"), "Missing Options section");
    println!("✅ Found Options section");
    
    println!("✅ All help content verified!");
    
    // Release the lock
    drop(chat);
    
    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "sanity"))]
fn test_save_force_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /save --force command... | Description: Tests the <code> /save --force</code> command to overwrite existing files and verify force save functionality");
    
    let save_path = "/tmp/qcli_test_save.json";
    let _cleanup = FileCleanup { path: save_path };

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Create actual conversation content
    let _help_response = chat.execute_command_with_timeout("/help",Some(2000))?;
    let _tools_response = chat.execute_command_with_timeout("/tools",Some(2000))?;
    println!("✅ Created conversation content with /help and /tools commands");

    // Execute /save command first
    let response = chat.execute_command_with_timeout(&format!("/save {}", save_path),Some(2000))?;
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

    // Release the lock
    drop(chat);

    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "sanity"))]
fn test_save_f_flag_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /save -f command... | Description: Tests the <code> /save -f</code> command (short form) to force overwrite existing files");
    
    let save_path = "/tmp/qcli_test_save.json";
    let _cleanup = FileCleanup { path: save_path };

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Create actual conversation content
    let _help_response = chat.execute_command_with_timeout("/help",Some(2000))?;
    let _tools_response = chat.execute_command_with_timeout("/tools",Some(2000))?;
    println!("✅ Created conversation content with /help and /tools commands");

    // Execute /save command first
    let response = chat.execute_command_with_timeout(&format!("/save {}", save_path),Some(2000))?;
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    assert!(response.contains("Exported"), "Initial save should succeed");
    println!("✅ Initial save completed");

    // Add more conversation content after initial save
    let _prompt_response = chat.execute_command_with_timeout("/context show",Some(2000))?;
    println!("✅ Added more conversation content after initial save");

    // Execute /save -f command to overwrite with new content
    let force_response = chat.execute_command_with_timeout(&format!("/save -f {}", save_path),Some(2000))?;

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

    // Release the lock
    drop(chat);

    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "sanity"))]
fn test_load_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /load --help command... | Description: Tests the <code> /load --help</code> command to display comprehensive help information for load functionality");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command_with_timeout("/load --help",Some(2000))?;
    
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
    println!("✅ Found Options section");
    
    println!("✅ All help content verified!");
    
    // Release the lock
    drop(chat);
    
    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "sanity"))]
fn test_load_h_flag_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /load -h command... | Description: Tests the <code> /load -h</code> command (short form) to display load help information");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command_with_timeout("/load -h",Some(2000))?;
    
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
    println!("✅ Found Options section");
    
    println!("✅ All help content verified!");
    
    // Release the lock
    drop(chat);
    
    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "sanity"))]
fn test_load_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /load command... | Description: Tests the <code> /load</code> command to import conversation state from a saved file and verify successful restoration");
    
    let save_path = "/tmp/qcli_test_load.json";
    let _cleanup = FileCleanup { path: save_path };
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Create actual conversation content
    let _help_response = chat.execute_command_with_timeout("/help",Some(2000))?;
    let _tools_response = chat.execute_command_with_timeout("/tools",Some(2000))?;
    println!("✅ Created conversation content with /help and /tools commands");
    
    // Execute /save command to create a file to load
    let save_response = chat.execute_command_with_timeout(&format!("/save {}", save_path),Some(2000))?;
    
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
    let load_response = chat.execute_command_with_timeout(&format!("/load {}", save_path),Some(2000))?;
    
    println!("📝 Load response: {} bytes", load_response.len());
    println!("📝 LOAD OUTPUT:");
    println!("{}", load_response);
    println!("📝 END LOAD OUTPUT");
    
    // Verify load was successful
    assert!(!load_response.is_empty(), "Load command should return non-empty response");
    assert!(load_response.contains("Imported") && load_response.contains(save_path), "Missing import confirmation message");
    println!("✅ Load command executed successfully and imported conversation state");
    
    // Release the lock
    drop(chat);
    
    Ok(())
}

#[test]
#[cfg(all(feature = "save_load", feature = "sanity"))]
fn test_load_command_argument_validation() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /load command argument validation... | Description: Tests the <code>/load</code> command without required arguments to verify proper error handling and usage display");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command_with_timeout("/load",Some(2000))?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify load error message
    assert!(response.contains("error"), "Missing load error message");
    println!("✅ Found load error message");
    
    assert!(response.contains("Usage"), "Missing Usage section");
    assert!(response.contains("/load"), "Missing /load command in usage");
    println!("✅ Found Usage section with /load command");
    
    assert!(response.contains("Arguments"), "Missing Arguments section");
    assert!(response.contains("<PATH>"), "Missing PATH argument");
    println!("✅ Found Arguments section with PATH parameter");
    
    assert!(response.contains("Options"), "Missing Options section");
    println!("✅ Found Options section");
    
    println!("✅ All help content verified!");
    
    // Release the lock
    drop(chat);
    
    Ok(())
}