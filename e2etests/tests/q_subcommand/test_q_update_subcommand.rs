#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;
use regex::Regex;

/// Tests the q update subcommand
#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_update_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q update subcommand... | Description: Tests the <code> q update </code> subcommand to check for updates.");
    
    println!("\n🛠️ Running 'q update' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["update"])?;

    println!("📝 Update response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Validate output contains expected update information
    assert!(response.contains("updates"), "Should contain 'updates'");
    
    // Check for version format (e.g., 1.16.2)
    let version_regex = Regex::new(r"\d+\.\d+\.\d+")?;
    assert!(version_regex.is_match(&response), "Should contain version in format x.y.z");
    
    println!("✅ Update check executed successfully!");
    
    Ok(())
}

/// Tests the q update -h help flag
#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_update_help_flag() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q update -h help flag...");
    
    let response = q_chat_helper::execute_q_subcommand("q", &["update", "-h"])?;
    
    // Verify exact help output format
    assert!(response.contains("Usage:") && response.contains("q update") && response.contains("[OPTIONS]"), "Should contain usage line");
    assert!(response.contains("-y, --non-interactive"), "Should contain non-interactive option");
    assert!(response.contains("--relaunch-dashboard"), "Should contain relaunch-dashboard option");
    assert!(response.contains("--rollout"), "Should contain rollout option");
    assert!(response.contains("-v, --verbose..."), "Should contain verbose option");
    assert!(response.contains("-h, --help"), "Should contain help option");
    
    println!("✅ Update help flag test passed!");
    Ok(())
}