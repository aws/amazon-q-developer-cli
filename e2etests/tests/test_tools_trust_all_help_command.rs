use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "tools")]
fn test_tools_trust_all_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools trust-all --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/tools trust-all --help")?;
    
    println!("📝 Tools trust-all help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify command description
    assert!(response.contains("Trust all tools") && response.contains("equivalent to deprecated /acceptall"), "Missing command description");
    println!("✅ Found command description");
    
    // Verify usage format
    assert!(response.contains("Usage:") && response.contains("/tools trust-all"), "Missing usage format");
    println!("✅ Found usage format");
    
    // Verify options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h") && response.contains("--help") && response.contains("Print help"), "Missing help option");
    println!("✅ Found options section with help flag");
    
    println!("✅ All tools trust-all help functionality verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}