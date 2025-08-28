#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(all(feature = "quit", feature = "sanity"))]
fn test_quit_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /quit command... | Description: Tests the /quit command to properly terminate the Q Chat session and exit cleanly");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    chat.quit()?;
    println!("✅ /quit command executed successfully");
    println!("✅ Test completed successfully");
    
    Ok(())
}
