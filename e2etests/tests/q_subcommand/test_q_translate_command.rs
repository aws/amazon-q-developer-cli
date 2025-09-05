#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_translate_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q translate command... | Description: Tests the q translate command for Natural Language to Shell translation");
    
    println!("\n🛠️ Running 'q translate' command with input 'hello'...");
    
    let response = q_chat_helper::execute_q_subcommand("q", &["translate", "hello"])?;
    
    println!("📝 Translate response: {} bytes", response.len());
    println!("📝 TRANSLATE OUTPUT:");
    println!("{}", response);
    println!("📝 END TRANSLATE OUTPUT");
    
    // Verify translation output contains shell command
    assert!(response.contains("echo") || response.contains("Shell"), "Missing shell command in translation");
    println!("✅ Found shell command translation");
    
    println!("✅ Translate command executed successfully!");
    
    Ok(())
}
