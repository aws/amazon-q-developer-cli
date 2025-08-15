use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "context")]
fn test_add_file_context() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context add <filename> command...");
    
    let test_file_path = "/tmp/test_context_file.py";
    
    // Create a test file
    std::fs::write(test_file_path, "# Test file for context\nprint('Hello from test file')")?;
    println!("✅ Created test file at {}", test_file_path);
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // Add file to context
    let add_response = chat.execute_command(&format!("/context add {}", test_file_path))?;
    
    println!("📝 Context add response: {} bytes", add_response.len());
    println!("📝 ADD RESPONSE:");
    println!("{}", add_response);
    println!("📝 END ADD RESPONSE");
    
    // Verify file was added successfully
    assert!(add_response.contains("Added 1 path(s) to context"), "Missing success message for adding file");
    println!("✅ File added to context successfully");
    
    // Execute /context show to confirm file is present
    let show_response = chat.execute_command("/context show")?;
    
    println!("📝 Context show response: {} bytes", show_response.len());
    println!("📝 SHOW RESPONSE:");
    println!("{}", show_response);
    println!("📝 END SHOW RESPONSE");
    
    // Verify file is present in context
    assert!(show_response.contains(test_file_path), "File not found in context show output");
    assert!(show_response.contains("💬 Session (temporary):"), "Missing Session section");
    println!("✅ File confirmed present in context");
    
    chat.quit()?;
    
    // Clean up test file
    let _ = std::fs::remove_file(test_file_path);
    println!("✅ Cleaned up test file");
    
    println!("✅ Test completed successfully");
    
    Ok(())
}
