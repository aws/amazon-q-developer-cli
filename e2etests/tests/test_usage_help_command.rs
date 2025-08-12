use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
fn test_usage_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /usage --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/usage --help")?;
    
    println!("📝 Usage help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("Show current session's context window usage"), "Missing usage command description");
    println!("✅ Found usage command description");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/usage"), "Missing /usage command in usage section");
    println!("✅ Found Usage section with /usage command");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with description");
    
    println!("✅ All usage help content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}