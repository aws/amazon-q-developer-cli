use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "session_mgmt")]
fn test_compact_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /compact command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/compact")?;
    
    println!("📝 Compact response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify compact response - either success or too short
    if response.contains("Conversation history has been compacted successfully!") {
        println!("✅ Found compact success message");
    } else if response.contains("Conversation too short to compact.") {
        println!("✅ Found conversation too short message");
    } else {
        panic!("Missing expected compact response");
    }
    
    println!("✅ All compact content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}