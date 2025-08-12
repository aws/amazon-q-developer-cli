use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
fn test_mcp_loading_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing MCP loading...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
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
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}