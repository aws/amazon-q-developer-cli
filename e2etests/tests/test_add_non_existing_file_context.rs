use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "context")]
fn test_add_non_existing_file_context() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context add non-existing file command...");
    
    let non_existing_file_path = "/tmp/non_existing_file.py";
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // Try to add non-existing file to context
    let add_response = chat.execute_command(&format!("/context add {}", non_existing_file_path))?;
    
    println!("📝 Context add response: {} bytes", add_response.len());
    println!("📝 ADD RESPONSE:");
    println!("{}", add_response);
    println!("📝 END ADD RESPONSE");
    
    // Verify error message for non-existing file
    assert!(add_response.contains("Error:") && add_response.contains("Invalid path") && add_response.contains("does not exist"), "Missing error message for non-existing file");
    assert!(add_response.contains("Use --force to add anyway"), "Missing --force suggestion in error message");
    println!("✅ Found expected error message for non-existing file with --force suggestion");
    
    chat.quit()?;
    
    println!("✅ Test completed successfully");
    
    Ok(())
}
