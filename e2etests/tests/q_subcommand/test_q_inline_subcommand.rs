#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_inline_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q inline subcommand... | Description: Tests the <code> q inline </code> subcommand for inline shell completion");   
    
    println!("\n🔍 Executing 'q inline' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["inline"])?;

    println!("📝 Debug response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Assert that q inline shows inline shell completions help
    assert!(response.contains("Inline shell completions"), "Response should contain 'Inline shell completions'");
    assert!(response.contains("enable"), "Response should show 'enable' command");
    assert!(response.contains("disable"), "Response should show 'disable' command");
    assert!(response.contains("status"), "Response should show 'status' command");
    
    println!("✅ q inline subcommand executed successfully!");
    
    Ok(())
}

#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_inline_help_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q inline --help subcommand... | Description: Tests the <code> q inline --help</code> subcommand for inline shell completion");   
    
    println!("\n🔍 Executing 'q inline --help' subcommand...");
    let response = q_chat_helper::execute_q_subcommand_with_stdin("q", &["inline"], Some("--help"))?;

    println!("📝 Debug response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Assert that q inline shows inline shell completions help
    assert!(response.contains("Inline shell completions"), "Response should contain 'Inline shell completions'");
    assert!(response.contains("enable"), "Response should show 'enable' command");
    assert!(response.contains("disable"), "Response should show 'disable' command");
    assert!(response.contains("status"), "Response should show 'status' command");
    
    println!("✅ q inline help subcommand executed successfully!");
    
    Ok(())
}

#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_inline_disable_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q inline disable subcommand... | Description: Tests the <code> q inline disable</code> subcommand for disabling inline");   
    
    println!("\n🔍 Executing 'q inline disable' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["inline", "disable"])?;

    println!("📝 Debug response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Assert that q inline disable shows success message
    assert!(response.contains("Inline disabled"), "Response should contain 'Inline disabled'");
    
    println!("✅ q inline disable subcommand executed successfully!");
    
    Ok(())
}

#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_inline_enable_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q inline enable subcommand... | Description: Tests the <code> q inline enable</code> subcommand for enabling inline");   
    
    println!("\n🔍 Executing 'q inline enable' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["inline", "enable"])?;

    println!("📝 Debug response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Assert that q inline enable shows success message
    assert!(response.contains("Inline enabled"), "Response should contain 'Inline enabled'");
    
    println!("✅ q inline enable subcommand executed successfully!");
    
    Ok(())
}

#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_inline_status_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q inline status subcommand... | Description: Tests the <code> q inline status</code> subcommand for showing inline status");
    
    println!("\n🔍 Executing 'q inline status' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["inline", "status"])?;

    println!("📝 Debug response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Assert that q inline status shows available customizations
    assert!(response.contains("Inline is enabled"), "Response should contain 'Inline is enabled'");

    println!("\n🔍 Executing 'q setting all' subcommand to verify settings...");
    let response = q_chat_helper::execute_q_subcommand("q", &["setting", "all"])?;

    println!("📝 Debug response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    if response.contains("inline.enabled") {
        println!("✅ Verified: inline_enabled is set to true");
    } else {
        println!("❌ Verification failed: inline_enabled is not set to true");
    }

    let response = q_chat_helper::execute_q_subcommand("q", &["settings", "inline.enabled", "--delete"])?;

    println!("📝 Debug response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    assert!(response.contains("Removing") || response.contains("inline.enabled"), "Response should confirm deletion or non-existence of the setting");
    
    println!("✅ q inline status subcommand executed successfully!");
    
    Ok(())
}

#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_inline_show_customizations_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q inline show-customizations subcommand... | Description: Tests the <code> q inline show-customizations</code> that show the available customizations");   
    
    println!("\n🔍 Executing 'q inline show-customizations' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["inline", "show-customizations"])?;

    println!("📝 Debug response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Assert that q inline show-customizations shows available customizations
    assert!(response.contains("Amazon-Internal-V1"), "Response should contain 'Amazon-Internal-V1'");
    assert!(response.contains("Amazon-Aladdin-V1"), "Response should contain 'Amazon-Aladdin-V1'");
    
    println!("✅ q inline show-customizations subcommand executed successfully!");
    
    Ok(())
}