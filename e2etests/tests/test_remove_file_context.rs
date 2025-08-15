use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "context")]
fn test_remove_file_context() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context remove <filename> command...");
    
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
    
    // Remove file from context
    let remove_response = chat.execute_command(&format!("/context remove {}", test_file_path))?;
    
    println!("📝 Context remove response: {} bytes", remove_response.len());
    println!("📝 REMOVE RESPONSE:");
    println!("{}", remove_response);
    println!("📝 END REMOVE RESPONSE");
    
    // Verify file was removed successfully
    assert!(remove_response.contains("Removed 1 path(s) from context"), "Missing success message for removing file");
    println!("✅ File removed from context successfully");
    
    // Execute /context show to confirm file is gone
    let final_show_response = chat.execute_command("/context show")?;
    
    println!("📝 Final context show response: {} bytes", final_show_response.len());
    println!("📝 FINAL SHOW RESPONSE:");
    println!("{}", final_show_response);
    println!("📝 END FINAL SHOW RESPONSE");
    
    // Verify file is no longer in context
    assert!(!final_show_response.contains(test_file_path), "File still found in context after removal");
    println!("✅ File confirmed removed from context");
    
    chat.quit()?;
    
    // Clean up test file
    let _ = std::fs::remove_file(test_file_path);
    println!("✅ Cleaned up test file");
    
    println!("✅ Test completed successfully");
    
    Ok(())
}
