use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "core_session")]
fn test_tools_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/tools")?;
    
    println!("📝 Tools response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify tools content structure
    assert!(response.contains("Tool"), "Missing Tool header");
    assert!(response.contains("Permission"), "Missing Permission header");
    println!("✅ Found tools table with Tool and Permission columns");
    
    assert!(response.contains("Built-in:"), "Missing Built-in section");
    println!("✅ Found Built-in tools section");
    
    // Verify some expected built-in tools
    assert!(response.contains("execute_bash"), "Missing execute_bash tool");
    assert!(response.contains("fs_read"), "Missing fs_read tool");
    assert!(response.contains("fs_write"), "Missing fs_write tool");
    assert!(response.contains("use_aws"), "Missing use_aws tool");
    println!("✅ Verified core built-in tools: execute_bash, fs_read, fs_write, use_aws");
    
    // Check for MCP tools section if present
    if response.contains("amzn-mcp (MCP):") {
        println!("✅ Found MCP tools section with Amazon-specific tools");
        assert!(response.contains("not trusted") || response.contains("trusted"), "Missing permission status");
        println!("✅ Verified permission status indicators (trusted/not trusted)");
        
        // Count some MCP tools
        let mcp_tools = ["andes", "cradle", "datanet", "read_quip", "taskei_get_task"];
        let found_tools: Vec<&str> = mcp_tools.iter().filter(|&&tool| response.contains(tool)).copied().collect();
        println!("✅ Found {} MCP tools including: {:?}", found_tools.len(), found_tools);
    }
    
    println!("✅ All tools content verified!");
    
    println!("✅ /tools command executed successfully");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
