use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "tools")]
fn test_tools_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/tools --help")?;
    
    println!("📝 Tools help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("By default, Amazon Q will ask for your permission to use certain tools."), "Missing permission description");
    println!("✅ Found tools permission description");
    
    // Verify documentation reference
    assert!(response.contains("Refer to the documentation for how to configure tools with your agent"), "Missing documentation reference");
    assert!(response.contains("https://github.com/aws/amazon-q-developer-cli/blob/main/docs/agent-format.md#tools-field"), "Missing documentation URL");
    println!("✅ Found documentation reference and URL");
    
    // Verify main description
    assert!(response.contains("View tools and permissions"), "Missing main description");
    println!("✅ Found main description");
    
    // Verify Usage section
    //assert!(response.contains("Usage: /tools [COMMAND]"), "Missing usage format");
    assert!(response.contains("Usage:") && response.contains("/tools") && response.contains("[COMMAND]"), "Missing Usage section");
    println!("✅ Found usage format");
    println!("✅ Found usage format");
    
    // Verify Commands section
    assert!(response.contains("Commands:"), "Missing Commands section");
    assert!(response.contains("schema"), "Missing schema command");
    assert!(response.contains("trust"), "Missing trust command");
    assert!(response.contains("untrust"), "Missing untrust command");
    assert!(response.contains("trust-all"), "Missing trust-all command");
    assert!(response.contains("reset"), "Missing reset command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Found all commands: schema, trust, untrust, trust-all, reset, help");
    
    // Verify command descriptions
    assert!(response.contains("Show the input schema for all available tools"), "Missing schema description");
    assert!(response.contains("Trust a specific tool or tools for the session"), "Missing trust description");
    assert!(response.contains("Revert a tool or tools to per-request confirmation"), "Missing untrust description");
    assert!(response.contains("Trust all tools (equivalent to deprecated /acceptall)"), "Missing trust-all description");
    assert!(response.contains("Reset all tools to default permission levels"), "Missing reset description");
    println!("✅ Found all command descriptions");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found Options section with help flags");
    
    println!("✅ All tools help content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}