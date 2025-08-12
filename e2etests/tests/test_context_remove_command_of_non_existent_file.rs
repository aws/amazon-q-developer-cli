use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "context")]
fn test_context_remove_command_of_non_existent_file() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context remove non existing file command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/context remove non_existent_file.txt")?;
    
    println!("📝 Context remove response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify error message for non-existent file
    assert!(response.contains("None of the specified paths were found in the context"), "Missing error message for non-existent file");
    println!("✅ Found expected error message for non-existent file removal");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
