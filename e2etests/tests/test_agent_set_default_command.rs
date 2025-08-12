use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "agent")]
fn test_agent_set_default_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /agent set-default with valid arguments...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/agent set-default -n q_cli_default")?;
    
    println!("📝 Agent set-default command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Collect all failures instead of stopping at first one
    let mut failures = Vec::new();
    
    if !response.contains("✓") { failures.push("Missing success checkmark"); }
    if !response.contains("Default agent set to") { failures.push("Missing success message"); }
    if !response.contains("q_cli_default") { failures.push("Missing agent name"); }
    if !response.contains("This will take effect") { failures.push("Missing effect message"); }
    if !response.contains("next time q chat is launched") { failures.push("Missing launch message"); }
    
    // Report all failures at once
    if !failures.is_empty() {
        panic!("Test failures: {}", failures.join(", "));
    }
    
    println!("✅ All expected success messages found");
    
    println!("✅ /agent set-default executed successfully with valid arguments");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}