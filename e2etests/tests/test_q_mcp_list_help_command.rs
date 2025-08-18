use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "mcp")]
fn test_q_mcp_list_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing q mcp list --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("q mcp list --help")?;
    
    println!("📝 MCP list help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify tool execution prompt
    assert!(response.contains("Using tool:"), "Missing tool execution indicator");
    assert!(response.contains("q mcp list --help"), "Missing command in tool execution");
    assert!(response.contains("Allow this action?") && response.contains("to trust (always allow) this tool for the session."), "Missing permission prompt");
    println!("✅ Found tool execution permission prompt");
    
    // Allow the tool execution
    let allow_response = chat.execute_command("y")?;
    
    println!("📝 Allow response: {} bytes", allow_response.len());
    println!("📝 ALLOW RESPONSE:");
    println!("{}", allow_response);
    println!("📝 END ALLOW RESPONSE");
    
    // Verify help content
    assert!(allow_response.contains("List configured servers"), "Missing command description");
    assert!(allow_response.contains("Usage: qchat mcp list") && allow_response.contains("[OPTIONS]") && allow_response.contains("[SCOPE]"), "Missing usage format");
    
    // Verify arguments section
    assert!(allow_response.contains("Arguments:"), "Missing Arguments section");
    assert!(allow_response.contains("[SCOPE]") && allow_response.contains("possible values: default, workspace, global"), "Missing scope argument");
    
    // Verify options section
    assert!(allow_response.contains("Options:"), "Missing Options section");
    assert!(allow_response.contains("-v") && allow_response.contains("--verbose")&&  allow_response.contains("Increase logging verbosity"), "Missing verbose option");
    assert!(allow_response.contains("-h") && allow_response.contains("--help") && allow_response.contains("Print help"), "Missing help option");
    
    // Verify additional explanation content
    //assert!(allow_response.contains("The q mcp list command") && allow_response.contains("configured MCP"), "Missing command explanation");
    assert!(allow_response.contains("default") && allow_response.contains("workspace") && allow_response.contains("global"), "Missing scope explanation");
    assert!(allow_response.contains("-v") && allow_response.contains("--verbose"),"Missing verbose explanation");

    
    assert!(allow_response.contains("Completed in"), "Missing completion indicator");
    println!("✅ Found all MCP list help content with explanations and completion");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}