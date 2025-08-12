use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
fn test_mcp_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /mcp --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/mcp --help")?;
    
    println!("📝 MCP help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("See mcp server loaded"), "Missing mcp server description");
    println!("✅ Found mcp server description");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/mcp"), "Missing /mcp command in usage section");
    println!("✅ Found Usage section with /mcp command");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    assert!(response.contains("Print help"), "Missing Print help description");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All mcp help content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}