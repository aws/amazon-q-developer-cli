use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "context")]
fn test_remove_multiple_file_context() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context remove <filename1> <filename2> command...");
    
    let test_file1_path = "/tmp/test_context_file1.py";
    let test_file2_path = "/tmp/test_context_file2.js";
    let test_file3_path = "/tmp/test_context_file3.txt";
    
    // Create multiple test files
    std::fs::write(test_file1_path, "# Test Python file for context\nprint('Hello from Python file')")?;
    std::fs::write(test_file2_path, "// Test JavaScript file for context\nconsole.log('Hello from JS file');")?;
    std::fs::write(test_file3_path, "Test text file for context\nHello from text file")?;
    println!("✅ Created test files at {}, {}, {}", test_file1_path, test_file2_path, test_file3_path);
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // Add multiple files to context
    let add_response = chat.execute_command(&format!("/context add {} {} {}", test_file1_path, test_file2_path, test_file3_path))?;
    
    println!("📝 Context add response: {} bytes", add_response.len());
    println!("📝 ADD RESPONSE:");
    println!("{}", add_response);
    println!("📝 END ADD RESPONSE");
    
    // Verify files were added successfully
    assert!(add_response.contains("Added 3 path(s) to context"), "Missing success message for adding multiple files");
    println!("✅ Multiple files added to context successfully");
    
    // Execute /context show to confirm files are present
    let show_response = chat.execute_command("/context show")?;
    
    println!("📝 Context show response: {} bytes", show_response.len());
    println!("📝 SHOW RESPONSE:");
    println!("{}", show_response);
    println!("📝 END SHOW RESPONSE");
    
    // Verify all files are present in context
    assert!(show_response.contains(test_file1_path), "Python file not found in context show output");
    assert!(show_response.contains(test_file2_path), "JavaScript file not found in context show output");
    assert!(show_response.contains(test_file3_path), "Text file not found in context show output");
    println!("✅ All files confirmed present in context");
    
    // Remove multiple files from context
    let remove_response = chat.execute_command(&format!("/context remove {} {} {}", test_file1_path, test_file2_path, test_file3_path))?;
    
    println!("📝 Context remove response: {} bytes", remove_response.len());
    println!("📝 REMOVE RESPONSE:");
    println!("{}", remove_response);
    println!("📝 END REMOVE RESPONSE");
    
    // Verify files were removed successfully
    assert!(remove_response.contains("Removed 3 path(s) from context"), "Missing success message for removing multiple files");
    println!("✅ Multiple files removed from context successfully");
    
    // Execute /context show to confirm files are gone
    let final_show_response = chat.execute_command("/context show")?;
    
    println!("📝 Final context show response: {} bytes", final_show_response.len());
    println!("📝 FINAL SHOW RESPONSE:");
    println!("{}", final_show_response);
    println!("📝 END FINAL SHOW RESPONSE");
    
    // Verify files are no longer in context
    assert!(!final_show_response.contains(test_file1_path), "Python file still found in context after removal");
    assert!(!final_show_response.contains(test_file2_path), "JavaScript file still found in context after removal");
    assert!(!final_show_response.contains(test_file3_path), "Text file still found in context after removal");
    println!("✅ All files confirmed removed from context");
    
    chat.quit()?;
    
    // Clean up test files
    let _ = std::fs::remove_file(test_file1_path);
    let _ = std::fs::remove_file(test_file2_path);
    let _ = std::fs::remove_file(test_file3_path);
    println!("✅ Cleaned up test files");
    
    println!("✅ Test completed successfully");
    
    Ok(())
}
