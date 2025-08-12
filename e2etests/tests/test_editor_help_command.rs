use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
fn test_editor_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /editor --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/editor --help")?;
    
    println!("📝 Editor help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("Open $EDITOR (defaults to vi) to compose a prompt"), "Missing editor description");
    println!("✅ Found editor description");
    
    // Verify Usage section
    assert!(response.contains("Usage:") && response.contains("/editor") && response.contains("[INITIAL_TEXT]"), "Missing Usage section");
    println!("✅ Found Usage section with /editor command");
    
    // Verify Arguments section
    assert!(response.contains("Arguments:"), "Missing Arguments section");
    assert!(response.contains("[INITIAL_TEXT]..."), "Missing INITIAL_TEXT argument");
    println!("✅ Found Arguments section");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    assert!(response.contains("Print help"), "Missing Print help description");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All editor help content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}

#[test]
fn test_help_editor_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /help editor command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/help editor")?;
    
    println!("📝 Help editor response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("Open $EDITOR (defaults to vi) to compose a prompt"), "Missing editor description");
    println!("✅ Found editor description");
    
    // Verify Usage section
    assert!(response.contains("Usage:") && response.contains("/editor") && response.contains("[INITIAL_TEXT]"), "Missing Usage section");
    println!("✅ Found Usage section with /editor command");
    
    // Verify Arguments section
    assert!(response.contains("Arguments:"), "Missing Arguments section");
    assert!(response.contains("[INITIAL_TEXT]..."), "Missing INITIAL_TEXT argument");
    println!("✅ Found Arguments section");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All help editor content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}