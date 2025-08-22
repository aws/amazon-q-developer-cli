#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "core_session")]
fn test_quit_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /quit command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    chat.quit()?;
    println!("✅ /quit command executed successfully");
    println!("✅ Test completed successfully");
    
    Ok(())
}
