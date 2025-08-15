use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "tools")]
fn test_tools_reset_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools reset command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // Execute reset command
    let reset_response = chat.execute_command("/tools reset")?;
    
    println!("📝 Reset response: {} bytes", reset_response.len());
    println!("📝 RESET OUTPUT:");
    println!("{}", reset_response);
    println!("📝 END RESET OUTPUT");
    
    // Verify reset confirmation message
    assert!(reset_response.contains("Reset all tools to the permission levels as defined in agent."), "Missing reset confirmation message");
    println!("✅ Found reset confirmation message");
    
    // Now check tools list to verify tools have mixed permissions
    let tools_response = chat.execute_command("/tools")?;
    
    println!("📝 Tools response after reset: {} bytes", tools_response.len());
    println!("📝 TOOLS OUTPUT:");
    println!("{}", tools_response);
    println!("📝 END TOOLS OUTPUT");
    
    // Verify that tools have all permission types
    assert!(tools_response.contains("trusted"), "Missing trusted tools");
    assert!(tools_response.contains("not trusted"), "Missing not trusted tools");
    assert!(tools_response.contains("read-only commands"), "Missing read-only commands tools");
    println!("✅ Found all permission types after reset");
    
    println!("✅ All tools reset functionality verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}