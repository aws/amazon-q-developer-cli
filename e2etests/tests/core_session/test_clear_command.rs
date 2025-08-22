#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "core_session")]
fn test_clear_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /clear command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // Send initial message
    println!("🔍 Sending prompt: 'My name is TestUser'");
    let _initial_response = chat.send_prompt("My name is TestUser")?;
    println!("📝 Initial response: {} bytes", _initial_response.len());
    println!("📝 INITIAL RESPONSE OUTPUT:");
    println!("{}", _initial_response);
    println!("📝 END INITIAL RESPONSE");
    
    // Execute clear command
    println!("🔍 Executing command: '/clear'");
    let _clear_response = chat.execute_command("/clear")?;

    println!("✅ Clear command executed");
    
    // Check if AI remembers previous conversation
    println!("🔍 Sending prompt: 'What is my name?'");
    let test_response = chat.send_prompt("What is my name?")?;
    println!("📝 Test response: {} bytes", test_response.len());
    println!("📝 TEST RESPONSE OUTPUT:");
    println!("{}", test_response);
    println!("📝 END TEST RESPONSE");
    
    // Verify history is cleared - AI shouldn't remember the name
    assert!(!test_response.to_lowercase().contains("testuser"), "Clear command failed - AI still remembers previous conversation");
    println!("✅ Clear command successful - Conversation history cleared.");
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}