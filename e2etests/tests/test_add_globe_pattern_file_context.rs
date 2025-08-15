use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "context")]
fn test_add_glob_pattern_file_context() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context add *.py glob pattern command...");
    
    let test_file1_path = "/tmp/test_context_file1.py";
    let test_file2_path = "/tmp/test_context_file2.py";
    let test_file3_path = "/tmp/test_context_file.js"; // Non-matching file
    let glob_pattern = "/tmp/*.py";
    
    // Create test files
    std::fs::write(test_file1_path, "# Test Python file 1 for context\nprint('Hello from Python file 1')")?;
    std::fs::write(test_file2_path, "# Test Python file 2 for context\nprint('Hello from Python file 2')")?;
    std::fs::write(test_file3_path, "// Test JavaScript file\nconsole.log('Hello from JS file');")?;
    println!("✅ Created test files at {}, {}, {}", test_file1_path, test_file2_path, test_file3_path);
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // Add glob pattern to context
    let add_response = chat.execute_command(&format!("/context add {}", glob_pattern))?;
    
    println!("📝 Context add response: {} bytes", add_response.len());
    println!("📝 ADD RESPONSE:");
    println!("{}", add_response);
    println!("📝 END ADD RESPONSE");
    
    // Verify glob pattern was added successfully
    assert!(add_response.contains("Added 1 path(s) to context"), "Missing success message for adding glob pattern");
    println!("✅ Glob pattern added to context successfully");
    
    // Execute /context show to confirm pattern matches files
    let show_response = chat.execute_command("/context show")?;
    
    println!("📝 Context show response: {} bytes", show_response.len());
    println!("📝 SHOW RESPONSE:");
    println!("{}", show_response);
    println!("📝 END SHOW RESPONSE");
    
    // Verify glob pattern is present and matches files
    assert!(show_response.contains(glob_pattern), "Glob pattern not found in context show output");
    assert!(show_response.contains("match"), "Missing match indicator for glob pattern");
    assert!(show_response.contains("💬 Session (temporary):"), "Missing Session section");
    println!("✅ Glob pattern confirmed present in context with matches");
    
    chat.quit()?;
    
    // Clean up test files
    let _ = std::fs::remove_file(test_file1_path);
    let _ = std::fs::remove_file(test_file2_path);
    let _ = std::fs::remove_file(test_file3_path);
    println!("✅ Cleaned up test files");
    
    println!("✅ Test completed successfully");
    
    Ok(())
}
