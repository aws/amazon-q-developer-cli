use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "mcp")]
fn test_q_mcp_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing q mcp --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // Execute q mcp --help command
    let help_response = chat.execute_command("q mcp --help")?;
    
    println!("📝 MCP help response: {} bytes", help_response.len());
    println!("📝 HELP RESPONSE:");
    println!("{}", help_response);
    println!("📝 END HELP RESPONSE");
    
    // Verify tool execution prompt appears
    assert!(help_response.contains("🛠️  Using tool: execute_bash"), "Missing tool execution indicator");
    assert!(help_response.contains("Allow this action?") && help_response.contains("to trust (always allow) this tool for the session."), "Missing permission prompt");
    println!("✅ Found tool execution permission prompt");
    
    // Allow the tool execution
    let allow_response = chat.execute_command("y")?;

    println!("📝 Allow response: {} bytes", allow_response.len());
    println!("📝 ALLOW RESPONSE:");
    println!("{}", allow_response);
    println!("📝 END ALLOW RESPONSE");
    
    // Verify complete help content
    assert!(allow_response.contains("Model Context Protocol (MCP)"), "Missing MCP description");
    assert!(allow_response.contains("Usage: qchat mcp"), "Missing usage information");
    assert!(allow_response.contains("Commands:"), "Missing Commands section");
    
    // Verify command descriptions
    assert!(allow_response.contains("add") && allow_response.contains("Add or replace a configured server"), "Missing add command description");
    assert!(allow_response.contains("remove") && allow_response.contains("Remove a server from the MCP configuration"), "Missing remove command description");
    assert!(allow_response.contains("list") && allow_response.contains("List configured servers"), "Missing list command description");
    assert!(allow_response.contains("import") && allow_response.contains("Import a server configuration from another file"), "Missing import command description");
    assert!(allow_response.contains("status") && allow_response.contains("Get the status of a configured server"), "Missing status command description");
    assert!(allow_response.contains("help"), "Missing help command");
    println!("✅ Found all MCP commands with descriptions");
    
    assert!(allow_response.contains("Options:"), "Missing Options section");
    assert!(allow_response.contains("-v, --verbose"), "Missing verbose option");
    assert!(allow_response.contains("-h, --help"), "Missing help option");
    assert!(allow_response.contains("Completed in"), "Missing completion indicator");
    println!("✅ Found all expected MCP help content and completion");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
