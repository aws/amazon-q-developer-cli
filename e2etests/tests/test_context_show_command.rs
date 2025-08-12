use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
fn test_context_show_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context show command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/context show")?;
    
    println!("📝 Context show response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify context show output contains expected sections
    assert!(response.contains("👤 Agent"), "Missing Agent section with emoji");
    println!("✅ Found Agent section with emoji");
    
    // Verify agent configuration details
    assert!(response.contains("AmazonQ.md"), "Missing AmazonQ.md in agent config");
    assert!(response.contains("README.md"), "Missing README.md in agent config");
    assert!(response.contains(".amazonq/rules/**/*.md"), "Missing .amazonq/rules pattern");
    println!("✅ Found all expected agent configuration files");
    
    println!("✅ All context show content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
