#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

/// Tests the q chat command startup and /help functionality
#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_chat_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q chat command... | Description: Tests the q chat command that opens Q terminal for interactive AI conversations.");
    
    println!("\n🛠️ Running 'q chat' command...");
    let response = q_chat_helper::execute_q_subcommand("q", &["chat", "\"what is aws?\""])?;

    println!("📝 Chat response: {} bytes", response.len());
    println!("📝 CHAT OUTPUT:");
    println!("{}", response);
    println!("📝 END CHAT OUTPUT");

    // Validate we got a proper AWS response
    assert!(response.contains("Amazon Web Services") || response.contains("AWS"), 
            "Response should contain AWS information");
    assert!(response.len() > 100, "Response should be substantial");
    
    println!("✅ Got substantial AI response ({} bytes)!", response.len());

    println!("✅ Chat command executed!");
    
    println!("✅ q chat command executed successfully!");
    
    Ok(())
}