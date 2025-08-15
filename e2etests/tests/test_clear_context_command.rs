use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "context")]
fn test_clear_context_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context clear command...");
    
    let test_file1_path = "/tmp/test_context_file1.py";
    let test_file2_path = "/tmp/test_context_file2.js";
    
    // Create multiple test files
    std::fs::write(test_file1_path, "# Test Python file for context\nprint('Hello from Python file')")?;
    std::fs::write(test_file2_path, "// Test JavaScript file for context\nconsole.log('Hello from JS file');")?;
    println!("✅ Created test files at {}, {}", test_file1_path, test_file2_path);
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // Add multiple files to context
    let add_response = chat.execute_command(&format!("/context add {} {}", test_file1_path, test_file2_path))?;
    
    println!("📝 Context add response: {} bytes", add_response.len());
    println!("📝 ADD RESPONSE:");
    println!("{}", add_response);
    println!("📝 END ADD RESPONSE");
    
    // Verify files were added successfully
    assert!(add_response.contains("Added 2 path(s) to context"), "Missing success message for adding files");
    println!("✅ Files added to context successfully");
    
    // Execute /context show to confirm files are present
    let show_response = chat.execute_command("/context show")?;
    
    println!("📝 Context show response: {} bytes", show_response.len());
    println!("📝 SHOW RESPONSE:");
    println!("{}", show_response);
    println!("📝 END SHOW RESPONSE");
    
    // Verify files are present in context
    assert!(show_response.contains(test_file1_path), "Python file not found in context show output");
    assert!(show_response.contains(test_file2_path), "JavaScript file not found in context show output");
    println!("✅ Files confirmed present in context");
    
    // Execute /context clear to remove all files
    let clear_response = chat.execute_command("/context clear")?;
    
    println!("📝 Context clear response: {} bytes", clear_response.len());
    println!("📝 CLEAR RESPONSE:");
    println!("{}", clear_response);
    println!("📝 END CLEAR RESPONSE");
    
    // Verify context was cleared successfully
    assert!(clear_response.contains("Cleared context"), "Missing success message for clearing context");
    println!("✅ Context cleared successfully");
    
    // Execute /context show to confirm no files remain
    let final_show_response = chat.execute_command("/context show")?;
    
    println!("📝 Final context show response: {} bytes", final_show_response.len());
    println!("📝 FINAL SHOW RESPONSE:");
    println!("{}", final_show_response);
    println!("📝 END FINAL SHOW RESPONSE");
    
    // Verify no files remain in context
    assert!(!final_show_response.contains(test_file1_path), "Python file still found in context after clear");
    assert!(!final_show_response.contains(test_file2_path), "JavaScript file still found in context after clear");
    assert!(final_show_response.contains("👤 Agent (q_cli_default):"), "Missing Agent section");
    assert!(final_show_response.contains("💬 Session (temporary):"), "Missing Session section");
    assert!(final_show_response.contains("<none>"), "Missing <none> indicator for cleared context");
    println!("✅ All files confirmed removed from context and <none> sections present");
    
    chat.quit()?;
    
    // Clean up test files
    let _ = std::fs::remove_file(test_file1_path);
    let _ = std::fs::remove_file(test_file2_path);
    println!("✅ Cleaned up test files");
    
    println!("✅ Test completed successfully");
    
    Ok(())
}
