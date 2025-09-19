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

#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_whoami_help_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q whoami --help subcommand... | Description: Tests the <code> q whoami --help</code> subcommand to validate help output format and content.");
    
    println!("\n🔍 Executing 'q whoami --help' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["whoami", "--help"])?;

    println!("📝 whoami response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Assert whoami help output contains expected commands
    assert!(response.contains("Usage:") && response.contains("q whoami") && response.contains("[OPTIONS]"), 
            "Help should contain usage line");
    assert!(response.contains("Options:"), "Help should contain Options section");
    assert!(response.contains("-f, --format"), "Help should contain format option");
    assert!(response.contains("-v, --verbose"), "Help should contain verbose option");
    assert!(response.contains("-h, --help"), "Should contain help option");

    println!("✅ Got whoami help output ({} bytes)!", response.len());
    println!("✅ q whoami --help subcommand executed successfully!");
    
    Ok(())
}

#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_whoami_f_plain_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q whoami  -f plain subcommand... | Description: Tests the <code> q whoami  -f plain</code> subcommand to display user profile information in plain format.");
    
    println!("\n🛠️ Running 'q whoami  -f plain' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["whoami", "-f", "plain"])?;

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

#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_whoami_f_json_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q whoami  -f json subcommand... | Description: Tests the <code> q whoami  -f json</code> subcommand to display user profile information in json format.");
    
    println!("\n🛠️ Running 'q whoami  -f json' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["whoami", "-f", "json"])?;

    println!("📝 Whoami response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Check if accountType and region appear between { and }
    let start = response.find('{').unwrap();
    let end = response.rfind('}').unwrap();
    let json_content = &response[start..=end];
    assert!(json_content.contains("accountType"));
    assert!(json_content.contains("region"));
    
    // Validate JSON is single-line format
    assert!(!json_content[1..json_content.len()-1].contains('\n'), "JSON should be in single-line format");

    // Validate output contains expected authentication information
    assert!(response.contains("Profile:"), "Should contain Profile section");
    
    println!("✅ Whoami information displayed successfully!");
    
    Ok(())
}

#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_whoami_f_json_pretty_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q whoami -f json-pretty subcommand... | Description: Tests the <code> q whoami -f json-pretty</code> subcommand to display user profile information in pretty json format.");
    
    println!("\n🛠️ Running 'q whoami -f json-pretty' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["whoami", "-f", "json-pretty"])?;

    println!("📝 Whoami response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Check if accountType and region appear between { and }
    let start = response.find('{').unwrap();
    let end = response.rfind('}').unwrap();
    let json_content = &response[start..=end];
    assert!(json_content.contains("accountType"));
    assert!(json_content.contains("region"));

    // Validate output contains expected authentication information
    assert!(response.contains("Profile:"), "Should contain Profile section");
    
    println!("✅ Whoami information displayed successfully!");
    
    Ok(())
}