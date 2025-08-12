use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
fn test_subscribe_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /subscribe command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/subscribe")?;
    
    println!("📝 Subscribe response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify subscription management message
    assert!(response.contains("Your Q Developer Pro subscription is managed through IAM Identity Center"), "Missing subscription management message");
    println!("✅ Found subscription management message");
    
    println!("✅ All subscribe content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}