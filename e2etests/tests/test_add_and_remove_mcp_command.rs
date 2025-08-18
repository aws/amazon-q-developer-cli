use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "mcp")]
fn test_add_and_remove_mcp_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing q mcp add command...");
    
    // First install uv dependency before starting Q Chat
    println!("🔍 Installing uv dependency...");
    std::process::Command::new("pip3")
        .args(["install", "uv", "--break-system-packages"])
        .output()
        .expect("Failed to install uv");
    println!("✅ uv dependency installed");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // Execute mcp add command
    println!("🔍 Executing command: 'q mcp add --name aws-documentation --command uvx --args awslabs.aws-documentation-mcp-server@latest'");
    let response = chat.execute_command("q mcp add --name aws-documentation --command uvx --args awslabs.aws-documentation-mcp-server@latest")?;
    
    println!("📝 Response: {} bytes", response.len());
    println!("📝 RESPONSE:");
    println!("{}", response);
    println!("📝 END RESPONSE");

    // Verify tool execution details
    assert!(response.contains("I will run the following shell command:"), "Missing command execution description");
    assert!(response.contains("q mcp add --name aws-documentation --command uvx --args awslabs.aws-documentation-mcp-server@latest"), "Missing full command");
    assert!(response.contains("Purpose:") && response.contains("Add AWS documentation MCP server"), "Missing purpose description");
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
    
    // Verify successful addition
    assert!(allow_response.contains("✓ Added MCP server") && allow_response.contains("'aws-documentation'") && allow_response.contains("to global config in"), "Missing success message");
    assert!(allow_response.contains("/Users/") && allow_response.contains("/.aws/amazonq/mcp.json"), "Missing config file path");
    println!("✅ Found successful addition message");
    
    // Verify completion indicator
    assert!(allow_response.contains("Completed in") && allow_response.contains("s"), "Missing completion time indicator");
    println!("✅ Found completion indicator");
    
    println!("✅ All q mcp add command execution verified successfully");
    
    // Now test removing the MCP server
    println!("🔍 Executing remove command: 'q mcp remove --name aws-documentation'");
    let remove_response = chat.execute_command("q mcp remove --name aws-documentation")?;
    
    println!("📝 Remove response: {} bytes", remove_response.len());
    println!("📝 REMOVE RESPONSE:");
    println!("{}", remove_response);
    println!("📝 END REMOVE RESPONSE");
    
    // Verify remove tool execution details
    assert!(remove_response.contains("I will run the following shell command:"), "Missing remove command execution description");
    assert!(remove_response.contains("q mcp remove --name aws-documentation"), "Missing full remove command");
    println!("✅ Found remove tool execution details");
    
    // Verify remove tool execution prompt
    assert!(remove_response.contains("🛠️  Using tool: execute_bash"), "Missing remove tool execution indicator");
    assert!(remove_response.contains("Allow this action?"), "Missing remove permission prompt");
    println!("✅ Found remove tool execution permission prompt");
    
    // Allow the remove tool execution
    let remove_allow_response = chat.execute_command("y")?;
    
    println!("📝 Remove allow response: {} bytes", remove_allow_response.len());
    println!("📝 REMOVE ALLOW RESPONSE:");
    println!("{}", remove_allow_response);
    println!("📝 END REMOVE ALLOW RESPONSE");
    
    // Verify successful removal
    assert!(remove_allow_response.contains("✓ Removed MCP server") && remove_allow_response.contains("'aws-documentation'") && remove_allow_response.contains("from global config"), "Missing removal success message");
    assert!(remove_allow_response.contains("/Users/") && remove_allow_response.contains("/.aws/amazonq/mcp.json"), "Missing config file path in removal");
    println!("✅ Found successful removal message");
    
    // Verify remove completion indicator
    assert!(remove_allow_response.contains("Completed in") && remove_allow_response.contains("s"), "Missing remove completion time indicator");
    println!("✅ Found remove completion indicator");
    
    println!("✅ All q mcp remove command execution verified successfully");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}