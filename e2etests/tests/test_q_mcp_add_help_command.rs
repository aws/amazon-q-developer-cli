use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "mcp")]
fn test_q_mcp_add_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing q mcp add --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // Execute mcp add --help command
    println!("🔍 Executing command: 'q mcp add --help'");
    let response = chat.execute_command("q mcp add --help")?;
    
    println!("📝 Restart response: {} bytes", response.len());
    println!("📝 RESTART RESPONSE:");
    println!("{}", response);
    println!("📝 END RESTART RESPONSE");
    
    // Verify tool execution details
    assert!(response.contains("I will run the following shell command:") && response.contains("q mcp add --help"), "Missing command execution description");
    assert!(response.contains("Purpose:") && response.contains("Get help information for the q mcp add command"), "Missing purpose description");
    println!("✅ Found tool execution details");

    // Verify tool execution prompt appears
    assert!(response.contains("🛠️  Using tool: execute_bash"), "Missing tool execution indicator");
    assert!(response.contains("Allow this action?") && response.contains("to trust (always allow) this tool for the session."), "Missing permission prompt");
    println!("✅ Found tool execution permission prompt");
    
    // Allow the tool execution
    let allow_response = chat.execute_command("y")?;
    
    println!("📝 Allow response: {} bytes", allow_response.len());
    println!("📝 ALLOW RESPONSE:");
    println!("{}", allow_response);
    println!("📝 END ALLOW RESPONSE");
    
    // Verify mcp add help output
    assert!(allow_response.contains("Add or replace a configured server"), "Missing command description");
    assert!(allow_response.contains("Usage: qchat mcp add"), "Missing usage information");
    assert!(allow_response.contains("--name <NAME>"), "Missing --name option");
    assert!(allow_response.contains("--command <COMMAND>"), "Missing --command option");
    assert!(allow_response.contains("--scope <SCOPE>"), "Missing --scope option");
    assert!(allow_response.contains("--args <ARGS>"), "Missing --args option");
    assert!(allow_response.contains("--agent <AGENT>"), "Missing --agent option");
    assert!(allow_response.contains("--env <ENV>"), "Missing --env option");
    assert!(allow_response.contains("--timeout <TIMEOUT>"), "Missing --timeout option");
    assert!(allow_response.contains("--disabled"), "Missing --disabled option");
    assert!(allow_response.contains("--force"), "Missing --force option");
    assert!(allow_response.contains("--verbose") && allow_response.contains("Increase logging verbosity"), "Missing --verbose option");
    assert!(allow_response.contains("--help") && allow_response.contains("Print help"), "Missing --help option");
    assert!(allow_response.contains("Completed in"), "Missing completion indicator");
    assert!(allow_response.contains("Required:"), "Missing Requried indicator");
    assert!(allow_response.contains("Optional:"), "Missing Optional indicator");
    println!("✅ MCP add help command executed successfully");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}