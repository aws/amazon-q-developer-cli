use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "ai_prompts1")]
fn test_prompts_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /prompts command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/prompts list")?;
    
    println!("📝 Prompts command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify usage instruction
    assert!(response.contains("Usage:") && response.contains("@") && response.contains("<prompt name>") && response.contains("[...args]"), "Missing usage instruction");
    println!("✅ Found usage instruction");
    
    // Verify table headers
    assert!(response.contains("Prompt"), "Missing Prompt header");
    assert!(response.contains("Arguments") && response.contains("*") && response.contains("required"), "Missing Arguments header");
    println!("✅ Found table headers with required notation");
    
    // Verify command executed successfully
    assert!(!response.is_empty(), "Empty response from prompts command");
    println!("✅ Command executed with response");
    
    println!("✅ All prompts command functionality verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}