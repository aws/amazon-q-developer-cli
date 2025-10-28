#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

/// Tests the q settings --help subcommand
#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_setting_help_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q settings --help subcommand... | Description: Tests the <code> q settings --help </code> subcommand to validate help output format and content.");
    
    println!("\n🛠️ Running 'q settings --help' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["settings", "--help"])?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Validate help output contains expected sections
    assert!(response.contains("Usage:") && response.contains("q settings") && response.contains("[OPTIONS]") && response.contains("[KEY]") && response.contains("[VALUE]") && response.contains("<COMMAND>"), 
            "Help should contain usage line");
    assert!(response.contains("Commands:"), 
            "Help should contain commands section");
    assert!(response.contains("open") && response.contains("list") && response.contains("help"), 
            "Help should contain all  subcommands related to q setting subcommand");
    assert!(response.contains("Arguments:"), 
            "Help should contain Arguments section");
    assert!(response.contains("Options:"), 
            "Help should contain Options section");
    assert!(response.contains("-d, --delete"), 
            "Help should contain delete option");
    assert!(response.contains("-f, --format <FORMAT>"), 
            "Help should contain format option");
    assert!(response.contains("-v, --verbose"), 
            "Help should contain verbose option");
    assert!(response.contains("-h, --help"), 
            "Should contain help option");
    println!("✅ Help output validated successfully!");
    
    Ok(())
}

/// Tests the q setting all subcommand
#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_settings_all_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q settings all subcommand... | Description: Tests the <code> q settings all </code> subcommand to display all settings.");
    
    println!("\n🛠️ Running 'q settings all' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["settings", "all"])?;

    println!("📝 All settings response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Validate output contains expected settings
    assert!(response.contains("chat.defaultAgent"), "Should contain chat.defaultAgent setting");
    assert!(response.len() > 10, "Response should be substantial");
    
    println!("✅ All settings displayed successfully!");
    
    Ok(())
}

/// Tests the q settings help subcommand
#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_settings_help_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q settings help subcommand... | Description: Tests the <code> q settings help </code> subcommand to validate help output format and content.");
    
    println!("\n🛠️ Running 'q settings help' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["settings", "help"])?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Validate help output contains expected sections
    assert!(response.contains("Usage:") && response.contains("q settings") && response.contains("[OPTIONS]") && response.contains("[KEY]") && response.contains("[VALUE]") && response.contains("<COMMAND>"), 
            "Help should contain usage line");
    assert!(response.contains("Commands:"), 
            "Help should contain commands section");
    assert!(response.contains("open") && response.contains("list") && response.contains("help"), 
            "Help should contain all subcommands related to q setting subcommand");
    assert!(response.contains("Arguments:"), 
            "Help should contain Arguments section");
    assert!(response.contains("Options:"), 
            "Help should contain Options section");
    assert!(response.contains("-d, --delete"), 
            "Help should contain delete option");
    assert!(response.contains("-f, --format <FORMAT>"), 
            "Help should contain format option");
    assert!(response.contains("-v, --verbose"), 
            "Help should contain verbose option");
    assert!(response.contains("-h, --help"), 
            "Should contain help option");
    println!("✅ Help output validated successfully!");
    
    Ok(())
}

