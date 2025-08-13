use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "save_load")]
fn test_load_command_argument_validation() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /load command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/load")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify load error message
    assert!(response.contains("error:") && response.contains("the following required arguments were not provided:"), "Missing load error message");
    println!("✅ Found load error message");
    
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/load"), "Missing /load command in usage");
    println!("✅ Found Usage section with /load command");
    
    assert!(response.contains("Arguments:"), "Missing Arguments section");
    assert!(response.contains("<PATH>"), "Missing PATH argument");
    println!("✅ Found Arguments section with PATH parameter");
    
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h"), "Missing -h flag");
    assert!(response.contains("--help") || response.contains("—help"), "Missing --help flag");
    println!("✅ Found Options section with -h, --help flags");
    
    assert!(response.contains("Print help"), "Missing help description");
    println!("✅ Found help flag description");
    
    println!("✅ All help content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
