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
#[cfg(feature = "save_load")]
fn test_load_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /load command...");
    
    let save_path = "/tmp/qcli_test_load.json";
    let _cleanup = FileCleanup { path: save_path };
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
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
    assert!(save_response.contains("Exported conversation state to") && save_response.contains(save_path), "Missing export confirmation message");
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
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
