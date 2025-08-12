use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
fn test_subscribe_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /subscribe --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/subscribe --help")?;
    
    println!("📝 Subscribe help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("Upgrade to a Q Developer Pro subscription for increased query limits"), "Missing subscription description");
    println!("✅ Found subscription description");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/subscribe"), "Missing /subscribe command in usage section");
    assert!(response.contains("[OPTIONS]"), "Missing [OPTIONS] in usage section");
    println!("✅ Found Usage section with /subscribe [OPTIONS]");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify manage option
    assert!(response.contains("--manage"), "Missing --manage option");
    println!("✅ Found --manage option");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    assert!(response.contains("Print help"), "Missing Print help description");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All subscribe help content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}