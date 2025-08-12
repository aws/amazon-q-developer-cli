use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "save_load")]
fn test_load_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /load --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/load --help")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify load command help content
    assert!(response.contains("Load a previous conversation"), "Missing load command description");
    println!("✅ Found load command description");
    
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/load"), "Missing /load command in usage");
    println!("✅ Found Usage section with /load command");
    
    assert!(response.contains("Arguments:"), "Missing Arguments section");
    assert!(response.contains("<PATH>"), "Missing PATH argument");
    println!("✅ Found Arguments section with PATH parameter");
    
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h"), "Missing -h flag");
    assert!(response.contains("--help"), "Missing --help flag");
    println!("✅ Found Options section with -h, --help flags");
    
    assert!(response.contains("Print help"), "Missing help description");
    println!("✅ Found help flag description");
    
    println!("✅ All help content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
