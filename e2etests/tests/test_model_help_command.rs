use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
fn test_model_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /model --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/model --help")?;
    
    println!("📝 Model help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("Select a model for the current conversation session"), "Missing model selection description");
    println!("✅ Found model selection description");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/model"), "Missing /model command in usage section");
    println!("✅ Found Usage section with /model command");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    assert!(response.contains("Print help"), "Missing Print help description");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All model help content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}