use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "mcp")]
fn test_mcp_remove_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing q mcp remove --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // Execute q mcp remove --help command
    let help_response = chat.execute_command("q mcp remove --help")?;
    
    println!("📝 MCP remove help response: {} bytes", help_response.len());
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
    
    // Verify complete help content in final response
    assert!(allow_response.contains("Remove a server from the MCP configuration"), "Missing MCP remove description");
    assert!(allow_response.contains("Usage: qchat mcp remove"), "Missing usage information");
    assert!(allow_response.contains("--name <NAME>"), "Missing --name option");
    assert!(allow_response.contains("--scope <SCOPE>"), "Missing --scope option");
    assert!(allow_response.contains("--agent <AGENT>"), "Missing --agent option");
    assert!(allow_response.contains("-h, --help"), "Missing help option");
    assert!(allow_response.contains("Completed in"), "Missing completion indicator");
    println!("✅ Found all expected MCP remove help content and completion");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
