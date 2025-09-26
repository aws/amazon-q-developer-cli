#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

/// Tests the q user subcommand
#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_user_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q user subcommand... | Description: Tests the <code> q user </code> subcommand to display user management help.");
    
    println!("\n🛠️ Running 'q user' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["user"])?;

    println!("📝 User response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Validate output contains expected help information
    assert!(response.contains("Usage:") && response.contains("user") && response.contains("[OPTIONS]") && response.contains("<COMMAND>"), "Should contain usage line");
    assert!(response.contains("Commands:"), "Should contain Commands section");
    assert!(response.contains("login"), "Should contain login command");
    assert!(response.contains("logout"), "Should contain logout command");
    assert!(response.contains("whoami"), "Should contain whoami command");
    assert!(response.contains("profile"), "Should contain profile command");
    assert!(response.contains("Options:"), "Should contain Options section");
    assert!(response.contains("-v, --verbose"), "Should contain verbose option");
    assert!(response.contains("-h, --help"), "Should contain help option");
    
    println!("✅ User command help displayed successfully!");
    
    Ok(())
}