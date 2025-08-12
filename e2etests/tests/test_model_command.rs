use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "model")]
fn test_model_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /model command interface and selection...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/model")?;
    
    println!("📝 Model response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify model selection header
    assert!(response.contains("Select a model for this chat session"), "Missing model selection header");
    println!("✅ Found model selection header");
    
    // Verify required models
    assert!(response.contains("claude-4-sonnet"), "Missing claude-4-sonnet model");
    assert!(response.contains("claude-3.7-sonnet"), "Missing claude-3.7-sonnet model");
    println!("✅ Found required models: claude-4-sonnet, claude-3.7-sonnet");
    
    // Verify active model indicator
    assert!(response.contains("(active)"), "Missing active model indicator");
    println!("✅ Found active model indicator");
    
    // Verify cursor selection indicator
    assert!(response.contains("❯"), "Missing cursor selection indicator");
    println!("✅ Found cursor selection indicator ❯");
    
    println!("✅ All model selection functionality verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
