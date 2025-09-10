#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_debug_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q debug subcommand... | Description: Tests the <code> q debug </code> subcommand that provides debugging utilities for the app including app debugging, build switching, logs viewing, and various diagnostic tools.");
    
    println!("\n🔍 Executing 'q debug' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["debug"])?;

    println!("📝 Debug response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Assert debug help output contains expected commands
    assert!(response.contains("Debug the app"), "Response should contain debug description");
    assert!(response.contains("Commands:"), "Response should list available commands");
    assert!(response.contains("app"), "Response should contain 'app' command");
    assert!(response.contains("build"), "Response should contain 'build' command");
    assert!(response.contains("logs"), "Response should contain 'logs' command");

    println!("✅ Got debug help output ({} bytes)!", response.len());
    println!("✅ q debug subcommand executed successfully!");
    
    Ok(())
}

#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_debug_app_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q debug app subcommand... | Description: Tests the <code> q debug app </code> subcommand that provides debugging utilities for the app including app debugging, build switching, logs viewing, and various diagnostic tools.");
    
    println!("\n🔍 Executing 'q debug app' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["debug", "app"])?;

    println!("📝 Debug response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Assert that q debug app launches the Amazon Q interface
    assert!(response.contains("Amazon Q"), "Response should contain 'Amazon Q'");
    assert!(response.contains("🤖 You are chatting with"), "Response should show chat interface");
    
    println!("✅ Got debug app output ({} bytes)!", response.len());
    println!("✅ q debug app subcommand executed successfully!");
    
    Ok(())
}