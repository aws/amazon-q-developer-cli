#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

/// Tests the q whoami subcommand
#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_whoami_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q whoami subcommand... | Description: Tests the <code> q whoami </code> subcommand to display user profile information.");
    
    println!("\n🛠️ Running 'q whoami' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["whoami"])?;

    println!("📝 Whoami response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Validate output contains expected authentication information
    assert!(response.contains("Logged"), "Should contain IAM Identity Center login info");
    assert!(response.contains("Profile:"), "Should contain Profile section");
    
    println!("✅ Whoami information displayed successfully!");
    
    Ok(())
}