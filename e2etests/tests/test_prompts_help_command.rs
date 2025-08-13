use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "ai_prompts")]
fn test_prompts_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /prompts --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/prompts --help")?;
    
    println!("📝 Prompts help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("Prompts are reusable templates that help you quickly access common workflows and tasks"), "Missing prompts description");
    assert!(response.contains("These templates are provided by the mcp servers you have installed and configured"), "Missing MCP servers description");
    println!("✅ Found prompts description");
    
    // Verify usage examples
    assert!(response.contains("@") && response.contains("<prompt name> [arg]") && response.contains("[arg]"), "Missing @ syntax example");
    assert!(response.contains("Retrieve prompt specified"), "Missing retrieve description");
    assert!(response.contains("/prompts") && response.contains("get") && response.contains("<prompt name>") && response.contains("[arg]"), "Missing long form example");
    println!("✅ Found usage examples with @ syntax and long form");
    
    // Verify main description
    assert!(response.contains("View and retrieve prompts"), "Missing main description");
    println!("✅ Found main description");
    
    // Verify Usage section
    assert!(response.contains("Usage:") && response.contains("/prompts") && response.contains("[COMMAND]"), "Missing usage format");
    println!("✅ Found usage format");
    
    // Verify Commands section
    assert!(response.contains("Commands:"), "Missing Commands section");
    assert!(response.contains("list"), "Missing list command");
    assert!(response.contains("get"), "Missing get command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Found all commands: list, get, help");
    
    // Verify command descriptions
    assert!(response.contains("List available prompts from a tool or show all available prompt"), "Missing list description");
    println!("✅ Found command descriptions");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h") && response.contains("--help"), "Missing help flags");
    println!("✅ Found Options section with help flags");
    
    println!("✅ All prompts help content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}