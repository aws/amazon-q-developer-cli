#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

#[test]
#[cfg(all(feature = "quit", feature = "sanity"))]
fn test_quit_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /quit command... | Description: Tests the <code> /quit</code> command to properly terminate the Q Chat session and exit cleanly");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap();

    println!("✅ Q Chat session started");
    
    chat.execute_command_with_timeout("/quit",Some(1000))?;
    
    println!("✅ /quit command executed successfully");
    println!("✅ Test completed successfully");

    Ok(())
}
