use q_cli_e2e_tests::q_chat_helper::QChatSession;

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
fn test_save_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /save command...");
    
    let save_path = "/tmp/qcli_test_save.json";
    let _cleanup = FileCleanup { path: save_path };
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
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
    assert!(response.contains("Exported conversation state to") && response.contains(save_path), "Missing export confirmation message");
    println!("✅ Found expected export message with file path");
    
    // Verify file was created and contains expected data
    assert!(std::path::Path::new(save_path).exists(), "Save file was not created");
    println!("✅ Save file created at {}", save_path);
    
    let file_content = std::fs::read_to_string(save_path)?;
    assert!(file_content.contains("help") || file_content.contains("tools"), "File missing expected conversation data");
    println!("✅ File contains expected conversation data");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
