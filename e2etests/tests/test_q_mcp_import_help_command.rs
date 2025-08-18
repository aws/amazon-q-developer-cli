use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "mcp")]
fn test_q_mcp_import_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing q mcp import --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // Execute mcp import --help command
    println!("🔍 Executing command: 'q mcp import --help'");
    let response = chat.execute_command("q mcp import --help")?;
    
    println!("📝 Restart response: {} bytes", response.len());
    println!("📝 RESTART RESPONSE:");
    println!("{}", response);
    println!("📝 END RESTART RESPONSE");

    // Verify tool execution details
    assert!(response.contains("I will run the following shell command:") && response.contains("q mcp import --help"), "Missing command execution description");
    assert!(response.contains("Purpose:") && response.contains("Get help information for the q mcp import command"), "Missing purpose description");
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
    
    // Verify command description
    assert!(allow_response.contains("Import a server configuration from another file"), "Missing command description");
    println!("✅ Found command description");
    
    // Verify usage line
    assert!(allow_response.contains("Usage: qchat mcp import [OPTIONS] --file <FILE> [SCOPE]"), "Missing complete usage line");
    println!("✅ Found usage information");
    
    // Verify Arguments section
    assert!(allow_response.contains("Arguments:"), "Missing Arguments section");
    assert!(allow_response.contains("[SCOPE]") && allow_response.contains("[possible values: default, workspace, global]"), "Missing SCOPE argument with possible values");
    println!("✅ Found Arguments section with SCOPE");
    
    // Verify Options section
    assert!(allow_response.contains("Options:"), "Missing Options section");
    assert!(allow_response.contains("--file <FILE>"), "Missing --file option");
    assert!(allow_response.contains("--force") && allow_response.contains("Overwrite an existing server with the same name"), "Missing --force option");
    assert!(allow_response.contains("-v, --verbose...") && allow_response.contains("Increase logging verbosity"), "Missing --verbose option");
    assert!(allow_response.contains("-h, --help") && allow_response.contains("Print help"), "Missing --help option");
    println!("✅ Found all options with descriptions");
    
    // Verify completion indicator
    assert!(allow_response.contains("Completed in") && allow_response.contains("s"), "Missing completion time indicator");
    println!("✅ Found completion indicator");
    
    println!("✅ All q mcp import --help content verified successfully");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}