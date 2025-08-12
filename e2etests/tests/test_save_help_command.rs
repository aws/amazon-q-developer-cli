use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
fn test_save_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /save --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/save --help")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify save command help content
    assert!(response.contains("Save the current conversation"), "Missing save command description");
    println!("✅ Found save command description");
    
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/save"), "Missing /save command in usage");
    println!("✅ Found Usage section with /save command");
    
    assert!(response.contains("Arguments:"), "Missing Arguments section");
    assert!(response.contains("<PATH>"), "Missing PATH argument");
    println!("✅ Found Arguments section with PATH parameter");
    
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-f"), "Missing -f flag");
    assert!(response.contains("--force"), "Missing --force flag");
    assert!(response.contains("-h"), "Missing -h flag");
    assert!(response.contains("--help") || response.contains("—help"), "Missing --help flag");
    println!("✅ Found Options section with -f, --force, -h, --help flags");
    
    assert!(response.contains("Print help"), "Missing help description");
    println!("✅ Found help flag description");
    
    println!("✅ All help content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
