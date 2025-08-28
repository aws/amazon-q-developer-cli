#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(all(feature = "help", feature = "sanity"))]
fn test_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /help command... | Description: Tests the /help command to display all available commands and verify core functionality like quit, clear, tools, and help commands are present");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/help")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify help content
    assert!(response.contains("Commands:"), "Missing Commands section");
    println!("✅ Found Commands section with all available commands");
    
    assert!(response.contains("quit"), "Missing quit command");
    assert!(response.contains("clear"), "Missing clear command");
    assert!(response.contains("tools"), "Missing tools command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Verified core commands: quit, clear, tools, help");
    
    // Verify specific useful commands
    if response.contains("context") {
        println!("✅ Found context management command");
    }
    if response.contains("agent") {
        println!("✅ Found agent management command");
    }
    if response.contains("model") {
        println!("✅ Found model selection command");
    }
    
    println!("✅ All help content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
