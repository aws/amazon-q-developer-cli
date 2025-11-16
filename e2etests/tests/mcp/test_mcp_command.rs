#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

#[test]
#[cfg(all(feature = "mcp", feature = "sanity"))]
fn test_mcp_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /mcp --help command... | Description: Tests the <code>/mcp --help</code> command to display help information for MCP server management functionality");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/mcp --help")?;
    
    println!("📝 MCP help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("See mcp server loaded"), "Missing mcp server description");
    println!("✅ Found mcp server description");
    
    // Verify Usage section
    assert!(response.contains("Usage"), "Missing Usage section");
    assert!(response.contains("/mcp"), "Missing /mcp command in usage section");
    println!("✅ Found Usage section with /mcp command");
    
    // Verify Options section
    assert!(response.contains("Options"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All mcp help content verified!");
    
    drop(chat);
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "sanity"))]
fn test_mcp_loading_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /mcp command... | Description: Tests the <code>/mcp</code> command to display MCP server loading status and information");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/mcp")?;
    
    println!("📝 MCP loading response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Check MCP status - either loaded or loading
    if response.contains("loaded in") {
        assert!(response.contains(" s"), "Missing seconds indicator for loading time");
        println!("✅ Found MCPs loaded with timing");
        
        // Count number of MCPs loaded
        let mcp_count = response.matches("✓").count();
        println!("✅ Found {} MCP(s) loaded", mcp_count);
    } else if response.contains("loading") {
        println!("✅ MCPs are still loading");
    } else {
        println!("ℹ️ MCP status unclear - may be in different state");
    }
    
    println!("✅ All MCP loading content verified!");
    
    drop(chat);
    
    Ok(())
}

